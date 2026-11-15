import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { getTrace, exportUrl, type Span } from "../api.ts";
import { Loading, ErrorState, SpanTypeTag, StatusBadge } from "../components/common.tsx";
import { useAsync } from "../hooks.ts";
import {
  formatAbsoluteTime,
  formatCost,
  formatDuration,
  formatTokens,
  shortId,
  SPAN_TYPE_LABEL,
} from "../format.ts";

interface Row {
  span: Span;
  depth: number;
  leftPct: number;
  widthPct: number;
  durMs: number;
}

function endOf(s: Span): number {
  return s.end_time_unix_nanos ?? s.start_time_unix_nanos;
}

/** Build an ordered, indented row list with waterfall geometry. */
function buildRows(spans: Span[]): { rows: Row[]; minStart: number; totalMs: number } {
  if (spans.length === 0) return { rows: [], minStart: 0, totalMs: 0 };
  const minStart = Math.min(...spans.map((s) => s.start_time_unix_nanos));
  const maxEnd = Math.max(...spans.map(endOf));
  const totalNs = Math.max(maxEnd - minStart, 1);

  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const children = new Map<string | undefined, Span[]>();
  for (const s of spans) {
    const key = s.parent_span_id && byId.has(s.parent_span_id) ? s.parent_span_id : undefined;
    const arr = children.get(key) ?? [];
    arr.push(s);
    children.set(key, arr);
  }
  for (const arr of children.values()) arr.sort((a, b) => a.start_time_unix_nanos - b.start_time_unix_nanos);

  const rows: Row[] = [];
  const walk = (parent: string | undefined, depth: number) => {
    for (const s of children.get(parent) ?? []) {
      const startNs = s.start_time_unix_nanos - minStart;
      const durNs = Math.max(endOf(s) - s.start_time_unix_nanos, 0);
      rows.push({
        span: s,
        depth,
        leftPct: (startNs / totalNs) * 100,
        widthPct: Math.max((durNs / totalNs) * 100, 0.6),
        durMs: durNs / 1_000_000,
      });
      walk(s.span_id, depth + 1);
    }
  };
  walk(undefined, 0);
  return { rows, minStart, totalMs: totalNs / 1_000_000 };
}

export default function TraceDetail() {
  const { traceId } = useParams();
  const q = useAsync(() => getTrace(traceId!), [traceId]);

  if (q.loading) return <Loading what="Loading trace" />;
  if (q.error) return <ErrorState error={q.error} />;
  const spans = q.data!.spans;

  return <TraceView traceId={traceId!} spans={spans} />;
}

function TraceView({ traceId, spans }: { traceId: string; spans: Span[] }) {
  const { rows, totalMs } = useMemo(() => buildRows(spans), [spans]);
  const root = rows.find((r) => r.depth === 0)?.span ?? spans[0];
  const firstFailed = spans.find((s) => s.status === "error");

  const [selectedId, setSelectedId] = useState<string>(firstFailed?.span_id ?? root.span_id);
  const selected = spans.find((s) => s.span_id === selectedId) ?? root;

  // --- replay: step through spans in execution (start) order. Read-only. ---
  const ordered = useMemo(
    () => [...spans].sort((a, b) => a.start_time_unix_nanos - b.start_time_unix_nanos),
    [spans],
  );
  const [replayIdx, setReplayIdx] = useState<number>(-1);
  const [playing, setPlaying] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    if (!playing) return;
    timer.current = window.setInterval(() => {
      setReplayIdx((i) => {
        const next = i + 1;
        if (next >= ordered.length) {
          setPlaying(false);
          return ordered.length - 1;
        }
        return next;
      });
    }, 700);
    return () => {
      if (timer.current) window.clearInterval(timer.current);
    };
  }, [playing, ordered.length]);

  useEffect(() => {
    if (replayIdx >= 0 && ordered[replayIdx]) setSelectedId(ordered[replayIdx].span_id);
  }, [replayIdx, ordered]);

  const replayActiveId = replayIdx >= 0 ? ordered[replayIdx]?.span_id : undefined;

  const errorCount = spans.filter((s) => s.status === "error").length;
  const tokens = spans.reduce((a, s) => a + (s.input_tokens ?? 0) + (s.output_tokens ?? 0), 0);
  const cost = spans.reduce((a, s) => a + (s.estimated_cost ?? 0), 0);
  const status = errorCount > 0 ? "error" : spans.some((s) => !s.end_time_unix_nanos) ? "running" : "ok";

  return (
    <>
      <div className="crumb">
        <Link to="/traces">Traces</Link> <span className="faint">/</span>{" "}
        <span className="mono">{shortId(traceId, 16)}</span>
      </div>

      <div className="page-head">
        <div>
          <div className="row gap" style={{ marginBottom: 6 }}>
            <StatusBadge status={status} />
            <h1 style={{ margin: 0 }}>{root.operation_name}</h1>
          </div>
          <div className="sub">
            {spans.length} spans · {formatDuration(totalMs)} · {formatTokens(tokens)} tokens ·{" "}
            {formatCost(cost)}
            {errorCount > 0 && <span style={{ color: "var(--error)" }}> · {errorCount} failed</span>}
          </div>
        </div>
        <div className="row">
          <a className="btn" href={exportUrl(traceId, "native")} download={`${shortId(traceId, 12)}.json`}>
            ↓ JSON
          </a>
          <a className="btn" href={exportUrl(traceId, "otlp")} download={`${shortId(traceId, 12)}.otlp.json`}>
            ↓ OTLP
          </a>
        </div>
      </div>

      <div className="detail-grid">
        <div className="panel">
          <div className="panel-head">
            Execution waterfall
            <ReplayControls
              idx={replayIdx}
              total={ordered.length}
              playing={playing}
              onPlay={() => {
                if (replayIdx >= ordered.length - 1) setReplayIdx(-1);
                setPlaying((p) => !p);
              }}
              onStep={(d) => {
                setPlaying(false);
                setReplayIdx((i) => Math.min(Math.max(i + d, 0), ordered.length - 1));
              }}
              onReset={() => {
                setPlaying(false);
                setReplayIdx(-1);
              }}
            />
          </div>
          <Legend spans={spans} />
          <div className="waterfall">
            {rows.map((r) => {
              const failed = r.span.status === "error";
              return (
                <div
                  key={r.span.span_id}
                  className={`wf-row${selectedId === r.span.span_id ? " selected" : ""}${
                    failed ? " failed" : ""
                  }${replayActiveId === r.span.span_id ? " replay-active" : ""}`}
                  onClick={() => setSelectedId(r.span.span_id)}
                >
                  <div className="wf-label" style={{ paddingLeft: 12 + r.depth * 16 }}>
                    <span className={`swatch ${r.span.span_type}`} />
                    <span className="wf-name" title={r.span.operation_name}>
                      {r.span.operation_name}
                    </span>
                  </div>
                  <div className="wf-track">
                    <div
                      className={`bar ${r.span.span_type}${failed ? " failed" : ""}`}
                      style={{ left: `${r.leftPct}%`, width: `${r.widthPct}%` }}
                    >
                      <span className="bar-dur">{formatDuration(r.durMs)}</span>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="panel span-detail">
          <SpanDetail span={selected} />
        </div>
      </div>
    </>
  );
}

function ReplayControls({
  idx,
  total,
  playing,
  onPlay,
  onStep,
  onReset,
}: {
  idx: number;
  total: number;
  playing: boolean;
  onPlay: () => void;
  onStep: (d: number) => void;
  onReset: () => void;
}) {
  return (
    <div className="replay" title="Replay the recorded execution order (does not run anything)">
      <button className="btn ghost" onClick={() => onStep(-1)} aria-label="Step back">
        ⏮
      </button>
      <button className="btn" onClick={onPlay} aria-label={playing ? "Pause" : "Play"}>
        {playing ? "⏸" : "▶"} Replay
      </button>
      <button className="btn ghost" onClick={() => onStep(1)} aria-label="Step forward">
        ⏭
      </button>
      <span className="step faint">{idx < 0 ? `0 / ${total}` : `${idx + 1} / ${total}`}</span>
      {idx >= 0 && (
        <button className="btn ghost" onClick={onReset}>
          ↺
        </button>
      )}
    </div>
  );
}

function Legend({ spans }: { spans: Span[] }) {
  const present = Array.from(new Set(spans.map((s) => s.span_type)));
  return (
    <div className="legend">
      {present.map((t) => (
        <span key={t} className="tag">
          <span className={`swatch ${t}`} />
          {SPAN_TYPE_LABEL[t] ?? t}
        </span>
      ))}
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

function SpanDetail({ span }: { span: Span }) {
  const dur =
    span.end_time_unix_nanos != null
      ? (span.end_time_unix_nanos - span.start_time_unix_nanos) / 1_000_000
      : null;
  const attrs = span.attributes ?? {};
  const hasAttrs = Object.keys(attrs).length > 0;

  return (
    <>
      <div className="panel-head">
        <SpanTypeTag type={span.span_type} />
        <StatusBadge status={span.status === "unset" ? "running" : span.status} />
      </div>

      <dl className="kv">
        <Field label="Operation">
          <span style={{ fontWeight: 500 }}>{span.operation_name}</span>
        </Field>
        <Field label="Span id">
          <span className="mono muted">{span.span_id}</span>
        </Field>
        {span.parent_span_id && (
          <Field label="Parent">
            <span className="mono muted">{shortId(span.parent_span_id, 16)}</span>
          </Field>
        )}
        <Field label="Started">{formatAbsoluteTime(span.start_time_unix_nanos)}</Field>
        <Field label="Duration">{dur != null ? formatDuration(dur) : <span className="faint">running…</span>}</Field>
        {span.provider && <Field label="Provider">{span.provider}</Field>}
        {span.model && (
          <Field label="Model">
            <span className="chip">{span.model}</span>
          </Field>
        )}
        {(span.input_tokens != null || span.output_tokens != null) && (
          <Field label="Tokens">
            <span className="mono">
              {formatTokens(span.input_tokens ?? 0)} in · {formatTokens(span.output_tokens ?? 0)} out
            </span>
          </Field>
        )}
        {span.estimated_cost != null && <Field label="Est. cost">{formatCost(span.estimated_cost)}</Field>}
        {span.tool_name && <Field label="Tool">{span.tool_name}</Field>}
        {span.retry_count != null && <Field label="Retry #">{span.retry_count}</Field>}
      </dl>

      {span.status === "error" && (
        <div className="error-box">
          <div className="etype">{span.error_type ?? "Error"}</div>
          {span.error_message && <div className="emsg">{span.error_message}</div>}
        </div>
      )}

      {hasAttrs && (
        <div className="attr-block">
          <h4>Attributes</h4>
          <pre className="json">{JSON.stringify(attrs, null, 2)}</pre>
        </div>
      )}

      {span.events && span.events.length > 0 && (
        <div className="attr-block">
          <h4>Events</h4>
          {span.events.map((e, i) => (
            <div key={i} className="mono muted" style={{ fontSize: 12 }}>
              • {e.name}
            </div>
          ))}
        </div>
      )}
    </>
  );
}
