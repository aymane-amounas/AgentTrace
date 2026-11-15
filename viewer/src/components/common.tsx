import { SPAN_TYPE_LABEL } from "../format.ts";
import type { SpanType } from "../api.ts";

export function StatusBadge({ status }: { status: string }) {
  const cls = status === "error" ? "error" : status === "running" ? "running" : "ok";
  const label = status === "running" ? "Running" : status === "error" ? "Error" : "OK";
  return (
    <span className={`badge ${cls}`}>
      <span className="dot" />
      {label}
    </span>
  );
}

export function SpanTypeTag({ type }: { type: SpanType }) {
  return (
    <span className="tag">
      <span className={`swatch ${type}`} />
      {SPAN_TYPE_LABEL[type] ?? type}
    </span>
  );
}

export function Loading({ what = "Loading" }: { what?: string }) {
  return (
    <div className="state">
      <div className="spinner" />
      <div className="muted">{what}…</div>
    </div>
  );
}

export function Empty({ title, hint, icon = "∅" }: { title: string; hint?: string; icon?: string }) {
  return (
    <div className="state">
      <div className="icon">{icon}</div>
      <h3>{title}</h3>
      {hint && <div className="muted">{hint}</div>}
    </div>
  );
}

export function ErrorState({ error }: { error: string }) {
  return (
    <div className="state error-state">
      <div className="icon">⚠</div>
      <h3>Couldn’t load data</h3>
      <div className="mono" style={{ fontSize: 12 }}>
        {error}
      </div>
      <div className="muted" style={{ fontSize: 12 }}>
        Is the collector running? Try <code>docker compose up</code> or{" "}
        <code>cargo run -p agenttrace-collector</code>.
      </div>
    </div>
  );
}
