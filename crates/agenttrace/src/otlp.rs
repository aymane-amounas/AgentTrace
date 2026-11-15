//! OpenTelemetry (OTLP/JSON) mapping for interoperability.
//!
//! AgentTrace spans map onto OTLP trace spans so traces can be exported to any
//! OTLP-aware backend (Tempo, Jaeger via OTLP, vendor collectors). Agent-specific
//! concepts become attributes, reusing `gen_ai.*` semantic conventions where they
//! exist and `agenttrace.*` otherwise. See `docs/schema.md` for the full table.
//!
//! This is an *export* mapping. Full OTLP ingestion is a documented roadmap item.

use crate::span::{SpanData, SpanStatus};
use serde_json::{json, Value};

/// OTLP status codes.
fn status_code(status: SpanStatus) -> i32 {
    match status {
        SpanStatus::Unset => 0, // STATUS_CODE_UNSET
        SpanStatus::Ok => 1,    // STATUS_CODE_OK
        SpanStatus::Error => 2, // STATUS_CODE_ERROR
    }
}

fn kv_str(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
}
fn kv_int(key: &str, value: u64) -> Value {
    // OTLP encodes int64 as a string in JSON.
    json!({ "key": key, "value": { "intValue": value.to_string() } })
}
fn kv_double(key: &str, value: f64) -> Value {
    json!({ "key": key, "value": { "doubleValue": value } })
}

fn span_to_otlp(s: &SpanData) -> Value {
    let mut attrs: Vec<Value> = Vec::new();
    attrs.push(kv_str("agenttrace.span_type", s.span_type.as_str()));
    attrs.push(kv_str("agenttrace.schema_version", &s.schema_version));
    if let Some(p) = &s.provider {
        attrs.push(kv_str("gen_ai.system", p));
    }
    if let Some(m) = &s.model {
        attrs.push(kv_str("gen_ai.request.model", m));
    }
    if let Some(t) = s.input_tokens {
        attrs.push(kv_int("gen_ai.usage.input_tokens", t));
    }
    if let Some(t) = s.output_tokens {
        attrs.push(kv_int("gen_ai.usage.output_tokens", t));
    }
    if let Some(c) = s.estimated_cost {
        attrs.push(kv_double("agenttrace.estimated_cost", c));
    }
    if let Some(name) = &s.tool_name {
        attrs.push(kv_str("agenttrace.tool.name", name));
    }
    if let Some(r) = s.retry_count {
        attrs.push(kv_int("agenttrace.retry_count", r as u64));
    }
    if let Some(et) = &s.error_type {
        attrs.push(kv_str("exception.type", et));
    }
    if let Some(em) = &s.error_message {
        attrs.push(kv_str("exception.message", em));
    }
    // Free-form attributes.
    for (k, v) in &s.attributes {
        match v {
            Value::String(st) => attrs.push(kv_str(k, st)),
            Value::Number(n) if n.is_f64() => attrs.push(kv_double(k, n.as_f64().unwrap())),
            Value::Number(n) => attrs.push(kv_int(k, n.as_u64().unwrap_or(0))),
            Value::Bool(b) => attrs.push(json!({"key": k, "value": {"boolValue": b}})),
            other => attrs.push(kv_str(k, &other.to_string())),
        }
    }

    let events: Vec<Value> = s
        .events
        .iter()
        .map(|e| {
            json!({
                "name": e.name,
                "timeUnixNano": e.time_unix_nanos.to_string(),
            })
        })
        .collect();

    let mut span = json!({
        "traceId": s.trace_id.to_hex(),
        "spanId": s.span_id.to_hex(),
        "name": s.operation_name,
        "kind": 1, // SPAN_KIND_INTERNAL
        "startTimeUnixNano": s.start_time_unix_nanos.to_string(),
        "endTimeUnixNano": s.end_time_unix_nanos.unwrap_or(s.start_time_unix_nanos).to_string(),
        "attributes": attrs,
        "events": events,
        "status": { "code": status_code(s.status) },
    });
    if let Some(parent) = s.parent_span_id {
        span["parentSpanId"] = json!(parent.to_hex());
    }
    span
}

/// Map a batch of AgentTrace spans to one OTLP/JSON `ExportTraceServiceRequest`.
pub fn to_otlp_json(service_name: &str, spans: &[SpanData]) -> Value {
    let otlp_spans: Vec<Value> = spans.iter().map(span_to_otlp).collect();
    json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [ kv_str("service.name", service_name) ]
            },
            "scopeSpans": [{
                "scope": { "name": "agenttrace", "version": crate::span::SCHEMA_VERSION },
                "spans": otlp_spans
            }]
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{SpanId, TraceId};
    use crate::span::SpanType;

    #[test]
    fn maps_core_fields_and_parent() {
        let tid = TraceId::generate();
        let parent = SpanId::generate();
        let mut s = SpanData::new(
            tid,
            SpanId::generate(),
            Some(parent),
            SpanType::ModelCall,
            "chat",
        );
        s.status = SpanStatus::Ok;
        s.provider = Some("anthropic".into());
        s.model = Some("claude-opus-4".into());
        s.input_tokens = Some(10);
        s.output_tokens = Some(20);
        s.end_time_unix_nanos = Some(s.start_time_unix_nanos + 1_000_000);

        let out = to_otlp_json("sample-agent", &[s]);
        let span = &out["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
        assert_eq!(span["traceId"], json!(tid.to_hex()));
        assert_eq!(span["parentSpanId"], json!(parent.to_hex()));
        assert_eq!(span["name"], json!("chat"));
        assert_eq!(span["status"]["code"], json!(1));
        let attrs = span["attributes"].as_array().unwrap();
        assert!(attrs.iter().any(|a| a["key"] == json!("gen_ai.system")));
        assert!(attrs
            .iter()
            .any(|a| a["key"] == json!("gen_ai.request.model")));
    }
}
