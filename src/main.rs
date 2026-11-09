#![allow(dead_code)]

use std::{error::Error, sync::Arc};
use tracing::info;

mod api;
mod config;
mod database;
pub mod events;
mod log;
pub mod media;
mod models;
mod repository;
mod security;
pub mod traffic;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _guard = crate::log::init();
    info!("Starting Uwah WhatsApp Gateway...");

    crate::config::init()?;
    let config = crate::config::get();

    let pool = crate::database::init().await?;
    info!("Database pool successfully connected.");

    let session_manager = Arc::new(crate::api::session_manager::SessionManager::new(
        &config.sessions.dir,
    ));
    let event_bus = crate::events::bus::global().clone();

    let state = Arc::new(crate::api::http::state::AppState::new(
        pool.clone(),
        config,
        session_manager.clone(),
        event_bus,
    ));
    let app = crate::api::http::create_router(state.clone());

    // 1. Gateway Auto-Boot Hydration (Track 1.5)
    auto_boot_sessions(session_manager.clone()).await;

    // 2. Supervised Webhook Outbox Dispatcher Daemon (Track 4.2)
    let outbox_cancel = tokio_util::sync::CancellationToken::new();
    let outbox_handle = crate::events::outbox_worker::start_outbox_daemon(outbox_cancel.clone());

    // 3. Background Database Maintenance Daemon (Track 3.6)
    let maint_pool = pool.clone();
    let maint_cancel = tokio_util::sync::CancellationToken::new();
    let maint_cancel_child = maint_cancel.clone();
    let maint_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800)); // Every 30 mins
        loop {
            tokio::select! {
                _ = maint_cancel_child.cancelled() => break,
                _ = interval.tick() => {
                    info!("Running periodic SQLite WAL checkpoint and incremental vacuum");
                    let _ = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)").execute(&maint_pool).await;
                    let _ = sqlx::query("PRAGMA incremental_vacuum(500)").execute(&maint_pool).await;
                }
            }
        }
    });

    let host = if config.service.host == "localhost" {
        "127.0.0.1"
    } else {
        &config.service.host
    };

    // 4. gRPC Microservices Interconnect Server (Track 14)
    let grpc_cancel = tokio_util::sync::CancellationToken::new();
    let grpc_cancel_child = grpc_cancel.clone();
    let grpc_state = state.clone();
    let grpc_port = config.service.port + 1;
    let grpc_addr: std::net::SocketAddr = format!("{}:{}", host, grpc_port).parse()?;
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = crate::api::grpc::start_grpc_server(grpc_state, grpc_addr, grpc_cancel_child).await {
            tracing::error!("gRPC server error: {}", e);
        }
    });

    let bind_addr = format!("{}:{}", host, config.service.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Uwah REST API listening on http://{}", bind_addr);
    info!("Uwah gRPC Microservices Interconnect listening on {}", grpc_addr);

    // 5. Deterministic Graceful Shutdown Pipeline (Track 1.4)
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Graceful shutdown initiated. Stopping background tasks...");
    grpc_cancel.cancel();
    outbox_cancel.cancel();
    maint_cancel.cancel();
    let _ = tokio::join!(outbox_handle, maint_handle, grpc_handle);

    // Disconnect and terminate all active WhatsApp sessions
    for session_id in session_manager.list_active_ids().await {
        session_manager.stop_session(&session_id).await;
    }

    info!("Closing database connection pool...");
    pool.close().await;
    info!("Uwah Gateway shutdown completed successfully.");

    Ok(())
}

async fn auto_boot_sessions(
    session_manager: Arc<crate::api::session_manager::SessionManager>,
) {
    let mut tasks = tokio::task::JoinSet::new();
    info!("Checking database for CONNECTED sessions to auto-hydrate...");
    match crate::repository::sessions::get_all().await {
        Ok(sessions) => {
            let connected_sessions: Vec<_> = sessions
                .into_iter()
                .filter(|s| s.status == "CONNECTED")
                .collect();

            if connected_sessions.is_empty() {
                info!("No active sessions found for auto-boot");
                return;
            }

            info!(
                count = connected_sessions.len(),
                "Auto-booting active WhatsApp sessions..."
            );

            let semaphore = Arc::new(tokio::sync::Semaphore::new(5));
            for session in connected_sessions {
                let sem = Arc::clone(&semaphore);
                let mgr = Arc::clone(&session_manager);
                let sid = session.id.clone();
                tasks.spawn(async move {
                    let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    info!(session_id = %sid, "Auto-booting WhatsApp session");
                    if let Err(e) = mgr.start_session(sid.clone()).await {
                        tracing::warn!(session_id = %sid, error = %e, "Auto-boot handshake failed");
                    }
                });
            }
            while let Some(result) = tasks.join_next().await {
                if let Err(err) = result {
                    tracing::warn!(error = %err, "Auto-boot task failed");
                }
            }
        }
        Err(err) => {
            tracing::error!(error = %err, "Failed to load sessions for auto-boot");
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C shutdown signal");
        }
        _ = terminate => {
            info!("Received SIGTERM shutdown signal");
        }
    }
}
