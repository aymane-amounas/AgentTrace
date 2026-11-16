# Security Policy

## Reporting a vulnerability

Please report security issues privately via a
[GitHub security advisory](https://github.com/aymane-amounas/AgentTrace/security/advisories/new)
rather than a public issue. You'll get an acknowledgement as soon as possible.

## Security model (summary)

- The collector **binds to loopback by default** and has no authentication. Do
  not expose it publicly; put it behind an authenticated proxy or SSH tunnel if
  remote access is required.
- Secrets (authorization headers, API keys, tokens, passwords) are **redacted
  before persistence**, at both the SDK and the collector.
- Raw prompt/tool-argument payloads are **not stored by default**.
- Request bodies are size-limited and span ids are validated.

See [`docs/privacy.md`](docs/privacy.md) for details and limitations.

## Scope & limitations

Redaction is heuristic and may miss novel secret formats or secrets placed in
unexpected fields. Keep payload capture off for sensitive data. This project has
not undergone an independent security audit.
