use crate::api::session_manager::SessionManager;
use crate::config::Config;
use crate::events::bus::GatewayEventBus;
use crate::traffic::TrafficShaper;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: &'static Config,
    pub sessions: Arc<SessionManager>,
    pub event_bus: GatewayEventBus,
    pub traffic: TrafficShaper,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        config: &'static Config,
        sessions: Arc<SessionManager>,
        event_bus: GatewayEventBus,
    ) -> Self {
        Self {
            pool,
            config,
            sessions,
            event_bus,
            traffic: TrafficShaper::default(),
        }
    }
}
