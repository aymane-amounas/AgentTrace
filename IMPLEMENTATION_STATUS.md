# Implementation status

Honest status of the first release (v0.1.0), as of 2026-11-15.

## Summary

The end-to-end pipeline is real and working: the SDK produces spans, the sample
agent generates realistic traces (including the signature failure), the collector
validates/redacts/stores them in SQLite, and the viewer renders them with a
waterfall, span tree, failure inspection, replay and export — verified in a real
browser.

## Milestones

| # | Milestone | Status |
| - | --------- | ------ |
| M0 | Workspace, toolchain, structure | ✅ Done |
| M1 | Trace schema + SDK | ✅ Done |
| M2 | Instrumentation + sample agent | ✅ Done |
| M3 | Collector + SQLite + redaction | ✅ Done |
| M4 | Viewer (overview, explorer, detail, failure inspection) | ✅ Done |
| M5 | Historical replay + export + OTLP mapping | ✅ Done (export); OTLP *ingest* is roadmap |
| M6 | Docker, CI, docs | ✅ Authored; see Docker note below |

## What works (verified)

- **SDK**: ids, timing, status, parent/child, tokens/cost, events; concurrency
  isolation; redaction before export; in-memory / NDJSON / HTTP exporters;
  non-blocking HTTP export that drops rather than blocks.
- **Redaction**: key + value masking; payloads off by default; applied at SDK
  **and** collector. A test POSTs a raw secret as an untrusted client and asserts
  it never reaches storage.
- **Collector**: ingest (wrapper/array/single), validation, idempotent upsert,
  incomplete-span handling, filters (status/type/provider/model/search/time/
  duration), stats, native + OTLP export, retention sweep, viewer hosting,
  loopback-only default, body-size limit.
- **Viewer**: overview metrics, filterable explorer, trace detail with execution
  waterfall, parent/child tree, selected-span detail panel, failure highlighting,
  historical replay stepper (read-only), JSON/OTLP download. Loading / empty /
  error states throughout. Verified in a browser against live data.
- **Sample agent**: produces success traces and the failure demo (model → tools →
  failing tool → retry → failed run with root cause).
- **Venus integration**: `tracing-layer` bridge implemented and tested; wires in
  with no changes to Venus core (see `docs/venus-integration.md`).

## Tests

`cargo test --workspace --all-features` → **36 passing**:
- SDK unit: 24 · SDK doc tests: 3 · collector unit: 6 · collector HTTP
  integration: 3.

Plus `cargo fmt --check`, `cargo clippy -D warnings` (clean), and
`npm run lint` / `npm run build` (clean).

## Docker note (honesty)

The `Dockerfile` and `docker-compose.yml` are written and `docker compose config`
validates. The multi-stage image build was **not run locally** this session
because the Docker daemon was not running on the build machine (only the Docker
CLI was available). The image build is exercised by the `docker` CI job on every
push. Treat the container path as "authored and CI-validated, not yet smoke-run
locally" until a maintainer runs `docker compose up --build`.

## Not done / limitations

- **Full OTLP ingestion** — only export mapping is implemented; the collector
  does not yet accept inbound OTLP.
- **Active replay** — intentionally excluded. Replay is historical/read-only and
  never re-executes anything. Any future active replay must be a separate,
  explicitly authorized mechanism.
- **Venus span annotations** — the adapter mechanism is complete; annotating
  Venus's internal spans lives in the Venus repo, not here (no Venus fork/patch
  shipped).
- **Auth** — none; the collector relies on loopback binding.
- **Redaction** — heuristic; novel secret formats in unflagged fields could slip
  through. Keep payload capture off for sensitive data.
- **Cost** — `estimated_cost` is an estimate from a static table, not billing.

## Recommended next milestone

Full OTLP/HTTP ingestion on the collector, so AgentTrace can receive traces from
any OTel-instrumented runtime alongside the native SDK.
