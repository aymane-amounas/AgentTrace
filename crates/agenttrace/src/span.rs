//! The AgentTrace span schema (v0.1.0).
//!
//! A span is one unit of work in an agent run: the run itself, a model call, a
//! tool call, an approval gate, a retry, or an error. Spans form a tree via
//! `parent_span_id`, and all spans in one run share a `trace_id`.
//!
//! The schema is intentionally explicit about agent concepts (provider, model,
//! tokens, cost, tool name, retry count) rather than burying them in a generic
//! attribute bag, so the viewer and queries can rely on them. Anything extra
//! lives in `attributes`. See `docs/schema.md` for the full convention table.

use crate::id::{SpanId, TraceId};
use crate::time;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the span schema. Bumped on any breaking change to field meaning.
pub const SCHEMA_VERSION: &str = "0.1.0";

/// Kind of operation a span represents.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    /// Top-level agent run (`agent.run`).
    AgentRun,
    /// A call to an LLM / model provider (`model.call`).
    ModelCall,
    /// A tool / function invocation (`tool.call`).
    ToolCall,
    /// A human-or-policy approval gate (`approval`).
    Approval,
    /// A retry of a previous operation (`retry`).
    Retry,
    /// A standalone error event span (`error`).
    Error,
}

impl SpanType {
    /// Canonical dotted operation-type name used in OTLP mapping and docs.
    pub fn as_str(&self) -> &'static str {
        match self {
            SpanType::AgentRun => "agent.run",
            SpanType::ModelCall => "model.call",
            SpanType::ToolCall => "tool.call",
            SpanType::Approval => "approval",
            SpanType::Retry => "retry",
            SpanType::Error => "error",
        }
    }
}

/// Completion status of a span.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    /// Not yet set (still running, or status never recorded).
    #[default]
    Unset,
    /// Completed successfully.
    Ok,
    /// Failed.
    Error,
}

/// A point-in-time event recorded on a span (e.g. "retry.scheduled").
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SpanEvent {
    pub name: String,
    pub time_unix_nanos: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_json::Value>,
}

impl SpanEvent {
    pub fn new(name: impl Into<String>) -> Self {
        SpanEvent {
            name: name.into(),
            time_unix_nanos: time::now_unix_nanos(),
            attributes: BTreeMap::new(),
        }
    }

    pub fn with_attr(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// The serialized record for a single span. This is the unit the SDK exports,
/// the collector ingests, and the viewer renders.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SpanData {
    /// Schema version this record was produced with.
    pub schema_version: String,

    // --- identity & structure ---
    pub trace_id: TraceId,
    pub span_id: SpanId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    pub span_type: SpanType,
    pub operation_name: String,

    // --- timing ---
    pub start_time_unix_nanos: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_unix_nanos: Option<u64>,

    // --- status ---
    #[serde(default)]
    pub status: SpanStatus,

    // --- model.call attributes ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,

    // --- tool.call attributes ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,

    // --- retry attributes ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,

    // --- error attributes ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    // --- extensibility ---
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<SpanEvent>,
}

impl SpanData {
    /// Create a new, open (not-yet-ended) span record.
    pub fn new(
        trace_id: TraceId,
        span_id: SpanId,
        parent_span_id: Option<SpanId>,
        span_type: SpanType,
        operation_name: impl Into<String>,
    ) -> Self {
        SpanData {
            schema_version: SCHEMA_VERSION.to_string(),
            trace_id,
            span_id,
            parent_span_id,
            span_type,
            operation_name: operation_name.into(),
            start_time_unix_nanos: time::now_unix_nanos(),
            end_time_unix_nanos: None,
            status: SpanStatus::Unset,
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            tool_name: None,
            retry_count: None,
            error_type: None,
            error_message: None,
            attributes: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// Duration in milliseconds, or `None` if the span has not ended.
    pub fn duration_ms(&self) -> Option<f64> {
        self.end_time_unix_nanos
            .map(|end| time::duration_ms(self.start_time_unix_nanos, end))
    }

    /// Whether the span has been closed.
    pub fn is_ended(&self) -> bool {
        self.end_time_unix_nanos.is_some()
    }

    /// Structural validation independent of transport. Used by the collector to
    /// reject malformed spans before they reach storage.
    pub fn validate(&self) -> Result<(), String> {
        if !self.trace_id.is_valid() {
            return Err("trace_id is zero/invalid".into());
        }
        if !self.span_id.is_valid() {
            return Err("span_id is zero/invalid".into());
        }
        if self.operation_name.trim().is_empty() {
            return Err("operation_name is empty".into());
        }
        if let Some(parent) = self.parent_span_id {
            if !parent.is_valid() {
                return Err("parent_span_id is present but zero/invalid".into());
            }
            if parent == self.span_id {
                return Err("span cannot be its own parent".into());
            }
        }
        if let Some(end) = self.end_time_unix_nanos {
            if end < self.start_time_unix_nanos {
                return Err("end_time precedes start_time".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SpanData {
        SpanData::new(
            TraceId::generate(),
            SpanId::generate(),
            None,
            SpanType::AgentRun,
            "demo.run",
        )
    }

    #[test]
    fn duration_none_until_ended() {
        let mut s = sample();
        assert!(s.duration_ms().is_none());
        s.start_time_unix_nanos = 0;
        s.end_time_unix_nanos = Some(5_000_000);
        assert_eq!(s.duration_ms(), Some(5.0));
    }

    #[test]
    fn validation_catches_structural_errors() {
        let mut s = sample();
        assert!(s.validate().is_ok());

        s.operation_name = "  ".into();
        assert!(s.validate().is_err());

        let mut s = sample();
        s.parent_span_id = Some(s.span_id);
        assert!(s.validate().is_err());

        let mut s = sample();
        s.start_time_unix_nanos = 100;
        s.end_time_unix_nanos = Some(50);
        assert!(s.validate().is_err());
    }

    #[test]
    fn json_omits_empty_optionals() {
        let json = serde_json::to_string(&sample()).unwrap();
        assert!(!json.contains("provider"));
        assert!(!json.contains("tool_name"));
        assert!(json.contains("\"span_type\":\"agent_run\""));
    }

    #[test]
    fn span_type_names_are_stable() {
        assert_eq!(SpanType::ModelCall.as_str(), "model.call");
        assert_eq!(SpanType::ToolCall.as_str(), "tool.call");
    }
}
