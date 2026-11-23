//! Axum HTTP response adapter, payload-limit error wrapper, and CORS/close middleware.

use crate::local_server::dto::error_response;
use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    CONNECTION, CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub(crate) const MAX_BRIDGE_BODY_BYTES: usize = 256 * 1024;

/// Outgoing HTTP JSON or empty response produced by local bridge handlers.
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    /// Creates a JSON HTTP response with the provided status code and serialized payload.
    pub fn json(status: u16, _reason: &'static str, body: String) -> Self {
        Self { status, body }
    }

    /// Creates a `204 No Content` response for CORS preflight requests.
    pub fn no_content() -> Self {
        Self {
            status: 204,
            body: String::new(),
        }
    }
}

impl IntoResponse for HttpResponse {
    fn into_response(self) -> Response {
        let code = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if self.status == 204 {
            return (code, Body::empty()).into_response();
        }
        Response::builder()
            .status(code)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(self.body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// Handles `OPTIONS` preflight, JSON 413 formatting, CORS headers, and `Connection: close`.
pub(super) async fn apply_bridge_middleware(req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        let mut resp = HttpResponse::no_content().into_response();
        attach_cors_and_close(resp.headers_mut());
        return resp;
    }
    let mut resp = next.run(req).await;
    if resp.status() == StatusCode::PAYLOAD_TOO_LARGE {
        resp = error_response(
            413,
            "Payload Too Large",
            "Request body exceeds 256 KB limit.",
        )
        .into_response();
    }
    if resp.status() != StatusCode::SWITCHING_PROTOCOLS {
        attach_cors_and_close(resp.headers_mut());
    }
    resp
}

fn attach_cors_and_close(headers: &mut HeaderMap) {
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PATCH, DELETE, OPTIONS"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "Content-Type, X-Learner-Id, X-Byok-Groq, X-Byok-Gemini, X-Byok-Openrouter, X-Byok-Freellmapi",
        ),
    );
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
}
