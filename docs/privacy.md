# Privacy and security model

Traces carry sensitive data, so I treat redaction as a core feature rather than a
display filter. It runs before anything is persisted.

## What is redacted

Two layers run together, both in `crates/agenttrace/src/redact.rs`.

The first is key rules. Any attribute whose key (case-insensitive) is in the
sensitive list has its value replaced with `[REDACTED]`. The list covers
`authorization`, `api_key`, `x-api-key`, `secret`, `client_secret`, `password`,
`token`, `access_token`, `refresh_token`, `private_key`, `cookie`, `credentials`,
and more. Key matching also catches sensitive substrings (for example
`db_password`, `openai_api_key`), and I chose the matching to avoid clobbering
benign fields like `input_tokens`.

The second is value rules. Every remaining string is scanned for secret-shaped
substrings and masked: bearer tokens, OpenAI/Anthropic `sk-…` keys (including the
hyphenated `sk-proj-…` and `sk-svcacct-…` forms), AWS access keys (`AKIA…`),
Google API keys (`AIza…`), GitHub (`ghp_…`), Slack (`xox…`), and JWTs.

## Raw payloads are off by default

Keys that usually hold raw payloads (`prompt`, `messages`, `input`, `output`,
`completion`, `response`, `arguments`, `content`, and so on) are dropped to
`[omitted: payload capture disabled]` unless you opt in with
`AGENTTRACE_CAPTURE_PAYLOADS=true` (or `RedactionPolicy::allow_payloads()` in the
SDK). Even with capture on, the key and value secret rules still run.

## Redaction happens twice (defense in depth)

The SDK redacts every span at a single choke point before any exporter sees it
(`Span::finish`). The collector then validates and redacts again at the storage
boundary (`Storage::store_span`), so a misconfigured or untrusted client that
POSTs raw secrets still cannot land them in SQLite.

There is a test for this, one that POSTs a raw secret as an untrusted client and
asserts it never shows up in storage
(`crates/agenttrace-collector/tests/api.rs`).

## Fail closed

After the storage-boundary redaction pass, the serialized span is scanned one more
time for known secret shapes (`Storage::store_span`). If a secret still matches,
the span is rejected and not written, rather than persisted raw. This is the
backstop for a payload or key shape the rules did not anticipate: the worst case is
a dropped span, never a leaked one. It is covered by
`storage::tests::fails_closed_when_policy_misses_a_secret`.

## Network and input safety

- The collector binds to loopback (`127.0.0.1`) by default. Binding anywhere else
  is an explicit, logged choice, and the Docker image only ever publishes to
  `127.0.0.1`.
- CORS is scoped to local origins (`localhost`/`127.0.0.1`/`[::1]`), not left
  open, so a page on some other site you are browsing cannot read traces off the
  loopback collector cross-origin. Same-origin (the collector serving its own
  built viewer) needs no CORS.
- Responses carry security headers: `X-Content-Type-Options: nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, and a tight
  `Content-Security-Policy` (`default-src 'self'`, no inline scripts,
  `frame-ancestors 'none'`).
- Internal errors are logged server-side, and the API returns a generic `internal
  error` instead of echoing SQLite or other internal error text.
- Request bodies are size-limited (default 4 MiB).
- Trace ids are validated (32 hex chars) before any query runs.
- A malformed span is rejected on its own without failing the rest of the batch.

## Limitations, honestly

- Redaction is pattern and heuristic based. A novel secret format, or a secret
  sitting in an unexpected field that is not flagged, can slip past the key and
  value rules. The fail-closed scan still catches known secret shapes even then,
  but a credential with no recognizable shape (say a plain password buried in a
  connection string like `postgres://user:pw@host`) is not caught by the value
  rules. So keep payload capture off for anything sensitive, and add custom rules
  for your own formats.
- Redaction runs at write time. Spans stored before a rule change are not
  re-scrubbed retroactively; drop the database to purge them.
- `estimated_cost` is an estimate from a static price table, not billing truth.
- The collector has no authentication; it relies on loopback binding. Do not
  expose it publicly.
- This project has not had an independent security audit.

## For contributors

Never log the values you redact while you are debugging the redaction system. For
reporting, see [`SECURITY.md`](../SECURITY.md).
