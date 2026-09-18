use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
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

fn check_query_auth(query_str: &str, expected: &str) -> bool {
    for param in query_str.split('&') {
        if let Some((k, v)) = param.split_once('=') {
            let is_auth_key = k == "key" || k == "token";
            if is_auth_key && verify_token(v, expected) {
                return true;
            }
        }
    }
    false
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

pub async fn swagger_auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // 1. Check Authorization: Bearer <token>
    let auth_token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|val| val.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let is_token_valid = auth_token
        .map(|token| verify_token(token.trim(), &state.config.service.auth))
        .unwrap_or(false);

    // 2. Check query parameter `key` or `token` (e.g. /swagger?key=123)
    let is_query_valid = request
        .uri()
        .query()
        .map(|q| check_query_auth(q, &state.config.service.auth))
        .unwrap_or(false);

    // 3. Check Referer header for nested asset / openapi.json requests initiated from an authenticated Swagger UI page
    let referer_query = request
        .headers()
        .get(axum::http::header::REFERER)
        .and_then(|val| val.to_str().ok())
        .and_then(|ref_url| ref_url.split_once('?'))
        .map(|(_, q)| q);

    let is_referer_valid = referer_query
        .map(|q| check_query_auth(q, &state.config.service.auth))
        .unwrap_or(false);

    let is_authenticated = is_token_valid || is_query_valid || is_referer_valid;

    if is_authenticated {
        // If the path is exactly "/swagger", redirect to "/swagger/" while preserving query string parameters (e.g. ?key=123)
        // using an HTTP 308 Permanent Redirect. This ensures browser requests like http://127.0.0.1:6767/swagger?key=123
        // seamlessly redirect to http://127.0.0.1:6767/swagger/?key=123 without stripping the authentication key.
        if request.uri().path() == "/swagger" {
            let query = request
                .uri()
                .query()
                .map(|q| format!("?{}", q))
                .unwrap_or_default();
            return Ok(Redirect::permanent(&format!("/swagger/{}", query)).into_response());
        }
        return Ok(next.run(request).await);
    }

    Err(AppError::Unauthorized)
}
