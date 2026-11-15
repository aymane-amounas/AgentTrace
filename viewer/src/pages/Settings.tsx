import { getConfig, type RuntimeConfig } from "../api.ts";
import { Loading, ErrorState } from "../components/common.tsx";
import { useAsync } from "../hooks.ts";

function Yes({ v }: { v: boolean }) {
  return <span className={`badge ${v ? "ok" : "error"}`}><span className="dot" />{v ? "On" : "Off"}</span>;
}

export default function Settings() {
  const q = useAsync<RuntimeConfig>(getConfig, []);
  if (q.loading) return <Loading what="Loading settings" />;
  if (q.error) return <ErrorState error={q.error} />;
  const c = q.data!;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Privacy &amp; runtime</h1>
          <div className="sub">How this collector handles your trace data. Configured via environment variables.</div>
        </div>
      </div>

      <div className="panel" style={{ maxWidth: 640 }}>
        <div className="panel-head">Effective settings</div>
        <dl className="kv" style={{ gridTemplateColumns: "220px 1fr" }}>
          <dt>Raw payload capture</dt>
          <dd>
            <Yes v={c.capture_payloads} />
            <div className="faint" style={{ fontSize: 12, marginTop: 4 }}>
              When off, prompts and tool arguments are dropped before storage. Secrets are always masked.
            </div>
          </dd>
          <dt>Bind address</dt>
          <dd className="mono">{c.bind_addr}</dd>
          <dt>Loopback only</dt>
          <dd>
            <Yes v={c.loopback_only} />
          </dd>
          <dt>Retention</dt>
          <dd>{c.retention_days > 0 ? `${c.retention_days} days` : "disabled"}</dd>
          <dt>Max request body</dt>
          <dd className="mono">{(c.max_body_bytes / (1024 * 1024)).toFixed(1)} MiB</dd>
          <dt>Schema version</dt>
          <dd className="mono">{c.schema_version}</dd>
        </dl>
      </div>

      <div className="panel" style={{ maxWidth: 640, marginTop: 16 }}>
        <div className="panel-head">What AgentTrace never stores</div>
        <div style={{ padding: "14px 16px", color: "var(--text-dim)", fontSize: 13, lineHeight: 1.7 }}>
          Authorization headers, API keys, tokens and passwords are redacted at the SDK and again at the
          collector, before anything is written to disk. Raw prompt and tool-argument payloads are dropped by
          default. Redaction happens before persistence - not just before display.
        </div>
      </div>
    </>
  );
}
