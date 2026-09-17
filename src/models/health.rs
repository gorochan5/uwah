use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub database: String,
}

