use crate::api::session_manager::SessionManager;
use crate::config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: &'static Config,
    pub sessions: Arc<SessionManager>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: &'static Config, sessions: Arc<SessionManager>) -> Self {
        Self {
            pool,
            config,
            sessions,
        }
    }
}
