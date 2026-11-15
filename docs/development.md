# Development

## Prerequisites

- Rust (stable, 1.85+ - a transitive dependency uses edition 2024)
- Node 20+ (22 used in CI)
- Docker (optional, for the container build)

## Workspace layout

```
crates/
  agenttrace/            SDK: schema, tracer, redaction, exporters, OTLP map
  agenttrace-collector/  axum + SQLite ingest/query/export server
  sample-agent/          instrumented demo agent
viewer/                  React + Vite trace viewer
docs/                    architecture, schema, privacy, deployment
```

## Common tasks

```bash
# Rust
cargo test --workspace --all-features     # all tests
cargo fmt --all                           # format
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Viewer
npm --prefix viewer install
npm --prefix viewer run dev               # dev server on :5173, proxies to :4357
npm --prefix viewer run build
npm --prefix viewer run lint

# Full local demo
cargo run --release -p agenttrace-collector &   # :4357
cargo run -p sample-agent                        # produces traces
```

During viewer development, run the collector (`:4357`) and `npm run dev`
(`:5173`); Vite proxies `/v1` and `/healthz` to the collector.

## Using the SDK in your own agent

```rust
use agenttrace::{Tracer, SpanType};
use agenttrace::export::HttpExporter; // needs feature = "http"

let tracer = Tracer::new(HttpExporter::new("http://127.0.0.1:4357"));
let mut run = tracer.start_run("my.agent");

let mut model = run.child(SpanType::ModelCall, "chat.completion");
model.set_model("anthropic", "claude-opus-4").set_tokens(850, 210).set_cost(0.02);
model.end();

let mut tool = run.child(SpanType::ToolCall, "search");
tool.set_tool_name("search");
if let Err(e) = do_search() {
    tool.set_error("SearchError", e.to_string());
}
tool.end();

run.end();          // also happens on drop
tracer.shutdown();  // flush the HTTP exporter before exit
```

Features: `http` (HTTP exporter), `tracing-layer` (bridge the `tracing` crate).

## Testing philosophy

Tests cover real behavior, not mocks: id round-trips, concurrency isolation,
redaction (including an untrusted-client secret-leak test through the HTTP API),
storage filtering, and a full ingest → query → inspect → export integration
test. See `cargo test --workspace --all-features`.
