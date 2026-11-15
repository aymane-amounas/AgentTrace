# Implementation status

Honest status of the first release (v0.1.0), as of 2026-11-15.

## Summary

The end-to-end pipeline is real and working: the SDK produces spans, the sample
agent generates realistic traces (including the signature failure), the collector
validates/redacts/stores them in SQLite, and the viewer renders them with a
waterfall, span tree, failure inspection, replay and export - verified in a real
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

`cargo test --workspace --all-features` → **39 passing**:
- SDK unit: 27 · SDK doc tests: 3 · collector unit: 6 · collector HTTP
  integration: 3 (incl. redaction regression tests for `sk-proj` keys and
  no-over-redaction of token counts).

Plus `cargo fmt --check`, `cargo clippy -D warnings` (clean), and
`npm run lint` / `npm run build` (clean).

## M7 - Release-hardening results (2026-11-15)

Validation performed after v0.1.0, treating the previous report as unverified.

- **Repo audit** - 39 tests pass, clippy `-D warnings` clean, viewer lint+build
  clean, remote tree verified (no build artifacts/DB/secrets committed).
- **Docker** - image build initially **failed** (base image `rust:1.83` too old;
  a transitive dep requires edition 2024). Fixed to `rust:1.90` and MSRV
  corrected to 1.85. Image now builds (169 MB) and the full Compose deployment was
  run and verified: health OK, viewer served, traces ingested, **persisted in
  SQLite across a container restart** (named volume), and the port is published to
  **127.0.0.1 only** (not public).
- **Venus integration** - verified against **real `venus-agent` code** via a
  harness (no changes to either repo): Venus's real capability orchestrator was
  driven through a successful `workspace.summary` and a failing
  `workspace.file_read`, producing a correct parent/child trace. Finding: Venus
  currently emits **no `tracing` spans** and has **no retry logic**, so deep
  per-model/tool tracing requires adding span annotations inside Venus (a
  reviewable change, not shipped here). A live LLM run needs the user's API key.
- **Security audit** - black-box injection against the running container found a
  **real leak**: modern OpenAI `sk-proj-…` keys (and the unflagged key name
  `openai_key`) slipped through because the regex stopped at the hyphen. **Fixed**
  (`sk-[A-Za-z0-9_-]{16,}` + sensitive-key substring matching) with regression
  tests at SDK and HTTP level; re-verified on the live container that `sk-proj`,
  `sk-ant`, and `db_password` are masked while benign `input_tokens` is preserved.
- **CI** - still **not active**: the `aymane-amounas` token lacks the `workflow`
  OAuth scope, so the workflow stays in `.github/workflows-pending/`. Activation
  requires `gh auth refresh -h github.com -s workflow` then moving the file.

## Not done / limitations

- **Full OTLP ingestion** - only export mapping is implemented; the collector
  does not yet accept inbound OTLP.
- **Active replay** - intentionally excluded. Replay is historical/read-only and
  never re-executes anything. Any future active replay must be a separate,
  explicitly authorized mechanism.
- **Venus span annotations** - the adapter mechanism is complete; annotating
  Venus's internal spans lives in the Venus repo, not here (no Venus fork/patch
  shipped).
- **Auth** - none; the collector relies on loopback binding.
- **Redaction** - heuristic; novel secret formats in unflagged fields could slip
  through (the M7 audit found and fixed one such case). Keep payload capture off
  for sensitive data. Redaction is applied at write time, so spans stored *before*
  a redaction-rule fix are not retroactively scrubbed.
- **Cost** - `estimated_cost` is an estimate from a static table, not billing.

## Recommended next milestone

Full OTLP/HTTP ingestion on the collector, so AgentTrace can receive traces from
any OTel-instrumented runtime alongside the native SDK.
