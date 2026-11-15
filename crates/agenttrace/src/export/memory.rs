//! In-memory exporter, primarily for tests and embedding.

use super::Exporter;
use crate::span::SpanData;
use std::sync::{Arc, Mutex};

/// Collects finished spans in a shared `Vec`. Clone-shareable; all clones write
/// to the same buffer.
#[derive(Clone, Default)]
pub struct InMemoryExporter {
    spans: Arc<Mutex<Vec<SpanData>>>,
}

impl InMemoryExporter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all spans recorded so far.
    pub fn spans(&self) -> Vec<SpanData> {
        self.spans.lock().expect("poisoned").clone()
    }

    /// Number of spans recorded.
    pub fn len(&self) -> usize {
        self.spans.lock().expect("poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Exporter for InMemoryExporter {
    fn export(&self, span: &SpanData) {
        self.spans.lock().expect("poisoned").push(span.clone());
    }
}
