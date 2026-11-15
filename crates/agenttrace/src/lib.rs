//! # AgentTrace SDK
//!
//! Structured, OpenTelemetry-inspired tracing for AI agents. Records spans for
//! agent runs, model calls, tool calls, approvals, retries and errors, with
//! parent/child structure, token/cost metadata, and secrets redacted *before*
//! they ever leave the process.
//!
//! ## Quick start
//! ```no_run
//! use agenttrace::{Tracer, SpanType};
//! use agenttrace::export::NdjsonExporter;
//!
//! let tracer = Tracer::new(NdjsonExporter::stdout());
//! let mut run = tracer.start_run("support.agent");
//! let mut model = run.child(SpanType::ModelCall, "chat.completion");
//! model.set_model("anthropic", "claude-opus-4").set_tokens(850, 210);
//! model.end();
//! run.end();
//! ```
//!
//! ## Design guarantees
//! * **Redaction before export** - every span is scrubbed at one choke point
//!   before any exporter sees it. Raw payloads are dropped unless opted in.
//! * **Never crashes the agent** - exporters are non-blocking and swallow errors;
//!   the HTTP exporter drops spans rather than stalling if the collector is down.
//! * **Concurrency-correct** - no hidden global state; children are created from a
//!   `Copy` [`SpanContext`], so concurrent runs stay isolated.

pub mod export;
pub mod id;
pub mod otlp;
pub mod redact;
pub mod span;
pub mod time;
pub mod tracer;

#[cfg(feature = "tracing-layer")]
pub mod tracing_layer;

pub use id::{SpanId, TraceId};
pub use redact::RedactionPolicy;
pub use span::{SpanData, SpanEvent, SpanStatus, SpanType, SCHEMA_VERSION};
pub use tracer::{Span, SpanContext, Tracer};
