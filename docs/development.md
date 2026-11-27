# Development

## Prerequisites

- Rust (stable, 1.85+, because a transitive dependency uses edition 2024)
- Node 20+ (CI uses 22)
- Docker, optional, for the container build

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

When you are working on the viewer, run the collector on `:4357` and
`npm run dev` on `:5173`. Vite proxies `/v1` and `/healthz` through to the
collector, so the UI talks to real data.

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

The two features are `http` (the HTTP exporter) and `tracing-layer` (the bridge to
the `tracing` crate).

## Testing philosophy

The tests go after real behavior rather than mocks: id round-trips, concurrency
isolation, redaction (including a test that POSTs a secret as an untrusted client
through the HTTP API and checks it never lands in storage), storage filtering, and
a full ingest → query → inspect → export integration test. Run them all with
`cargo test --workspace --all-features`.
