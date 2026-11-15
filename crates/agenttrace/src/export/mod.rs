//! Exporters: where finished (already-redacted) spans go.
//!
//! The SDK redacts every span before handing it to an exporter, so an exporter
//! never sees raw secrets. Exporters must be cheap and must never panic or block
//! the agent; the HTTP exporter in particular drops spans rather than stalling
//! the instrumented process if the collector is unavailable.

use crate::span::SpanData;

#[cfg(feature = "http")]
pub mod http;
pub mod memory;
pub mod ndjson;

#[cfg(feature = "http")]
pub use http::HttpExporter;
pub use memory::InMemoryExporter;
pub use ndjson::NdjsonExporter;

/// Sink for finished spans. Implementations must be `Send + Sync` so a single
/// `Tracer` can be shared across threads and tasks.
pub trait Exporter: Send + Sync {
    /// Record one finished span. Called exactly once per span, after redaction.
    fn export(&self, span: &SpanData);

    /// Flush/close underlying resources. Default is a no-op.
    fn shutdown(&self) {}
}

/// Drops every span. Useful to disable tracing with zero overhead.
pub struct NoopExporter;

impl Exporter for NoopExporter {
    fn export(&self, _span: &SpanData) {}
}

/// Fan-out to multiple exporters (e.g. local NDJSON file + HTTP collector).
pub struct MultiExporter {
    exporters: Vec<Box<dyn Exporter>>,
}

impl MultiExporter {
    pub fn new(exporters: Vec<Box<dyn Exporter>>) -> Self {
        MultiExporter { exporters }
    }
}

impl Exporter for MultiExporter {
    fn export(&self, span: &SpanData) {
        for e in &self.exporters {
            e.export(span);
        }
    }
    fn shutdown(&self) {
        for e in &self.exporters {
            e.shutdown();
        }
    }
}
