# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-15

First release.

### Added
- **SDK (`agenttrace`)** - trace schema v0.1.0 (run, model call, tool call,
  approval, retry, error), OTel-style ids, `Tracer`/`Span` API with concurrency
  -safe context propagation, token/cost metadata, and events.
- **Redaction** - secret masking by key and value pattern, raw-payload capture
  off by default, applied before export.
- **Exporters** - in-memory, NDJSON (file/stdout), and non-blocking HTTP.
- **OTLP/JSON export mapping** using `gen_ai.*`/`agenttrace.*` conventions.
- **Collector (`agenttrace-collector`)** - axum + SQLite ingest/query/export
  API, validation and redaction at the persistence boundary, idempotent upserts,
  retention sweeping, and viewer hosting. Loopback-only by default.
- **Viewer** - React + Vite app: overview metrics, filterable trace explorer,
  trace detail with execution waterfall, parent/child span tree, failure
  inspection, historical replay, and JSON/OTLP export.
- **Sample agent** - fully instrumented demo producing the signature
  failure trace (model call → tools → failing tool → retry → failed run).
- **Venus integration** - `tracing-layer` bridge requiring no changes to Venus.
- **Deployment** - Dockerfile + Compose, CI (fmt, clippy, tests, viewer build,
  Docker build, dependency audit), and docs.

### Security
- Redaction now masks hyphenated OpenAI key formats (`sk-proj-…`, `sk-svcacct-…`)
  and matches sensitive key-name substrings (e.g. `db_password`, `openai_api_key`),
  without over-redacting benign fields like `input_tokens`. Found and fixed during
  release-hardening with regression tests at the SDK and HTTP boundaries.

### Fixed
- Docker base image bumped to `rust:1.90` (MSRV 1.85): a transitive dependency
  requires Rust edition 2024, which the previous `rust:1.83` base could not build.
- Bumped `rusqlite` 0.32 -> 0.43 to pull patched `libsqlite3-sys` >= 0.40,
  resolving RUSTSEC-2026-0459 (medium). CI now runs `cargo audit` as a real gate.

[0.1.0]: https://github.com/aymane-amounas/AgentTrace/releases/tag/v0.1.0
