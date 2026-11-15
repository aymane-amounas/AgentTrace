//! The public tracing API: `Tracer` and `Span`.
//!
//! Usage follows a handle model that is correct under concurrency: starting a
//! run returns a `Span`; children are created from a parent's `SpanContext`,
//! which is `Copy` and can be moved across threads/tasks. There is no hidden
//! thread-local state, so concurrent runs never cross-contaminate.
//!
//! ```no_run
//! use agenttrace::{Tracer, SpanType};
//! use agenttrace::export::NdjsonExporter;
//!
//! let tracer = Tracer::new(NdjsonExporter::stdout());
//! let mut run = tracer.start_run("my.agent");
//! let mut call = run.child(SpanType::ModelCall, "chat.completion");
//! call.set_model("anthropic", "claude-opus-4");
//! call.set_tokens(1200, 340);
//! call.end();            // closes the child
//! run.end();             // closes the run (also happens on drop)
//! ```

use crate::export::Exporter;
use crate::id::{SpanId, TraceId};
use crate::redact::RedactionPolicy;
use crate::span::{SpanData, SpanEvent, SpanStatus, SpanType};
use crate::time;
use std::sync::Arc;

/// Minimal, copyable reference to a span: enough to parent a child or propagate
/// context across async boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
}

struct TracerInner {
    exporter: Box<dyn Exporter>,
    policy: RedactionPolicy,
}

/// Cheaply-cloneable entry point. All clones share one exporter and policy.
#[derive(Clone)]
pub struct Tracer {
    inner: Arc<TracerInner>,
}

impl Tracer {
    /// Create a tracer with the strict (safe) redaction policy.
    pub fn new<E: Exporter + 'static>(exporter: E) -> Self {
        Tracer {
            inner: Arc::new(TracerInner {
                exporter: Box::new(exporter),
                policy: RedactionPolicy::strict(),
            }),
        }
    }

    /// Create a tracer with a custom redaction policy.
    pub fn with_policy<E: Exporter + 'static>(exporter: E, policy: RedactionPolicy) -> Self {
        Tracer {
            inner: Arc::new(TracerInner {
                exporter: Box::new(exporter),
                policy,
            }),
        }
    }

    /// Start a root span (`agent.run` by default) with a fresh trace id.
    pub fn start_run(&self, operation_name: impl Into<String>) -> Span {
        self.start_root(SpanType::AgentRun, operation_name)
    }

    /// Start a root span of an explicit type (new trace).
    pub fn start_root(&self, span_type: SpanType, operation_name: impl Into<String>) -> Span {
        let data = SpanData::new(
            TraceId::generate(),
            SpanId::generate(),
            None,
            span_type,
            operation_name,
        );
        Span {
            data,
            tracer: self.clone(),
            ended: false,
        }
    }

    /// Start a child span from an explicit parent context (same trace id).
    pub fn start_child(
        &self,
        parent: SpanContext,
        span_type: SpanType,
        operation_name: impl Into<String>,
    ) -> Span {
        let data = SpanData::new(
            parent.trace_id,
            SpanId::generate(),
            Some(parent.span_id),
            span_type,
            operation_name,
        );
        Span {
            data,
            tracer: self.clone(),
            ended: false,
        }
    }

    /// Flush the underlying exporter.
    pub fn shutdown(&self) {
        self.inner.exporter.shutdown();
    }

    fn emit(&self, data: &SpanData) {
        self.inner.exporter.export(data);
    }
}

/// A live span. Mutate it while the operation runs, then `end()` it (or let it
/// drop, which auto-closes). On close the span is redacted and exported exactly
/// once.
pub struct Span {
    data: SpanData,
    tracer: Tracer,
    ended: bool,
}

impl Span {
    /// Context for parenting children / propagation.
    pub fn context(&self) -> SpanContext {
        SpanContext {
            trace_id: self.data.trace_id,
            span_id: self.data.span_id,
        }
    }

    pub fn trace_id(&self) -> TraceId {
        self.data.trace_id
    }
    pub fn span_id(&self) -> SpanId {
        self.data.span_id
    }

    /// Start a child span under this one.
    pub fn child(&self, span_type: SpanType, operation_name: impl Into<String>) -> Span {
        self.tracer
            .start_child(self.context(), span_type, operation_name)
    }

    // --- attribute setters (chainable) ---

    pub fn set_attr(
        &mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> &mut Self {
        self.data.attributes.insert(key.into(), value.into());
        self
    }

    pub fn set_model(
        &mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> &mut Self {
        self.data.provider = Some(provider.into());
        self.data.model = Some(model.into());
        self
    }

    pub fn set_tokens(&mut self, input: u64, output: u64) -> &mut Self {
        self.data.input_tokens = Some(input);
        self.data.output_tokens = Some(output);
        self
    }

    pub fn set_cost(&mut self, estimated_cost: f64) -> &mut Self {
        self.data.estimated_cost = Some(estimated_cost);
        self
    }

    pub fn set_tool_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.data.tool_name = Some(name.into());
        self
    }

    pub fn set_retry_count(&mut self, count: u32) -> &mut Self {
        self.data.retry_count = Some(count);
        self
    }

    pub fn set_status(&mut self, status: SpanStatus) -> &mut Self {
        self.data.status = status;
        self
    }

    /// Mark the span failed and record the error type/message.
    pub fn set_error(
        &mut self,
        error_type: impl Into<String>,
        message: impl Into<String>,
    ) -> &mut Self {
        self.data.status = SpanStatus::Error;
        self.data.error_type = Some(error_type.into());
        self.data.error_message = Some(message.into());
        self
    }

    /// Record a point-in-time event.
    pub fn add_event(&mut self, event: SpanEvent) -> &mut Self {
        self.data.events.push(event);
        self
    }

    /// Set status from a `Result`, recording the error on failure. Returns the
    /// result unchanged for ergonomic chaining.
    pub fn record_result<T, E: std::fmt::Display>(
        &mut self,
        error_type: &str,
        result: Result<T, E>,
    ) -> Result<T, E> {
        match &result {
            Ok(_) => {
                self.data.status = SpanStatus::Ok;
            }
            Err(e) => {
                self.set_error(error_type, e.to_string());
            }
        }
        result
    }

    /// Close the span explicitly.
    pub fn end(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.data.end_time_unix_nanos = Some(time::now_unix_nanos());
        if self.data.status == SpanStatus::Unset {
            self.data.status = SpanStatus::Ok;
        }
        // Redaction happens here, at the single choke point before export, so no
        // exporter ever receives raw secrets.
        self.tracer.inner.policy.redact_span(&mut self.data);
        self.tracer.emit(&self.data);
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        // Auto-close so an interrupted/early-returned operation is still recorded.
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::InMemoryExporter;

    #[test]
    fn parent_child_share_trace_and_link() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());

        let run = tracer.start_run("agent.run");
        let run_ctx = run.context();
        let child = run.child(SpanType::ToolCall, "search");
        let child_ctx = child.context();
        child.end();
        run.end();

        let spans = exp.spans();
        assert_eq!(spans.len(), 2);
        // child emitted first (ended first)
        let child = spans
            .iter()
            .find(|s| s.span_id == child_ctx.span_id)
            .unwrap();
        let run = spans.iter().find(|s| s.span_id == run_ctx.span_id).unwrap();
        assert_eq!(child.trace_id, run.trace_id);
        assert_eq!(child.parent_span_id, Some(run.span_id));
        assert!(run.parent_span_id.is_none());
    }

    #[test]
    fn auto_closes_on_drop_with_ok_status() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());
        {
            let _s = tracer.start_run("dropme");
        }
        let spans = exp.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].status, SpanStatus::Ok);
        assert!(spans[0].is_ended());
    }

    #[test]
    fn error_is_recorded() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());
        let mut s = tracer.start_root(SpanType::ToolCall, "flaky");
        s.set_error("TimeoutError", "upstream timed out");
        s.end();
        let span = &exp.spans()[0];
        assert_eq!(span.status, SpanStatus::Error);
        assert_eq!(span.error_type.as_deref(), Some("TimeoutError"));
    }

    #[test]
    fn redaction_applied_before_export() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());
        let mut s = tracer.start_root(SpanType::ModelCall, "chat");
        s.set_attr("authorization", "Bearer sk-ant-xxxxxxxxxxxxxxxxxxxx");
        s.set_attr("prompt", "secret user text");
        s.end();
        let span = &exp.spans()[0];
        assert_eq!(
            span.attributes["authorization"],
            serde_json::json!("[REDACTED]")
        );
        assert_eq!(
            span.attributes["prompt"],
            serde_json::json!("[omitted: payload capture disabled]")
        );
    }

    #[test]
    fn concurrent_runs_do_not_cross_contaminate() {
        let exp = InMemoryExporter::new();
        let tracer = Tracer::new(exp.clone());
        let mut handles = vec![];
        for i in 0..8 {
            let t = tracer.clone();
            handles.push(std::thread::spawn(move || {
                let run = t.start_run(format!("run.{i}"));
                let c1 = run.child(SpanType::ModelCall, "m");
                c1.end();
                let c2 = run.child(SpanType::ToolCall, "t");
                c2.end();
                run.end();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let spans = exp.spans();
        assert_eq!(spans.len(), 8 * 3);
        // Every child's trace_id must match a run with the same trace_id and no parent.
        for s in spans.iter().filter(|s| s.parent_span_id.is_some()) {
            let root = spans
                .iter()
                .find(|r| r.trace_id == s.trace_id && r.parent_span_id.is_none());
            assert!(root.is_some(), "child without matching root in its trace");
        }
    }
}
