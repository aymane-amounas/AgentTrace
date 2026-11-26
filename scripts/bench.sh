#!/usr/bin/env bash
# Local, reproducible perf probe for the collector: ingestion throughput, query
# latency and storage per span/trace. Not a rigorous benchmark; single machine,
# loopback, SQLite WAL, payloads off. Fill the README table from its output.
set -euo pipefail

PORT="${AGENTTRACE_PORT:-4399}"
DB="$(mktemp -d)/bench.db"
BASE="http://127.0.0.1:${PORT}"

cargo build --release -p agenttrace-collector >/dev/null
AGENTTRACE_DB="$DB" AGENTTRACE_PORT="$PORT" ./target/release/agenttrace-collector >/dev/null 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null || true; rm -rf "$(dirname "$DB")"' EXIT
sleep 1

python3 - "$DB" "$BASE" <<'PY'
import json, time, os, sys, glob, random, urllib.request
db, base = sys.argv[1], sys.argv[2]
hx = lambda n: ''.join(random.choice('0123456789abcdef') for _ in range(n))

spans, traces = [], 150  # ~7 spans/trace, matching the sample agent
for t in range(traces):
    tid, root = hx(32), hx(16)
    def mk(st, name, sid, parent, extra=None):
        s = {"schema_version": "1", "trace_id": tid, "span_id": sid,
             "span_type": st, "operation_name": name,
             "start_time_unix_nanos": 1700000000000000000 + t*1000000,
             "end_time_unix_nanos": 1700000000002000000 + t*1000000, "status": "ok"}
        if parent: s["parent_span_id"] = parent
        if extra: s.update(extra)
        return s
    spans.append(mk("agent_run", "run", root, None))
    for _ in range(6):
        spans.append(mk("model_call", "chat", hx(16), root,
                        {"provider": "anthropic", "model": "claude-opus-4",
                         "input_tokens": 820, "output_tokens": 190, "estimated_cost": 0.012}))

t0, sent = time.time(), 0
for i in range(0, len(spans), 256):
    batch = spans[i:i+256]
    req = urllib.request.Request(base + "/v1/spans",
                                 data=json.dumps({"spans": batch}).encode(),
                                 headers={"content-type": "application/json"})
    urllib.request.urlopen(req).read(); sent += len(batch)
dt = time.time() - t0
print(f"ingestion: {sent} spans in {dt*1000:.0f} ms => {sent/dt:,.0f} spans/sec")

def p50(url, n=25):
    ts = []
    for _ in range(n):
        a = time.time(); urllib.request.urlopen(base + url).read(); ts.append((time.time()-a)*1000)
    ts.sort(); return ts[len(ts)//2]
print(f"GET /v1/traces      p50 = {p50('/v1/traces?limit=50'):.1f} ms")
tid = json.loads(urllib.request.urlopen(base + '/v1/traces?limit=1').read())["traces"][0]["trace_id"]
print(f"GET /v1/traces/:id  p50 = {p50('/v1/traces/' + tid):.1f} ms")

urllib.request.urlopen(base + "/v1/stats").read()
sz = sum(os.path.getsize(f) for f in glob.glob(db + "*"))
print(f"storage: {sz:,} B for {len(spans)} spans => {sz/len(spans):.0f} B/span, {sz/traces:.0f} B/trace")
PY
