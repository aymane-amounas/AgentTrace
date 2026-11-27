# Contributing

Thanks for taking a look at AgentTrace.

For the workspace layout and local setup, start with
[`docs/development.md`](docs/development.md).

## Before you open a PR

Run the same checks CI runs, so nothing surprises you later:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm --prefix viewer run lint
npm --prefix viewer run build
```

## Guidelines

A few things I care about:

- Keep the SDK, collector, storage and viewer independently testable.
- Add tests when you change behavior. I value realistic tests (failed calls,
  retries, nested tools, interrupted runs) over mocks.
- Never weaken the redaction defaults or log redacted values. Any security change
  has to come with tests proving secrets stay out of storage.
- Keep dependencies lean, and justify a new one when you add it.
- Conventional-ish commit messages (`feat:`, `fix:`, `docs:`, and so on) are
  appreciated.

## Code of conduct

Be respectful and constructive. That is the whole policy.
