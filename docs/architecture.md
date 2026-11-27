# Architecture

AgentTrace has four parts, each testable on its own. Data flows one way: an
instrumented agent produces spans, the collector validates, redacts and stores
them, and the viewer reads them back.

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

### SDK - `crates/agenttrace`

This is the instrumentation library. The public API is `Tracer` plus `Span` (see
[`tracer.rs`](../crates/agenttrace/src/tracer.rs)). It generates OTel-style
trace and span ids (`id.rs`) and keeps parent/child structure through a `Copy`
`SpanContext`, so there is no hidden global state and concurrent runs stay
isolated. It records timing, status, token and cost metadata, and events.

The important bit for privacy: every span is redacted at one choke point before
export (`redact.rs`), so there is a single place to reason about. Export goes
through pluggable sinks in `export/`: in-memory, NDJSON to a file or stdout, and
HTTP. An optional `tracing-layer` feature bridges the `tracing` crate, which is
what the Venus adapter uses.

The SDK never requires the collector to be running. The HTTP exporter is
non-blocking and drops spans rather than stalling the agent, because I would
rather lose a trace than slow down the thing being traced.

### Collector - `crates/agenttrace-collector`

An `axum` service over SQLite (`rusqlite`, bundled). It accepts spans on
`POST /v1/spans` (a wrapper object, an array, or a single span), validates their
structure, and re-redacts at the persistence boundary so an untrusted client
cannot land secrets in the database. It upserts by `span_id`, which makes ingest
idempotent and tolerant of duplicates and out-of-order delivery, and it keeps
incomplete or interrupted spans (null end time) rather than discarding them. On
top of that it serves the query endpoints and OTLP/native export, prunes spans
past the retention window on startup and hourly, and serves the built viewer as a
SPA.

Storage is kept deliberately thin: the `Storage` type owns all the SQL, so a
different backend could slot in without touching the SDK or the API handlers.

### Viewer - `viewer/`

React + Vite + TypeScript. The pages are Overview (metrics), Traces (a filterable
explorer), Trace detail (waterfall, span tree, detail panel, historical replay and
export), and Privacy. No backend logic lives here; it only reads the API.

### Sample agent - `crates/sample-agent`

A runnable agent that exercises every span type and produces the signature failure
demo (model call → tools → failing tool → retry → failed run).

## Key decisions

A few choices that shaped the rest, and why I made them:

- **Rust-first core** for the SDK and collector. It keeps overhead low, it is easy
  to embed in Rust agent runtimes (Venus is Rust), and the collector ships as a
  single static binary.
- **SQLite** for first-party local storage. Zero-config, durable, and a good fit
  for single-node developer use. It is indexed by trace, time, type and provider.
- **OpenTelemetry-shaped** ids and an OTLP/JSON export mapping, for
  interoperability, without taking on the full weight of the OTel SDK.
- **Redaction twice** (SDK and collector), as defense in depth.
- **No hidden async runtime in the SDK**. The HTTP exporter uses a lightweight
  blocking client on a background thread, so instrumenting a synchronous agent
  does not drag in an executor.

For the data model see [`schema.md`](schema.md), and for the security model see
[`privacy.md`](privacy.md).
