#![allow(dead_code)]

use std::{error::Error, sync::Arc};
use tracing::info;

mod api;
mod config;
mod database;
mod log;
mod models;
mod repository;
mod security;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _guard = crate::log::init();
    info!("Starting Uwah WhatsApp Gateway...");

    crate::config::init()?;
    let config = crate::config::get();

    let pool = crate::database::init().await?;
    info!("Database pool successfully connected.");

    let session_manager = Arc::new(crate::api::session_manager::SessionManager::new(&config.sessions.dir));
    let state = Arc::new(crate::api::http::state::AppState::new(pool, config, session_manager));
    let app = crate::api::http::create_router(state);

    let host = if config.service.host == "localhost" {
        "127.0.0.1"
    } else {
        &config.service.host
    };
    let bind_addr = format!("{}:{}", host, config.service.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Uwah REST API listening on http://{}", bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
