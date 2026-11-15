# Privacy & security model

Traces are sensitive. AgentTrace treats redaction as a core feature, applied
**before persistence**, not just before display.

## What is redacted

Two layers run together (`crates/agenttrace/src/redact.rs`):

1. **Key rules.** Any attribute whose key (case-insensitive) is in the sensitive
   list — `authorization`, `api_key`, `x-api-key`, `secret`, `client_secret`,
   `password`, `token`, `access_token`, `refresh_token`, `private_key`,
   `cookie`, `credentials`, … — has its value replaced with `[REDACTED]`.
2. **Value rules.** Every remaining string is scanned for secret-shaped
   substrings and masked: bearer tokens, OpenAI/Anthropic `sk-…` keys, AWS access
   keys (`AKIA…`), Google API keys (`AIza…`), GitHub (`ghp_…`), Slack (`xox…`),
   and JWTs.

## Raw payloads are off by default

Keys that typically hold raw payloads — `prompt`, `messages`, `input`, `output`,
`completion`, `response`, `arguments`, `content`, … — are dropped to
`[omitted: payload capture disabled]` unless you opt in with
`AGENTTRACE_CAPTURE_PAYLOADS=true` (or `RedactionPolicy::allow_payloads()` in the
SDK). Even with capture on, key and value secret rules still apply.

## Redaction happens twice (defense in depth)

- **SDK:** every span is redacted at a single choke point before any exporter
  sees it (`Span::finish`).
- **Collector:** every span is validated and redacted **again** at the storage
  boundary (`Storage::store_span`), so a misconfigured or untrusted client that
  POSTs raw secrets still cannot land them in SQLite.

This is covered by tests, including one that POSTs a raw secret as an untrusted
client and asserts it never appears in storage
(`crates/agenttrace-collector/tests/api.rs`).

## Network & input safety

- The collector **binds to loopback (`127.0.0.1`) by default**. Binding elsewhere
  is an explicit, logged choice; the Docker image publishes only to `127.0.0.1`.
- Request bodies are size-limited (default 4 MiB).
- Trace ids are validated (32 hex chars) before any query.
- Malformed spans are rejected per-span without failing the whole batch.

## Limitations (be honest about these)

- Redaction is pattern/heuristic based. A novel secret format or a secret placed
  in an unexpected, non-flagged field could slip through. Keep payload capture
  off for anything sensitive, and add custom rules for your own formats.
- `estimated_cost` is an estimate from a static price table, not billing truth.
- The collector has no authentication; it relies on loopback binding. Do not
  expose it publicly.
- This project has not had an independent security audit.

## For contributors

Never log the values you redact while debugging the redaction system. See
[`SECURITY.md`](../SECURITY.md) for reporting.
