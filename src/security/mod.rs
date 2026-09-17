use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use crate::api::http::{error::AppError, state::AppState};

pub fn verify_token(token: &str, expected: &str) -> bool {
    // 1. Direct plaintext match (e.g. auth = "123" in config)
    if token == expected {
        return true;
    }

    // 2. SHA-256 hash match (e.g. hex digest in config)
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hash_bytes = hasher.finalize();
    let token_hash = hash_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    token_hash.eq_ignore_ascii_case(expected)
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|val| val.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => header[7..].trim(),
        _ => return Err(AppError::Unauthorized),
    };

    if verify_token(token, &state.config.service.auth) {
        Ok(next.run(request).await)
    } else {
        Err(AppError::Unauthorized)
    }
}
