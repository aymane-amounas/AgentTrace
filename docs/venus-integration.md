# Integrating with Venus Agent

[Venus Agent](https://github.com/aymane-amounas/venusAgent) is a Rust agent
runtime that already emits [`tracing`](https://docs.rs/tracing) spans and
initializes a `tracing-subscriber`. AgentTrace plugs in as an **additional
subscriber layer**, so instrumenting Venus needs **no changes to its core logic**
- only its subscriber setup.

This keeps the adapter isolated and optional: the sample agent (and the SDK in
general) works with or without Venus.

## How it works

`agenttrace`'s `tracing-layer` feature provides `AgentTraceLayer`, a
`tracing_subscriber::Layer` that mirrors `tracing` spans into AgentTrace spans,
preserving parent/child structure. Fields prefixed `at.` steer the mapping
(`at.span_type`, `at.provider`, `at.model`, `at.input_tokens`, …); other fields
become redacted attributes. See
[`tracing_layer.rs`](../crates/agenttrace/src/tracing_layer.rs).

## Wiring it in

In Venus's binary, where the subscriber is built, add the layer:

```rust
use agenttrace::Tracer;
use agenttrace::export::HttpExporter;
use agenttrace::tracing_layer::AgentTraceLayer;
use tracing_subscriber::prelude::*;

let tracer = Tracer::new(HttpExporter::new("http://127.0.0.1:4357"));

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())     // Venus's existing logging
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .with(AgentTraceLayer::new(tracer))         // ← AgentTrace
    .init();
```

Add the dependency with the feature:

```toml
agenttrace = { git = "https://github.com/aymane-amounas/AgentTrace", features = ["http", "tracing-layer"] }
```

## Emitting rich spans

Venus's model-call, tool-call, approval and retry sites can annotate their
existing `tracing` spans so AgentTrace classifies them precisely:

```rust
let span = tracing::info_span!(
    "model.call",
    at.span_type = "model_call",
    at.provider = "anthropic",
    at.model = %model_id,
    at.input_tokens = input_tokens,
    at.output_tokens = output_tokens,
);
let _g = span.enter();
```

Where a span is not annotated, the layer infers the type from the span name
(`model`/`tool`/`approval`/`retry`/`run`).

## Status

The adapter mechanism (the `tracing-layer` bridge) is implemented and tested.
The exact annotation of Venus's internal spans lives in the Venus repository and
is applied there; AgentTrace deliberately ships no fork of or patch to Venus. The
sample agent demonstrates the full feature set independently.

### Verified against real Venus code (2026-11-15)

AgentTrace was run against **real `venus-agent` code** (its capability
orchestrator: authorization → execution → audit) with no changes to either repo.
A successful `workspace.summary` and a failing `workspace.file_read` produced a
correct parent/child trace (run → two tool calls, one `error`).

Two findings from inspecting Venus:

- Venus currently emits **no `tracing` spans**, so the layer bridge alone captures
  nothing until spans are added. Until then, wrap Venus operations with the
  AgentTrace SDK directly (as the verification harness does).
- Venus has **no retry logic**, so the `retry` span type has nothing to map there
  yet.

To get deep, automatic tracing, add `tracing::info_span!` annotations (with the
`at.*` fields above) at Venus's model-call sites (`src/cli.rs`, `src/tui.rs`) and
its capability runtime (`src/orchestrator.rs::run`). These are additive, need no
new dependency (Venus already uses `tracing`), and leave default behavior
unchanged - a good first PR, left for review rather than applied blindly.
