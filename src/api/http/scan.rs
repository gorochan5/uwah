use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use qrcode::{QrCode, render::svg};
use serde::Deserialize;
use std::sync::Arc;

use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::sessions::Session;
use crate::repository::sessions as repo;
use crate::security::verify_token;

#[derive(Deserialize, Default)]
pub struct ScanQuery {
    pub token: Option<String>,
}

/// Generates a clean, standalone vector SVG string for the QR payload.
pub fn render_qr_svg(payload: &str) -> Result<String, String> {
    let code =
        QrCode::new(payload.as_bytes()).map_err(|e| format!("Failed to encode QR payload: {e}"))?;

    let svg_str = code
        .render::<svg::Color>()
        .min_dimensions(260, 260)
        .quiet_zone(true)
        .dark_color(svg::Color("#0f172a"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_str)
}

fn check_auth(state: &AppState, query_token: Option<&str>, headers: &HeaderMap) -> bool {
    if query_token.is_some_and(|t| verify_token(t, &state.config.service.auth)) {
        return true;
    }

    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        return verify_token(token.trim(), &state.config.service.auth);
    }

    false
}

/// GET /sessions/{id}/scan
/// Full standalone HTML page with HTMX polling container.
#[utoipa::path(
    get,
    path = "/sessions/{id}/scan",
    tag = "Sessions",
    summary = "Render interactive QR pairing page",
    description = "Renders a responsive Maud + HTMX HTML page that displays and auto-refreshes the WhatsApp pairing QR code.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("token" = Option<String>, Query, description = "Authentication token when opening directly in a browser")
    ),
    responses(
        (status = 200, description = "Standalone HTML pairing page with HTMX live QR updates", content_type = "text/html"),
        (status = 401, description = "Unauthorized - invalid or missing credentials", content_type = "text/html"),
        (status = 404, description = "Session not found", body = ErrorResponse)
    )
)]
pub async fn scan_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ScanQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !check_auth(&state, query.token.as_deref(), &headers) {
        return Ok((
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            render_unauthorized_page(&id).into_string(),
        )
            .into_response());
    }

    let session = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    // Self-healing: auto-start WhatsApp bot worker if not yet running
    if session.status != "CONNECTED" && !state.sessions.is_running(&id).await {
        let sid = id.clone();
        let sm = state.sessions.clone();
        tokio::spawn(async move {
            let _ = sm.start_session(sid).await;
        });
    }

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        scan_page_template(&session, query.token.as_deref()).into_string(),
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/qr-frame",
    tag = "Sessions",
    summary = "HTMX QR frame fragment",
    description = "Polled dynamic HTMX HTML fragment displaying the real-time QR code or connected status. Returns HTTP 286 to stop polling once connected.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("token" = Option<String>, Query, description = "Authentication token when opening directly in a browser")
    ),
    responses(
        (status = 200, description = "HTML fragment with current QR code", content_type = "text/html"),
        (status = 286, description = "HTMX stop polling: session is connected", content_type = "text/html"),
        (status = 401, description = "Unauthorized", content_type = "text/html"),
        (status = 404, description = "Session not found", body = ErrorResponse)
    )
)]
pub async fn qr_frame(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<ScanQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if !check_auth(&state, query.token.as_deref(), &headers) {
        return Ok((
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html! {
                div class="p-4 text-center text-rose-500 font-medium text-xs" {
                    "Session unauthorized. Please refresh with a valid token."
                }
            }
            .into_string(),
        )
            .into_response());
    }

    let session = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    // Self-healing: auto-start WhatsApp bot worker if not yet running
    if session.status != "CONNECTED" && !state.sessions.is_running(&id).await {
        let sid = id.clone();
        let sm = state.sessions.clone();
        tokio::spawn(async move {
            let _ = sm.start_session(sid).await;
        });
    }

    if session.status == "CONNECTED" {
        let stop_status = StatusCode::from_u16(286).unwrap_or(StatusCode::OK);
        let markup = render_connected_card(&session);
        return Ok((
            stop_status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            markup.into_string(),
        )
            .into_response());
    }

    let token_param = query
        .token
        .as_deref()
        .map(|t| format!("?token={}", t))
        .unwrap_or_default();
    let poll_url = format!("/sessions/{}/qr-frame{}", session.id, token_param);

    let markup = render_qr_frame_content(&session, &poll_url);
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        markup.into_string(),
    )
        .into_response())
}

/// Full HTML page template using Tailwind & HTMX
pub fn scan_page_template(session: &Session, auth_token: Option<&str>) -> Markup {
    let title = format!("Link WhatsApp — {}", session.id);
    let token_param = auth_token
        .map(|t| format!("?token={}", t))
        .unwrap_or_default();
    let qr_frame_url = format!("/sessions/{}/qr-frame{}", session.id, token_param);

    html! {
        (DOCTYPE)
        html lang="en" class="h-full bg-slate-900 text-slate-100 antialiased" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) }

                // Tailwind CSS & HTMX
                script src="https://cdn.tailwindcss.com" {}
                script src="https://unpkg.com/htmx.org@2.0.4" {}

                style {
                    (PreEscaped(r#"
                        #qr-frame svg {
                            display: block;
                            width: 100%;
                            height: 100%;
                            max-width: 260px;
                            max-height: 260px;
                            border-radius: 0.5rem;
                        }
                    "#))
                }
            }

            body class="min-h-full flex flex-col items-center justify-center p-4 sm:p-6 lg:p-8" {
                main class="w-full max-w-md bg-slate-800/95 backdrop-blur border border-slate-700/60 rounded-2xl shadow-2xl p-6 sm:p-8 space-y-6" {

                    // Header
                    header class="text-center space-y-2" {
                        div class="inline-flex items-center space-x-2 px-3 py-1 rounded-full bg-slate-700/50 border border-slate-600/50 text-xs font-mono text-emerald-400" {
                            span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" {}
                            span { "Session: " (session.id) }
                        }
                        h1 class="text-2xl font-bold tracking-tight text-white" {
                            "Link WhatsApp Account"
                        }
                        p class="text-xs text-slate-400" {
                            "Scan the QR code below using WhatsApp on your primary phone."
                        }
                    }

                    // QR Viewport (HTMX polling target)
                    div class="flex justify-center" {
                        div id="qr-frame"
                            class="w-[290px] min-h-[290px] flex items-center justify-center bg-white rounded-xl shadow-inner border border-slate-700 p-3"
                            hx-get=(qr_frame_url)
                            hx-trigger="every 2s"
                            hx-swap="innerHTML"
                        {
                            (render_qr_frame_content(session, &qr_frame_url))
                        }
                    }

                    // Mobile Instructions
                    section class="bg-slate-900/60 rounded-xl p-4 border border-slate-700/40 text-xs text-slate-300 space-y-2.5" {
                        p class="font-semibold text-slate-200" { "Instructions:" }
                        ol class="list-decimal list-inside space-y-1 text-slate-400 leading-relaxed" {
                            li { "Open " span class="text-slate-200 font-medium" { "WhatsApp" } " on your phone" }
                            li { "Go to " span class="text-slate-200 font-medium" { "Settings > Linked Devices" } }
                            li { "Tap " span class="text-slate-200 font-medium" { "Link a Device" } }
                            li { "Point your phone's camera at this screen" }
                        }
                    }

                    footer class="pt-2 border-t border-slate-700/40 text-center text-[11px] text-slate-500 font-mono" {
                        "uwah gateway • real-time session pairing"
                    }
                }
            }
        }
    }
}

/// Fragment rendering active QR code or transient spinner
pub fn render_qr_frame_content(session: &Session, _poll_url: &str) -> Markup {
    match session.status.as_str() {
        "CONNECTED" => render_connected_card(session),
        "QRCODE" => {
            if let Some(ref payload) = session.qr_payload {
                if !payload.trim().is_empty() {
                    render_qr_code(payload)
                } else {
                    render_transient_spinner("Awaiting QR code payload...")
                }
            } else {
                render_transient_spinner("Generating WhatsApp pairing key...")
            }
        }
        status => render_status_notice(status),
    }
}

fn render_qr_code(payload: &str) -> Markup {
    match render_qr_svg(payload) {
        Ok(svg_xml) => html! {
            div class="flex flex-col items-center justify-center space-y-3" {
                div class="p-1 bg-white rounded-lg" {
                    (PreEscaped(svg_xml))
                }
                div class="flex items-center space-x-2 text-[11px] font-medium text-slate-600" {
                    span class="relative flex h-2 w-2" {
                        span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" {}
                        span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500" {}
                    }
                    span { "Auto-refreshes every 2s" }
                }
            }
        },
        Err(err) => html! {
            div class="flex flex-col items-center justify-center p-4 text-center space-y-2 text-rose-600" {
                p class="text-xs font-semibold" { "Failed to build QR code" }
                p class="text-[10px] text-slate-400 font-mono" { (err) }
            }
        },
    }
}

fn render_transient_spinner(message: &str) -> Markup {
    html! {
        div class="flex flex-col items-center justify-center p-6 text-center space-y-4 text-slate-700" {
            div class="w-10 h-10 rounded-full border-4 border-slate-200 border-t-emerald-600 animate-spin" {}
            div class="space-y-1" {
                p class="text-xs font-semibold text-slate-800" { (message) }
                p class="text-[11px] text-slate-400" { "Exchanging pairing handshake with WhatsApp..." }
            }
        }
    }
}

pub fn render_connected_card(session: &Session) -> Markup {
    let phone = session.phone_number.as_deref().unwrap_or("Linked Device");
    let name = session.push_name.as_deref();

    html! {
        div class="w-full flex flex-col items-center justify-center p-6 text-center space-y-4" {
            div class="w-14 h-14 rounded-full bg-emerald-100 text-emerald-600 flex items-center justify-center shadow-md ring-8 ring-emerald-50" {
                svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24" {
                    path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" {}
                }
            }

            div class="space-y-1" {
                h2 class="text-base font-bold text-slate-900" { "Session Connected!" }
                p class="text-xs text-slate-500" { "Your WhatsApp account is paired and active." }
            }

            div class="w-full bg-slate-50 rounded-lg border border-slate-200 p-2.5 text-xs text-slate-700 space-y-1 text-left" {
                div class="flex justify-between" {
                    span class="text-slate-400" { "Phone / JID:" }
                    span class="font-mono font-medium text-slate-900" { (phone) }
                }
                @if let Some(push_name) = name {
                    div class="flex justify-between" {
                        span class="text-slate-400" { "Push Name:" }
                        span class="font-medium text-slate-900" { (push_name) }
                    }
                }
                div class="flex justify-between" {
                    span class="text-slate-400" { "Status:" }
                    span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-semibold bg-emerald-100 text-emerald-800" {
                        "CONNECTED"
                    }
                }
            }

            p class="text-[11px] text-slate-400" {
                "Polling stopped. You may now close this browser window."
            }
        }
    }
}

fn render_status_notice(status: &str) -> Markup {
    html! {
        div class="flex flex-col items-center justify-center p-6 text-center space-y-3 text-slate-600" {
            p class="text-xs font-semibold text-slate-800" { "Session Status: " (status) }
            p class="text-[11px] text-slate-400" { "Waiting for session state update..." }
        }
    }
}

fn render_unauthorized_page(session_id: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" class="h-full bg-slate-900 text-slate-100 antialiased" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "Unauthorized — Uwah" }
                script src="https://cdn.tailwindcss.com" {}
            }
            body class="min-h-full flex items-center justify-center p-4" {
                div class="w-full max-w-sm bg-slate-800 border border-slate-700 rounded-xl p-6 text-center space-y-4 shadow-xl" {
                    div class="w-12 h-12 rounded-full bg-rose-500/20 text-rose-400 flex items-center justify-center mx-auto" {
                        svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" {
                            path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" {}
                        }
                    }
                    h1 class="text-lg font-bold text-white" { "Authentication Required" }
                    p class="text-xs text-slate-400" {
                        "This scan endpoint is protected. Please append your auth token to the URL query string:"
                    }
                    div class="bg-slate-900 p-2.5 rounded text-[11px] font-mono text-emerald-400 break-all text-left" {
                        "/sessions/" (session_id) "/scan?token=YOUR_TOKEN"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{render_connected_card, render_qr_frame_content, render_qr_svg, scan_page_template};
    use crate::models::sessions::Session;

    fn session(status: &str, qr_payload: Option<&str>) -> Session {
        let now = chrono::Utc::now();
        Session {
            id: "session-1".into(),
            phone_number: None,
            push_name: None,
            status: status.into(),
            qr_payload: qr_payload.map(str::to_owned),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn renders_valid_svg_for_qr_payload() {
        let svg = render_qr_svg("pairing-payload").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("#0f172a"));
    }

    #[test]
    fn qr_frame_selects_payload_waiting_and_status_views() {
        let qr = render_qr_frame_content(&session("QRCODE", Some("payload")), "").into_string();
        assert!(qr.contains("<svg"));

        let waiting = render_qr_frame_content(&session("QRCODE", None), "").into_string();
        assert!(waiting.contains("Generating WhatsApp pairing key"));

        let status = render_qr_frame_content(&session("STARTING", None), "").into_string();
        assert!(status.contains("STARTING"));
    }

    #[test]
    fn connected_card_and_page_include_session_details() {
        let mut connected = session("CONNECTED", None);
        connected.phone_number = Some("+5511999999999".into());
        connected.push_name = Some("Alice".into());
        let card = render_connected_card(&connected).into_string();
        assert!(card.contains("+5511999999999"));
        assert!(card.contains("Alice"));

        let page = scan_page_template(&connected, Some("token-123")).into_string();
        assert!(page.contains("session-1"));
        assert!(page.contains("?token=token-123"));
    }
}
