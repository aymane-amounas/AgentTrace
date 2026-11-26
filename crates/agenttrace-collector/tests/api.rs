//! End-to-end API test: ingest real SDK-produced spans through the HTTP router,
//! then query, inspect failure, and export them. Uses `tower::oneshot` so no
//! socket is needed.

use agenttrace::export::InMemoryExporter;
use agenttrace::{SpanType, Tracer};
use agenttrace_collector::config::Config;
use agenttrace_collector::routes::{app, AppState};
use agenttrace_collector::storage::Storage;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

/// Produce a realistic trace: run -> model call -> tool (ok) -> tool (fails) ->
/// retry -> run fails. Returns the serialized spans and the trace id.
fn make_trace() -> (Vec<Value>, String) {
    let exp = InMemoryExporter::new();
    let tracer = Tracer::new(exp.clone());

    let mut run = tracer.start_run("support.agent");
    let trace_id = run.trace_id().to_hex();

    let mut model = run.child(SpanType::ModelCall, "chat.completion");
    model
        .set_model("anthropic", "claude-opus-4")
        .set_tokens(820, 190)
        .set_cost(0.012);
    model.end();

    let ok_tool = run.child(SpanType::ToolCall, "search_docs");
    {
        let mut t = ok_tool;
        t.set_tool_name("search_docs");
        t.end();
    }

    let mut bad_tool = run.child(SpanType::ToolCall, "fetch_url");
    bad_tool.set_tool_name("fetch_url");
    bad_tool.set_error("TimeoutError", "upstream timed out after 5s");
    bad_tool.end();

    let mut retry = run.child(SpanType::Retry, "fetch_url.retry");
    retry.set_retry_count(1);
    retry.set_error("TimeoutError", "still timing out");
    retry.end();

    run.set_error("ToolFailure", "fetch_url failed after 1 retry");
    run.end();

    let spans: Vec<Value> = exp
        .spans()
        .iter()
        .map(|s| serde_json::to_value(s).unwrap())
        .collect();
    (spans, trace_id)
}

fn test_state() -> AppState {
    let storage = Storage::open_in_memory(agenttrace::RedactionPolicy::strict()).unwrap();
    AppState {
        storage,
        config: Arc::new(Config::default()),
    }
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn ingest_query_inspect_and_export() {
    let state = test_state();
    let router = app(state);
    let (spans, trace_id) = make_trace();

    // --- ingest ---
    let body = json!({ "spans": spans }).to_string();
    let resp = router
        .clone()
        .oneshot(
            Request::post("/v1/spans")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let v = body_json(resp).await;
    assert_eq!(v["accepted"], json!(5));
    assert_eq!(v["rejected"], json!(0));

    // --- list: one trace, status error ---
    let resp = router
        .clone()
        .oneshot(Request::get("/v1/traces").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let traces = v["traces"].as_array().unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["status"], json!("error"));
    assert_eq!(traces[0]["span_count"], json!(5));
    assert_eq!(traces[0]["input_tokens"], json!(820));

    // --- filter by status=error returns it; status=ok does not ---
    let resp = router
        .clone()
        .oneshot(
            Request::get("/v1/traces?status=ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(resp).await;
    assert_eq!(v["traces"].as_array().unwrap().len(), 0);

    // --- detail: find the failing span ---
    let resp = router
        .clone()
        .oneshot(
            Request::get(format!("/v1/traces/{trace_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let spans = v["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 5);
    let failing: Vec<&Value> = spans
        .iter()
        .filter(|s| s["status"] == json!("error"))
        .collect();
    assert!(failing
        .iter()
        .any(|s| s["error_type"] == json!("TimeoutError")));

    // --- OTLP export ---
    let resp = router
        .clone()
        .oneshot(
            Request::get(format!("/v1/traces/{trace_id}/export?format=otlp"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(resp).await;
    assert!(
        v["resourceSpans"][0]["scopeSpans"][0]["spans"]
            .as_array()
            .unwrap()
            .len()
            == 5
    );

    // --- stats ---
    let resp = router
        .clone()
        .oneshot(Request::get("/v1/stats").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let v = body_json(resp).await;
    assert_eq!(v["trace_count"], json!(1));
    assert_eq!(v["model_calls"], json!(1));
    assert_eq!(v["tool_calls"], json!(2));

    // --- health ---
    let resp = router
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn sets_security_headers_and_scopes_cors_to_localhost() {
    let router = app(test_state());

    // Security headers present on an API response.
    let resp = router
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let h = resp.headers();
    assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
    assert!(h.get("content-security-policy").is_some());

    // A request from a foreign site gets no CORS allow-origin header, so the
    // browser will not expose the response to that page.
    let resp = router
        .clone()
        .oneshot(
            Request::get("/v1/traces")
                .header("origin", "https://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.headers().get("access-control-allow-origin").is_none());

    // The local viewer dev server is allowed.
    let resp = router
        .oneshot(
            Request::get("/v1/traces")
                .header("origin", "http://localhost:5173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "http://localhost:5173"
    );
}

#[tokio::test]
async fn rejects_malformed_trace_id() {
    let router = app(test_state());
    let resp = router
        .oneshot(
            Request::get("/v1/traces/not-a-valid-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn secrets_never_reach_storage_via_api() {
    let state = test_state();
    let router = app(state);

    // Simulate an UNTRUSTED client that did NOT redact: craft raw span JSON with
    // a real secret in attributes. The collector must redact at its own boundary.
    let trace_id = agenttrace::TraceId::generate().to_hex();
    let span_id = agenttrace::SpanId::generate().to_hex();
    let raw_span = json!({
        "schema_version": agenttrace::SCHEMA_VERSION,
        "trace_id": trace_id,
        "span_id": span_id,
        "span_type": "agent_run",
        "operation_name": "leaky.agent",
        "start_time_unix_nanos": 1_700_000_000_000_000_000u64,
        "end_time_unix_nanos": 1_700_000_000_500_000_000u64,
        "status": "ok",
        "attributes": {
            "authorization": "Bearer sk-ant-aaaaaaaaaaaaaaaaaaaaaaaa",
            "openai_key": "sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
            "prompt": "please do the thing",
            "note": "leaked AKIA1234567890ABCDEF in logs"
        }
    });

    let body = json!({ "spans": [raw_span] }).to_string();
    let resp = router
        .clone()
        .oneshot(
            Request::post("/v1/spans")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let resp = router
        .oneshot(
            Request::get(format!("/v1/traces/{trace_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(resp).await;
    let stored = serde_json::to_string(&v).unwrap();
    assert!(
        !stored.contains("sk-ant-aaaaaaaaaaaaaaaaaaaaaaaa"),
        "secret leaked into storage"
    );
    assert!(
        !stored.contains("AKIA1234567890ABCDEF"),
        "AWS key leaked into storage"
    );
    assert!(
        !stored.contains("sk-proj-ABCDEFGHIJKL"),
        "OpenAI sk-proj key leaked into storage"
    );
    assert!(stored.contains("[REDACTED]"));
}
