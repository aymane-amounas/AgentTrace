# Architecture

AgentTrace has four independently testable parts. Data flows one way: an
instrumented agent produces spans, the collector validates/redacts/stores them,
and the viewer reads them back.

```
┌──────────────────────┐   spans (HTTP / NDJSON)   ┌───────────────────────┐
│  AI agent runtime     │ ────────────────────────▶ │  Collector             │
│  (sample agent /      │                           │  axum + SQLite         │
│   Venus via adapter)  │                           │  validate · redact ·   │
│                       │                           │  store · retention     │
│  AgentTrace SDK       │                           │                        │
│  tracer · spans ·     │                           │  REST API              │
│  redaction · export   │                           └───────────┬───────────┘
└──────────────────────┘                                        │ JSON
                                                                 ▼
                                                      ┌───────────────────────┐
                                                      │  Viewer (React)        │
                                                      │  overview · explorer · │
                                                      │  waterfall · replay ·  │
                                                      │  failure inspection    │
                                                      └───────────────────────┘
```

## Components

### SDK — `crates/agenttrace`
The instrumentation library. Public API is `Tracer` + `Span` (see
[`tracer.rs`](../crates/agenttrace/src/tracer.rs)). Responsibilities:

- Generate OTel-style trace/span ids (`id.rs`).
- Maintain parent/child structure via a `Copy` `SpanContext` — no hidden global
  state, so concurrent runs stay isolated.
- Record timing, status, token/cost metadata, events.
- **Redact every span at one choke point before export** (`redact.rs`).
- Export through pluggable sinks (`export/`): in-memory, NDJSON file/stdout, HTTP.
- Optional `tracing-layer` feature bridges the `tracing` crate (used by the Venus
  adapter).

The SDK never requires the collector to be running; the HTTP exporter is
non-blocking and drops spans rather than stalling the agent.

### Collector — `crates/agenttrace-collector`
An `axum` service over SQLite (`rusqlite`, bundled). It:

- Accepts spans on `POST /v1/spans` (wrapper object, array, or single span).
- Validates structure and **re-redacts at the persistence boundary** so an
  untrusted client cannot land secrets in the database.
- Upserts by `span_id` (idempotent; tolerant of duplicates and out-of-order
  delivery), and keeps incomplete/interrupted spans (null end time).
- Serves query endpoints and OTLP/native export.
- Prunes spans past the retention window on startup and hourly.
- Serves the built viewer as a SPA.

Storage is interface-light: the `Storage` type owns all SQL, so a different
backend could be added without touching the SDK or API handlers.

### Viewer — `viewer/`
React + Vite + TypeScript. Pages: Overview (metrics), Traces (filterable
explorer), Trace detail (waterfall + span tree + detail panel + historical
replay + export), Privacy. No backend logic lives here; it only reads the API.

### Sample agent — `crates/sample-agent`
A runnable agent that exercises every span type and produces the signature
failure demo (model call → tools → failing tool → retry → failed run).

## Key decisions

- **Rust-first core** for the SDK and collector: low overhead, easy to embed in
  Rust agent runtimes (Venus is Rust), single static binary for the collector.
- **SQLite** for first-party local storage: zero-config, durable, great for
  single-node developer use. Indexed by trace, time, type and provider.
- **OpenTelemetry-shaped** ids and an OTLP/JSON export mapping for
  interoperability, without adopting the full OTel SDK weight.
- **Redaction twice** (SDK and collector): defense in depth.
- **No hidden async runtime in the SDK**: the HTTP exporter uses a lightweight
  blocking client on a background thread, so instrumenting a sync agent needs no
  executor.

See [`schema.md`](schema.md) for the data model and [`privacy.md`](privacy.md)
for the security model.
