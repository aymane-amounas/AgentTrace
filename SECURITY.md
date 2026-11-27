# Security Policy

## Reporting a vulnerability

If you find a security issue, please report it privately through a
[GitHub security advisory](https://github.com/aymane-amounas/AgentTrace/security/advisories/new)
rather than opening a public issue. I will acknowledge it as soon as I can.

## Security model (summary)

The collector binds to loopback by default and has no authentication. Do not
expose it publicly. If you need remote access, put it behind an authenticated
proxy or an SSH tunnel.

Secrets (authorization headers, API keys, tokens, passwords) are redacted before
persistence, at both the SDK and the collector, and raw prompt and tool-argument
payloads are not stored by default. Request bodies are size-limited and span ids
are validated.

There is more detail, including the limits, in
[`docs/privacy.md`](docs/privacy.md).

## Scope and limitations

Redaction is heuristic, so it can miss a novel secret format or a secret placed in
an unexpected field. Keep payload capture off for sensitive data. This project has
not had an independent security audit.
