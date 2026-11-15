# Contributing

Thanks for your interest in AgentTrace.

## Getting started

See [`docs/development.md`](docs/development.md) for the workspace layout and
local setup.

## Before you open a PR

Run the same checks CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm --prefix viewer run lint
npm --prefix viewer run build
```

## Guidelines

- Keep the SDK, collector, storage and viewer independently testable.
- Add tests with behavior changes. Realistic tests (failed calls, retries,
  nested tools, interrupted runs) are valued over mocks.
- **Never** weaken redaction defaults or log redacted values. Security changes
  must include tests proving secrets stay out of storage.
- Keep dependencies lean; justify new ones.
- Conventional-ish commit messages (`feat:`, `fix:`, `docs:`, …) are appreciated.

## Code of conduct

Be respectful and constructive.
