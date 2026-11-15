//! HTTP exporter: ships spans to the AgentTrace collector.
//!
//! Design goals (a collector outage must never take down the agent):
//! * `export` is non-blocking — it hands the span to a bounded queue via
//!   `try_send` and **drops** the span if the queue is full.
//! * A single background thread batches queued spans and POSTs them with a short
//!   timeout. Network errors are swallowed (optionally surfaced via a counter).
//! * `flush` lets a short-lived process wait for in-flight spans before exiting.
//!
//! Enabled by the `http` feature. Uses the lightweight blocking `ureq` client so
//! instrumented agents don't need an async runtime just to export.

use super::Exporter;
use crate::span::SpanData;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const DEFAULT_QUEUE_CAPACITY: usize = 8192;
const MAX_BATCH: usize = 256;

struct Metrics {
    /// Spans dropped because the queue was full.
    dropped: AtomicU64,
    /// Spans still queued or being sent.
    pending: AtomicUsize,
    /// Failed POST attempts.
    send_errors: AtomicU64,
}

/// Exports spans to a collector's ingest endpoint over HTTP.
pub struct HttpExporter {
    tx: Option<SyncSender<SpanData>>,
    worker: Option<JoinHandle<()>>,
    metrics: Arc<Metrics>,
}

impl HttpExporter {
    /// Create an exporter posting to `endpoint` (e.g. `http://127.0.0.1:4357`).
    /// Spans are sent to `{endpoint}/v1/spans`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self::with_capacity(endpoint, DEFAULT_QUEUE_CAPACITY)
    }

    pub fn with_capacity(endpoint: impl Into<String>, capacity: usize) -> Self {
        let endpoint = endpoint.into();
        let url = format!("{}/v1/spans", endpoint.trim_end_matches('/'));
        let (tx, rx) = sync_channel::<SpanData>(capacity);
        let metrics = Arc::new(Metrics {
            dropped: AtomicU64::new(0),
            pending: AtomicUsize::new(0),
            send_errors: AtomicU64::new(0),
        });

        let worker_metrics = Arc::clone(&metrics);
        let worker = std::thread::Builder::new()
            .name("agenttrace-http-exporter".into())
            .spawn(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(5))
                    .build();
                // Block on first span, then drain a batch.
                while let Ok(first) = rx.recv() {
                    let mut batch = Vec::with_capacity(MAX_BATCH);
                    batch.push(first);
                    while batch.len() < MAX_BATCH {
                        match rx.try_recv() {
                            Ok(s) => batch.push(s),
                            Err(_) => break,
                        }
                    }
                    let count = batch.len();
                    let body = serde_json::json!({ "spans": batch });
                    let res = agent.post(&url).send_json(body);
                    if res.is_err() {
                        worker_metrics.send_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    worker_metrics.pending.fetch_sub(count, Ordering::Relaxed);
                }
            })
            .expect("failed to spawn exporter thread");

        HttpExporter {
            tx: Some(tx),
            worker: Some(worker),
            metrics,
        }
    }

    /// Spans dropped due to a full queue.
    pub fn dropped(&self) -> u64 {
        self.metrics.dropped.load(Ordering::Relaxed)
    }

    /// Failed POST attempts.
    pub fn send_errors(&self) -> u64 {
        self.metrics.send_errors.load(Ordering::Relaxed)
    }

    /// Wait up to `timeout` for all queued spans to be sent. Call before a
    /// short-lived process exits so the last spans aren't lost.
    pub fn flush(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while self.metrics.pending.load(Ordering::Relaxed) > 0 {
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Exporter for HttpExporter {
    fn export(&self, span: &SpanData) {
        if let Some(tx) = &self.tx {
            self.metrics.pending.fetch_add(1, Ordering::Relaxed);
            match tx.try_send(span.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                    self.metrics.pending.fetch_sub(1, Ordering::Relaxed);
                    self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn shutdown(&self) {
        self.flush(Duration::from_secs(2));
    }
}

impl Drop for HttpExporter {
    fn drop(&mut self) {
        // Give in-flight spans a moment, then close the channel and join so the
        // worker thread exits cleanly.
        self.flush(Duration::from_secs(2));
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
