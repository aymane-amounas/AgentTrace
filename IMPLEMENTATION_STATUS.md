# Implementation status (v0.1.0)

This is an honest account of what is actually implemented, what I have verified,
and what is still open. I would rather undersell it here than have you find the
gaps later.

## What works (verified)

- **SDK** - OTel-style ids, timing, status, parent/child structure, token and cost
  metadata, and events. Concurrent runs stay isolated, redaction runs before
  export, and the in-memory, NDJSON and non-blocking HTTP exporters drop spans
  rather than block the agent.
- **Redaction** - secret masking by key and by value, raw payloads off by default,
  applied at both the SDK and the collector (the persistence boundary). One test
  POSTs a raw secret as an untrusted client and asserts it never reaches storage.
- **Collector** - ingest (wrapper object, array, or single span), validation,
  idempotent upsert, incomplete-span handling, filters (status, type, provider,
  model, search, time, duration), stats, native and OTLP export, retention sweep,
  viewer hosting, loopback-only default, and a body-size limit.
- **Viewer** - overview metrics, a filterable explorer, trace detail with the
  execution waterfall, parent/child span tree, a selected-span detail panel,
  failure highlighting, historical replay (read-only), JSON/OTLP download, and
  loading, empty and error states throughout.
- **Sample agent** - produces both successful traces and the signature failure
  demo: a model call, tools, a failing tool, a retry, and a failed run with its
  root cause.
- **Venus integration** - a `tracing`-layer bridge that needs no changes to Venus
  core. I verified it against real `venus-agent` code (see
  [`docs/venus-integration.md`](docs/venus-integration.md)).
- **Deployment** - Docker image and Compose, verified end to end: health, viewer,
  ingest, SQLite persistence across a restart, and loopback-only binding.

## Tests

`cargo test --workspace --all-features` gives **50 passing**: SDK unit 27, SDK doc
3, SDK redaction integration 8, collector unit 8, and collector HTTP integration 4
(which includes the redaction regression tests). CI also runs
`cargo fmt --check`, `cargo clippy -D warnings`, the viewer lint and build, a
Docker build, and `cargo audit`.

## Limitations

- **OTLP ingestion** - only the export mapping exists; the collector does not yet
  accept inbound OTLP traffic.
- **Replay is historical only** - it reconstructs recorded execution and never
  re-runs anything. If I ever add active replay it will be a separate, explicitly
  authorized mechanism.
- **Venus spans** - the adapter mechanism is complete, but Venus does not emit its
  own spans yet, so deep tracing still needs annotations added inside Venus.
- **Auth** - there is none. The collector relies on loopback binding and is not
  meant to be exposed publicly.
- **Redaction** - it is pattern and heuristic based, so keep payload capture off
  for anything sensitive. It also runs at write time, so spans stored before a
  rule change are not retroactively scrubbed.
- **Cost** - `estimated_cost` comes from a static price table. It is an estimate,
  not billing.

## Next milestone

Full OTLP/HTTP ingestion, so AgentTrace can receive traces from any
OTel-instrumented runtime alongside the native SDK.
