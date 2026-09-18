use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub database: String,
}
