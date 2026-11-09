use super::{create_router, state::AppState};
use crate::{
    api::session_manager::SessionManager,
    config::{Config, Database, Filter, Media, Service, Sessions},
    events::bus::GatewayEventBus,
};
use axum::{body::Body, http::{Request, StatusCode}, Router};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

async fn test_app() -> (Router, TempDir, SqlitePool) {
    let temp_dir = tempfile::tempdir().expect("create isolated test directory");
    let database_path = temp_dir.path().join("integration.sqlite");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&database_path)
                .create_if_missing(true),
        )
        .await
        .expect("connect to isolated test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply project migrations to isolated test database");

    let config = Box::leak(Box::new(Config {
        database: Database { dir: database_path.to_string_lossy().into_owned() },
        service: Service { host: "127.0.0.1".into(), port: 6767, auth: "integration-test-token".into() },
        media: Media {
            dir: temp_dir.path().join("media").to_string_lossy().into_owned(),
            cache_ttl_seconds: Some(3600),
            max_file_size: Some(1024 * 1024),
            max_concurrent_downloads: Some(1),
        },
        sessions: Sessions { dir: temp_dir.path().join("sessions").to_string_lossy().into_owned() },
        webhook: None,
        filter: Filter::default(),
    }));
    let state = Arc::new(AppState::new(
        pool.clone(),
        config,
        Arc::new(SessionManager::new(&config.sessions.dir)),
        GatewayEventBus::new(32),
    ));

    (create_router(state), temp_dir, pool)
}

async fn get(app: Router, path: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("build request"),
    )
    .await
    .expect("router should return a response")
}

#[tokio::test]
async fn health_endpoints_report_migrated_database_and_ready_storage() {
    let (app, _temp_dir, pool) = test_app().await;

    let health = get(app.clone(), "/health").await;
    assert_eq!(health.status(), StatusCode::OK);
    let health_body = axum::body::to_bytes(health.into_body(), 1024 * 1024).await.unwrap();
    let health_json: serde_json::Value = serde_json::from_slice(&health_body).unwrap();
    assert_eq!(health_json["database"], "connected");

    let readiness = get(app.clone(), "/health/ready").await;
    assert_eq!(readiness.status(), StatusCode::OK);
    let readiness_body = axum::body::to_bytes(readiness.into_body(), 1024 * 1024).await.unwrap();
    let readiness_json: serde_json::Value = serde_json::from_slice(&readiness_body).unwrap();
    assert_eq!(readiness_json["status"], "ready");
    assert_eq!(readiness_json["storage"], "accessible");

    let startup = get(app, "/health/startup").await;
    assert_eq!(startup.status(), StatusCode::OK);
    let startup_body = axum::body::to_bytes(startup.into_body(), 1024 * 1024).await.unwrap();
    let startup_json: serde_json::Value = serde_json::from_slice(&startup_body).unwrap();
    assert_eq!(startup_json["initialized"], true);

    pool.close().await;
}

#[tokio::test]
async fn protected_api_rejects_missing_and_invalid_bearer_tokens() {
    let (app, _temp_dir, pool) = test_app().await;

    let missing = get(app.clone(), "/sessions").await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);

    pool.close().await;
}
