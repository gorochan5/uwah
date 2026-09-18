use crate::api::http::{error::AppError, state::AppState};
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub type HmacSha256 = Hmac<Sha256>;

pub const SIGNATURE_HEADER: &str = "X-Uwah-Signature";

/// Compares two byte slices in constant time.
/// Returns false if lengths differ without leaking timing information.
#[inline]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// Compares two strings in constant time.
/// Pre-hashes both inputs with SHA-256 to a fixed 32-byte digest before comparison,
/// preventing secret length leakage via side-channel timing analysis.
pub fn constant_time_eq_str(a: &str, b: &str) -> bool {
    let hash_a = Sha256::digest(a.as_bytes());
    let hash_b = Sha256::digest(b.as_bytes());
    hash_a.ct_eq(&hash_b).into()
}

/// Signs webhook payload bytes with HMAC-SHA256.
/// Output format: `sha256=<64_hex_digits>` suitable for header `X-Uwah-Signature`.
pub fn sign_payload(secret: &str, payload: &[u8]) -> Result<String, AppError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to initialize HMAC: {}", e)))?;
    mac.update(payload);
    let signature_bytes = mac.finalize().into_bytes();
    Ok(format!("sha256={}", hex::encode(signature_bytes)))
}

/// Verifies an incoming `X-Uwah-Signature` header value in constant time.
pub fn verify_signature(secret: &str, payload: &[u8], signature_header: &str) -> bool {
    let expected_hex = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header);
    let Ok(expected_bytes) = hex::decode(expected_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);
    mac.verify_slice(&expected_bytes).is_ok()
}

/// Hardened token verification preventing timing side-channel attacks.
pub fn verify_token(token: &str, expected: &str) -> bool {
    // 1. Direct plaintext match using constant-time hash comparison
    if constant_time_eq_str(token, expected) {
        return true;
    }

    // 2. SHA-256 hash match (e.g. if expected is configured as a 64-char hex digest)
    let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
    constant_time_eq_str(&token_hash.to_lowercase(), &expected.to_lowercase())
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
