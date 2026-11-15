# Implementation status (v0.1.0)

An honest view of what is implemented, verified, and still open.

## What works (verified)

- **SDK** - OTel-style ids, timing, status, parent/child structure, token/cost
  metadata, events; concurrency isolation; redaction before export; in-memory,
  NDJSON and non-blocking HTTP exporters that drop rather than block.
- **Redaction** - secret masking by key and value, raw payloads off by default,
  applied at both the SDK and the collector (the persistence boundary). A test
  POSTs a raw secret as an untrusted client and asserts it never reaches storage.
- **Collector** - ingest (wrapper object / array / single span), validation,
  idempotent upsert, incomplete-span handling, filters (status, type, provider,
  model, search, time, duration), stats, native + OTLP export, retention sweep,
  viewer hosting, loopback-only default, body-size limit.
- **Viewer** - overview metrics, filterable explorer, trace detail with execution
  waterfall, parent/child span tree, selected-span detail panel, failure
  highlighting, historical replay (read-only), JSON/OTLP download, and loading /
  empty / error states throughout.
- **Sample agent** - produces successful traces and the signature failure demo
  (model call, tools, a failing tool, a retry, a failed run with root cause).
- **Venus integration** - a `tracing`-layer bridge that needs no changes to Venus
  core. Verified against real `venus-agent` code (see `docs/venus-integration.md`).
- **Deployment** - Docker image + Compose, verified end to end: health, viewer,
  ingest, SQLite persistence across a restart, and loopback-only binding.

## Tests

`cargo test --workspace --all-features` -> **39 passing** (SDK unit 27, SDK doc 3,
collector unit 6, collector HTTP integration 3, including redaction regression
tests). CI also runs `cargo fmt --check`, `cargo clippy -D warnings`, the viewer
lint/build, a Docker build, and `cargo audit`.

## Limitations

- **OTLP ingestion** - only the export mapping is implemented; the collector does
  not yet accept inbound OTLP traffic.
- **Replay is historical only** - it reconstructs recorded execution and never
  re-runs anything. Any future active replay must be a separate, explicitly
  authorized mechanism.
- **Venus spans** - the adapter mechanism is complete, but Venus does not yet emit
  its own spans; deep tracing needs annotations added inside Venus.
- **Auth** - none; the collector relies on loopback binding and is not meant to be
  exposed publicly.
- **Redaction** - pattern/heuristic based; keep payload capture off for anything
  sensitive. Redaction runs at write time, so spans stored before a rule change
  are not retroactively scrubbed.
- **Cost** - `estimated_cost` is an estimate from a static price table, not billing.

## Next milestone

Full OTLP/HTTP ingestion, so AgentTrace can receive traces from any
OTel-instrumented runtime alongside the native SDK.
