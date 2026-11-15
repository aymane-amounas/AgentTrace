# Deployment

## Docker (recommended)

```bash
docker compose up --build -d
```

This builds the viewer, compiles the collector, and runs it. The API and viewer
are then at <http://127.0.0.1:4357/>. The compose file publishes the port to
**loopback only**.

- Data persists in the named volume `agenttrace-data` (`/data/agenttrace.db`).
- Health: `curl http://127.0.0.1:4357/healthz`.
- Logs: `docker compose logs -f`.
- Stop: `docker compose down` (keeps the volume). Remove data with
  `docker compose down -v`.

### Backup & recovery

The database is a single SQLite file in the volume.

```bash
# Backup
docker compose exec collector sh -c 'cat /data/agenttrace.db' > backup.db
# Restore (collector stopped)
docker compose cp backup.db collector:/data/agenttrace.db
```

For a local (non-Docker) run, just copy `agenttrace.db` (and `-wal`/`-shm` if
present) while the collector is stopped.

## Local (no Docker)

```bash
# 1. Build the viewer
npm --prefix viewer install
npm --prefix viewer run build

# 2. Run the collector (serves API + viewer)
cargo run --release -p agenttrace-collector

# 3. Produce some traces
cargo run -p sample-agent
```

Open <http://127.0.0.1:4357/>.

## Configuration

All configuration is via environment variables (safe defaults shown):

| Variable                      | Default         | Purpose |
| ----------------------------- | --------------- | ------- |
| `AGENTTRACE_HOST`             | `127.0.0.1`     | Bind host (loopback by default) |
| `AGENTTRACE_PORT`             | `4357`          | Bind port |
| `AGENTTRACE_DB`               | `agenttrace.db` | SQLite path |
| `AGENTTRACE_VIEWER_DIR`       | `viewer/dist`   | Built viewer to serve |
| `AGENTTRACE_CAPTURE_PAYLOADS` | `false`         | Keep raw prompts/tool args (secrets still masked) |
| `AGENTTRACE_RETENTION_DAYS`   | `30`            | Prune spans older than N days (0 = off) |
| `AGENTTRACE_MAX_BODY_BYTES`   | `4194304`       | Max request body |
| `RUST_LOG`                    | `info`          | Log level (tracing EnvFilter) |

## Security notes

- Never expose the collector publicly. It has no auth and relies on loopback
  binding. If you must reach it remotely, put it behind an authenticated reverse
  proxy or an SSH tunnel.
- Do not hardcode secrets in Compose files or images; none are required.
