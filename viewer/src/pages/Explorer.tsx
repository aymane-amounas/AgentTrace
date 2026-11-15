import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { listTraces, type TraceSummary } from "../api.ts";
import { Loading, ErrorState, Empty, StatusBadge } from "../components/common.tsx";
import { useAsync } from "../hooks.ts";
import { formatCost, formatDuration, formatRelativeTime, formatTokens, shortId } from "../format.ts";

export default function Explorer() {
  const navigate = useNavigate();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("");
  const [spanType, setSpanType] = useState("");
  const [provider, setProvider] = useState("");
  const [minDur, setMinDur] = useState("");

  const q = useAsync(
    () =>
      listTraces({
        search: search || undefined,
        status: status || undefined,
        span_type: spanType || undefined,
        provider: provider || undefined,
        min_duration_ms: minDur ? Number(minDur) : undefined,
        limit: 200,
      }),
    [search, status, spanType, provider, minDur],
  );

  function reset() {
    setSearch("");
    setStatus("");
    setSpanType("");
    setProvider("");
    setMinDur("");
  }

  const active = search || status || spanType || provider || minDur;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Traces</h1>
          <div className="sub">Search, filter and open any agent run.</div>
        </div>
      </div>

      <div className="filters">
        <input
          className="inp search"
          placeholder="Search operation, tool or trace id…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select className="inp" value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">Any status</option>
          <option value="ok">OK</option>
          <option value="error">Error</option>
          <option value="running">Running</option>
        </select>
        <select className="inp" value={spanType} onChange={(e) => setSpanType(e.target.value)}>
          <option value="">Any span type</option>
          <option value="model_call">Has model call</option>
          <option value="tool_call">Has tool call</option>
          <option value="approval">Has approval</option>
          <option value="retry">Has retry</option>
        </select>
        <input
          className="inp"
          style={{ width: 150 }}
          placeholder="Provider"
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
        />
        <input
          className="inp"
          style={{ width: 130 }}
          type="number"
          placeholder="Min ms"
          value={minDur}
          onChange={(e) => setMinDur(e.target.value)}
        />
        {active && (
          <button className="btn ghost" onClick={reset}>
            Clear
          </button>
        )}
      </div>

      <div className="panel">
        {q.loading ? (
          <Loading what="Searching traces" />
        ) : q.error ? (
          <ErrorState error={q.error} />
        ) : q.data!.traces.length === 0 ? (
          <Empty title="No matching traces" hint={active ? "Try relaxing the filters." : "Run the sample agent to generate some."} icon="≋" />
        ) : (
          <Table traces={q.data!.traces} onOpen={(id) => navigate(`/traces/${id}`)} />
        )}
      </div>
    </>
  );
}

function Table({ traces, onOpen }: { traces: TraceSummary[]; onOpen: (id: string) => void }) {
  return (
    <table className="ttable">
      <thead>
        <tr>
          <th>Status</th>
          <th>Operation</th>
          <th>Models</th>
          <th className="num">Spans</th>
          <th className="num">Errors</th>
          <th className="num">Tokens</th>
          <th className="num">Cost</th>
          <th className="num">Duration</th>
          <th className="num">When</th>
        </tr>
      </thead>
      <tbody>
        {traces.map((t) => (
          <tr key={t.trace_id} onClick={() => onOpen(t.trace_id)}>
            <td>
              <StatusBadge status={t.status} />
            </td>
            <td>
              <div style={{ fontWeight: 500 }}>{t.root_operation ?? "—"}</div>
              <div className="mono faint" style={{ fontSize: 11 }}>
                {shortId(t.trace_id, 16)}
              </div>
            </td>
            <td>
              {t.models.length ? (
                t.models.map((m) => (
                  <span key={m} className="chip">
                    {m}
                  </span>
                ))
              ) : (
                <span className="faint">—</span>
              )}
            </td>
            <td className="num">{t.span_count}</td>
            <td className="num" style={{ color: t.error_count ? "var(--error)" : undefined }}>
              {t.error_count || "—"}
            </td>
            <td className="num">{formatTokens(t.input_tokens + t.output_tokens)}</td>
            <td className="num">{formatCost(t.estimated_cost)}</td>
            <td className="num">{formatDuration(t.duration_ms)}</td>
            <td className="num faint">{formatRelativeTime(t.start_time_unix_nanos)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
