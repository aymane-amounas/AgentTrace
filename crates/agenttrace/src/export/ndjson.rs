//! NDJSON exporter: one JSON span per line. Portable, greppable, and the format
//! the collector can bulk-import. Works with any `Write` sink (file, stdout).

use super::Exporter;
use crate::span::SpanData;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

/// Writes each span as a single JSON line to an arbitrary sink.
pub struct NdjsonExporter {
    sink: Mutex<Box<dyn Write + Send>>,
}

impl NdjsonExporter {
    /// Build from any writer (e.g. `io::stdout()`).
    pub fn new<W: Write + Send + 'static>(writer: W) -> Self {
        NdjsonExporter {
            sink: Mutex::new(Box::new(writer)),
        }
    }

    /// Append spans to a file, creating it if needed.
    pub fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(NdjsonExporter::new(BufWriter::new(file)))
    }

    /// Truncate-or-create a file (fresh run output).
    pub fn file_truncate(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(NdjsonExporter::new(BufWriter::new(file)))
    }

    /// Write spans to stdout.
    pub fn stdout() -> Self {
        NdjsonExporter::new(io::stdout())
    }
}

impl Exporter for NdjsonExporter {
    fn export(&self, span: &SpanData) {
        // Serialization of a span cannot fail in practice; if the sink errors
        // (disk full, closed pipe) we drop the line rather than crash the agent.
        if let Ok(line) = serde_json::to_string(span) {
            if let Ok(mut sink) = self.sink.lock() {
                let _ = sink.write_all(line.as_bytes());
                let _ = sink.write_all(b"\n");
            }
        }
    }

    fn shutdown(&self) {
        if let Ok(mut sink) = self.sink.lock() {
            let _ = sink.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{SpanId, TraceId};
    use crate::span::SpanType;

    #[test]
    fn writes_one_line_per_span() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.ndjson");
        {
            let exp = NdjsonExporter::file_truncate(&path).unwrap();
            for i in 0..3 {
                let s = SpanData::new(
                    TraceId::generate(),
                    SpanId::generate(),
                    None,
                    SpanType::AgentRun,
                    format!("run.{i}"),
                );
                exp.export(&s);
            }
            exp.shutdown();
        }
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        for line in lines {
            let _: SpanData = serde_json::from_str(line).unwrap();
        }
    }
}
