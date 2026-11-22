//! Strict loopback `Origin` allowlisting for localhost WebSocket upgrades.

use crate::local_server::dto::error_response;
use crate::local_server::http::HttpResponse;
use axum::http::{header::ORIGIN, HeaderMap, Uri};
use std::net::IpAddr;

/// Validates that a single HTTP upgrade `Origin` header is present and strictly loopback.
pub(super) fn validate_loopback_origin(headers: &HeaderMap) -> Result<(), HttpResponse> {
    let mut origins = headers.get_all(ORIGIN).iter();
    let first = origins.next().ok_or_else(forbidden_origin)?;
    if origins.next().is_some() {
        return Err(forbidden_origin());
    }
    let raw = first
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(forbidden_origin)?;
    let uri = raw.parse::<Uri>().map_err(|_| forbidden_origin())?;
    is_allowed_origin_uri(&uri)
        .then_some(())
        .ok_or_else(forbidden_origin)
}

fn is_allowed_origin_uri(uri: &Uri) -> bool {
    let Some(auth) = uri.authority() else {
        return false;
    };
    if auth.as_str().contains('@') || !matches!(uri.path(), "" | "/") || uri.query().is_some() {
        return false;
    }
    match uri.scheme_str().map(str::to_ascii_lowercase).as_deref() {
        Some("tauri") => auth.host().eq_ignore_ascii_case("localhost"),
        Some("http") | Some("https") => is_loopback_host(auth.host()),
        _ => false,
    }
}

fn is_loopback_host(host: &str) -> bool {
    let clean = host.trim_matches(|c| c == '[' || c == ']');
    clean.eq_ignore_ascii_case("localhost")
        || clean.eq_ignore_ascii_case("tauri.localhost")
        || clean.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn forbidden_origin() -> HttpResponse {
    error_response(403, "Forbidden", "WebSocket origin is not allowlisted.")
}
