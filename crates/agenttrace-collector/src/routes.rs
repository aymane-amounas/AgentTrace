//! HTTP API for ingestion, querying, export and the viewer.
//!
//! Routes:
//! * `POST /v1/spans`            - ingest spans (`{"spans":[...]}`, an array, or one span)
//! * `GET  /v1/traces`          - list trace summaries (filterable)
//! * `GET  /v1/traces/:id`      - all spans of a trace (detail + replay)
//! * `GET  /v1/traces/:id/export?format=native|otlp`
//! * `GET  /v1/stats`           - overview metrics
//! * `GET  /v1/config`          - effective privacy/runtime settings (read-only)
//! * `GET  /healthz`            - health check
//! * `GET  /*`                  - the built viewer (SPA), if present

use crate::config::Config;
use crate::storage::{Storage, TraceFilter};
use agenttrace::span::SpanData;
use axum::http::{HeaderValue, Method};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub config: Arc<Config>,
}

/// Build the application router.
pub fn app(state: AppState) -> Router {
    let max_body = state.config.max_body_bytes;
    let viewer_dir = state.config.viewer_dir.clone();

    let api = Router::new()
        .route("/healthz", get(health))
        .route("/v1/spans", post(ingest))
        .route("/v1/traces", get(list_traces))
        .route("/v1/traces/:trace_id", get(get_trace))
        .route("/v1/traces/:trace_id/export", get(export_trace))
        .route("/v1/stats", get(stats))
        .route("/v1/config", get(runtime_config))
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(max_body))
        // Same-origin (the collector serves the built viewer) needs no CORS. For
        // the dev server we allow only local origins, so a page on some other
        // site the user is browsing cannot read their traces off localhost.
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _req| {
                    is_local_origin(origin)
                }))
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(tower_http::cors::Any),
        )
        .layer(axum::middleware::from_fn(security_headers));

    // Serve the built viewer if it exists, with SPA fallback to index.html.
    let index = viewer_dir.join("index.html");
    if viewer_dir.is_dir() && index.is_file() {
        use tower_http::services::{ServeDir, ServeFile};
        let serve = ServeDir::new(&viewer_dir).fallback(ServeFile::new(index));
        api.fallback_service(serve)
    } else {
        api.fallback(get(no_viewer))
    }
}

/// Allow cross-origin requests only from a loopback origin (the viewer dev
/// server). Matches `http(s)://localhost`, `127.0.0.1` or `[::1]` on any port.
fn is_local_origin(origin: &HeaderValue) -> bool {
    let Ok(s) = origin.to_str() else {
        return false;
    };
    let host = match s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
    {
        Some(rest) => rest.split(['/', ':']).next().unwrap_or(""),
        None => return false,
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// Conservative response headers for the viewer. The collector serves an SPA
/// from its own origin, so a tight CSP and framing/nosniff guards cost nothing
/// and shrink the XSS/clickjacking surface.
async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; connect-src 'self'; base-uri 'none'; \
             frame-ancestors 'none'; object-src 'none'",
        ),
    );
    res
}

// --- error type ---

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    /// Log the real cause server-side and return a generic message so SQLite or
    /// other internal error text is not echoed back to the client.
    fn internal(detail: impl std::fmt::Display) -> Self {
        tracing::error!(error = %detail, "internal error serving request");
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "internal error".into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

// --- handlers ---

async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "schema_version": agenttrace::SCHEMA_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn no_viewer() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "service": "agenttrace-collector",
            "note": "Viewer not built. Run `npm --prefix viewer run build`, or use the API directly.",
            "api": ["/v1/spans", "/v1/traces", "/v1/traces/:id", "/v1/stats", "/v1/config", "/healthz"],
        })),
    )
}

/// Ingest body: a wrapper object, a bare array, or a single span.
#[derive(Deserialize)]
#[serde(untagged)]
enum IngestBody {
    Wrapped { spans: Vec<SpanData> },
    Array(Vec<SpanData>),
    One(Box<SpanData>),
}

async fn ingest(
    State(state): State<AppState>,
    Json(body): Json<IngestBody>,
) -> Result<impl IntoResponse, ApiError> {
    let spans = match body {
        IngestBody::Wrapped { spans } => spans,
        IngestBody::Array(spans) => spans,
        IngestBody::One(s) => vec![*s],
    };

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for span in spans {
        match state.storage.store_span(span) {
            Ok(()) => accepted += 1,
            Err(e) => {
                rejected += 1;
                if errors.len() < 20 {
                    errors.push(e.to_string());
                }
            }
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "accepted": accepted, "rejected": rejected, "errors": errors })),
    ))
}

#[derive(Deserialize, Default)]
struct TraceQuery {
    status: Option<String>,
    span_type: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    search: Option<String>,
    time_from: Option<i64>,
    time_to: Option<i64>,
    min_duration_ms: Option<f64>,
    max_duration_ms: Option<f64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_traces(
    State(state): State<AppState>,
    Query(q): Query<TraceQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let filter = TraceFilter {
        status: q.status,
        span_type: q.span_type,
        provider: q.provider,
        model: q.model,
        search: q.search,
        time_from: q.time_from,
        time_to: q.time_to,
        min_duration_ms: q.min_duration_ms,
        max_duration_ms: q.max_duration_ms,
        limit: q.limit.unwrap_or(50).clamp(1, 500),
        offset: q.offset.unwrap_or(0).max(0),
    };
    let traces = state
        .storage
        .list_traces(&filter)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(json!({ "traces": traces })))
}

async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let trace_id = validate_id(&trace_id)?;
    let spans = state
        .storage
        .get_trace_spans(&trace_id)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if spans.is_empty() {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("no spans for trace {trace_id}"),
        });
    }
    Ok(Json(json!({ "trace_id": trace_id, "spans": spans })))
}

#[derive(Deserialize)]
struct ExportQuery {
    #[serde(default)]
    format: Option<String>,
}

async fn export_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let trace_id = validate_id(&trace_id)?;
    let spans = state
        .storage
        .get_trace_spans(&trace_id)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if spans.is_empty() {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("no spans for trace {trace_id}"),
        });
    }
    let value = match q.format.as_deref() {
        Some("otlp") => agenttrace::otlp::to_otlp_json("agenttrace-collector", &spans),
        _ => {
            json!({ "schema_version": agenttrace::SCHEMA_VERSION, "trace_id": trace_id, "spans": spans })
        }
    };
    Ok(Json(value))
}

async fn stats(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let s = state
        .storage
        .stats()
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(s))
}

async fn runtime_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "capture_payloads": state.config.capture_payloads,
        "retention_days": state.config.retention_days,
        "max_body_bytes": state.config.max_body_bytes,
        "bind_addr": state.config.bind_addr.to_string(),
        "loopback_only": state.config.is_loopback(),
        "schema_version": agenttrace::SCHEMA_VERSION,
    }))
}

/// Trace ids are 32 lowercase hex chars. Reject anything else early to avoid
/// wasted queries and odd inputs.
fn validate_id(id: &str) -> Result<String, ApiError> {
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(id.to_lowercase())
    } else {
        Err(ApiError::bad_request("trace_id must be 32 hex characters"))
    }
}
