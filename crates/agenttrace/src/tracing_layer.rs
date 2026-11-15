//! Bridge from the [`tracing`] crate into AgentTrace.
//!
//! Many Rust agent runtimes (Venus Agent included) already emit `tracing` spans.
//! Adding [`AgentTraceLayer`] to their subscriber turns those spans into
//! AgentTrace spans with **no changes to the agent's own code** — the adapter is
//! isolated and optional.
//!
//! ## Field conventions
//! Span fields prefixed `at.` steer the mapping:
//! * `at.span_type` — one of `agent_run`, `model_call`, `tool_call`, `approval`,
//!   `retry`, `error` (otherwise inferred from the span name).
//! * `at.provider`, `at.model`, `at.tool_name`
//! * `at.input_tokens`, `at.output_tokens`, `at.retry_count`
//! * `at.error_type`, `at.error_message`
//!
//! Any other field is copied into the span's attribute bag (and still redacted).
//!
//! ```no_run
//! use agenttrace::Tracer;
//! use agenttrace::export::NdjsonExporter;
//! use agenttrace::tracing_layer::AgentTraceLayer;
//! use tracing_subscriber::prelude::*;
//!
//! let tracer = Tracer::new(NdjsonExporter::stdout());
//! tracing_subscriber::registry()
//!     .with(AgentTraceLayer::new(tracer))
//!     .init();
//! ```

use crate::span::SpanType;
use crate::tracer::{Span, SpanContext, Tracer};
use std::collections::BTreeMap;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// A `tracing_subscriber` layer that mirrors spans into AgentTrace.
pub struct AgentTraceLayer {
    tracer: Tracer,
}

impl AgentTraceLayer {
    pub fn new(tracer: Tracer) -> Self {
        AgentTraceLayer { tracer }
    }
}

// Wrappers to store per-tracing-span state in the registry's extensions.
struct StoredSpan(Span);
#[derive(Clone, Copy)]
struct StoredCtx(SpanContext);

#[derive(Default)]
struct FieldVisitor {
    span_type: Option<SpanType>,
    provider: Option<String>,
    model: Option<String>,
    tool_name: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    retry_count: Option<u32>,
    error_type: Option<String>,
    error_message: Option<String>,
    attrs: BTreeMap<String, serde_json::Value>,
}

fn parse_span_type(s: &str) -> Option<SpanType> {
    match s.to_lowercase().replace(['.', '-'], "_").as_str() {
        "agent_run" | "run" => Some(SpanType::AgentRun),
        "model_call" | "model" | "llm" => Some(SpanType::ModelCall),
        "tool_call" | "tool" => Some(SpanType::ToolCall),
        "approval" => Some(SpanType::Approval),
        "retry" => Some(SpanType::Retry),
        "error" => Some(SpanType::Error),
        _ => None,
    }
}

fn infer_span_type(name: &str) -> SpanType {
    let n = name.to_lowercase();
    if n.contains("model") || n.contains("llm") || n.contains("completion") || n.contains("chat") {
        SpanType::ModelCall
    } else if n.contains("approval") || n.contains("approve") {
        SpanType::Approval
    } else if n.contains("retry") {
        SpanType::Retry
    } else if n.contains("tool") {
        SpanType::ToolCall
    } else if n.contains("run") || n.contains("agent") {
        SpanType::AgentRun
    } else {
        // Neutral default; most unnamed work is a tool/sub-operation.
        SpanType::ToolCall
    }
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "at.span_type" => self.span_type = parse_span_type(value),
            "at.provider" => self.provider = Some(value.to_string()),
            "at.model" => self.model = Some(value.to_string()),
            "at.tool_name" => self.tool_name = Some(value.to_string()),
            "at.error_type" => self.error_type = Some(value.to_string()),
            "at.error_message" => self.error_message = Some(value.to_string()),
            other => {
                self.attrs.insert(
                    other.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "at.input_tokens" => self.input_tokens = Some(value),
            "at.output_tokens" => self.output_tokens = Some(value),
            "at.retry_count" => self.retry_count = Some(value as u32),
            other => {
                self.attrs
                    .insert(other.to_string(), serde_json::json!(value));
            }
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if value >= 0 {
            self.record_u64(field, value as u64);
        } else {
            self.attrs
                .insert(field.name().to_string(), serde_json::json!(value));
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.attrs
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.attrs
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        // Route through record_str so `at.*` keys are still recognized.
        self.record_str(field, &rendered);
    }
}

fn apply(visitor: &FieldVisitor, span: &mut Span) {
    if let (Some(p), Some(m)) = (&visitor.provider, &visitor.model) {
        span.set_model(p.clone(), m.clone());
    } else {
        if let Some(p) = &visitor.provider {
            span.set_attr("provider", p.clone());
        }
        if let Some(m) = &visitor.model {
            span.set_attr("model", m.clone());
        }
    }
    if let (Some(i), Some(o)) = (visitor.input_tokens, visitor.output_tokens) {
        span.set_tokens(i, o);
    }
    if let Some(t) = &visitor.tool_name {
        span.set_tool_name(t.clone());
    }
    if let Some(r) = visitor.retry_count {
        span.set_retry_count(r);
    }
    if let (Some(et), Some(em)) = (&visitor.error_type, &visitor.error_message) {
        span.set_error(et.clone(), em.clone());
    }
    for (k, v) in &visitor.attrs {
        span.set_attr(k.clone(), v.clone());
    }
}

impl<S> Layer<S> for AgentTraceLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);

        let name = attrs.metadata().name();
        let span_type = visitor.span_type.unwrap_or_else(|| infer_span_type(name));

        // Parent AgentTrace context, if this tracing span has a parent we tracked.
        let parent_ctx = ctx
            .span(id)
            .and_then(|s| s.parent())
            .and_then(|p| p.extensions().get::<StoredCtx>().map(|c| c.0));

        let mut at_span = match parent_ctx {
            Some(pc) => self.tracer.start_child(pc, span_type, name.to_string()),
            None => self.tracer.start_root(span_type, name.to_string()),
        };
        apply(&visitor, &mut at_span);

        if let Some(s) = ctx.span(id) {
            let sctx = at_span.context();
            let mut ext = s.extensions_mut();
            ext.insert(StoredSpan(at_span));
            ext.insert(StoredCtx(sctx));
        }
    }

    fn on_record(&self, id: &Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        values.record(&mut visitor);
        if let Some(s) = ctx.span(id) {
            if let Some(stored) = s.extensions_mut().get_mut::<StoredSpan>() {
                apply(&visitor, &mut stored.0);
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(s) = ctx.span(&id) {
            let stored = s.extensions_mut().remove::<StoredSpan>();
            if let Some(StoredSpan(span)) = stored {
                span.end();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::InMemoryExporter;
    use tracing_subscriber::prelude::*;

    #[test]
    fn mirrors_tracing_spans_into_agenttrace() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());
        let subscriber = tracing_subscriber::registry().with(AgentTraceLayer::new(tracer));

        tracing::subscriber::with_default(subscriber, || {
            let run = tracing::info_span!("agent_run", at.span_type = "agent_run");
            let _g = run.enter();
            let model = tracing::info_span!(
                "chat.completion",
                at.span_type = "model_call",
                at.provider = "anthropic",
                at.model = "claude-opus-4",
                at.input_tokens = 100u64,
                at.output_tokens = 50u64
            );
            model.in_scope(|| {});
        });

        let spans = exp.spans();
        assert_eq!(spans.len(), 2);
        let model = spans
            .iter()
            .find(|s| s.span_type == SpanType::ModelCall)
            .unwrap();
        let run = spans
            .iter()
            .find(|s| s.span_type == SpanType::AgentRun)
            .unwrap();
        assert_eq!(model.provider.as_deref(), Some("anthropic"));
        assert_eq!(model.input_tokens, Some(100));
        assert_eq!(model.parent_span_id, Some(run.span_id));
        assert_eq!(model.trace_id, run.trace_id);
    }
}
