import { useNavigate } from "react-router-dom";
import { getStats, listTraces, type Stats, type TraceSummary } from "../api.ts";
import { Loading, ErrorState, Empty, StatusBadge } from "../components/common.tsx";
import { useAsync } from "../hooks.ts";
import { formatCost, formatDuration, formatRelativeTime, formatTokens, shortId } from "../format.ts";

function Tile({ label, value, unit, err }: { label: string; value: string; unit?: string; err?: boolean }) {
  return (
    <div className="tile">
      <div className="label">{label}</div>
      <div className={`value${err ? " err" : ""}`}>
        {value}
        {unit && <span className="unit">{unit}</span>}
      </div>
    </div>
  );
}

export default function Overview() {
  const navigate = useNavigate();
  const stats = useAsync<Stats>(getStats, []);
  const recent = useAsync(() => listTraces({ limit: 8 }), []);

  if (stats.loading) return <Loading what="Loading overview" />;
  if (stats.error) return <ErrorState error={stats.error} />;
  const s = stats.data!;

  return (
    <>
      <div className="page-head">
        <div>
          <h1>Overview</h1>
          <div className="sub">Everything your agents did, understood at a glance.</div>
        </div>
        <button className="btn" onClick={() => { stats.reload(); recent.reload(); }}>
          ↻ Refresh
        </button>
      </div>

      <div className="tiles">
        <Tile label="Traces" value={String(s.trace_count)} />
        <Tile label="Spans" value={String(s.span_count)} />
        <Tile label="Failed traces" value={String(s.traces_with_errors)} err={s.traces_with_errors > 0} />
        <Tile label="Model calls" value={String(s.model_calls)} />
        <Tile label="Tool calls" value={String(s.tool_calls)} />
        <Tile label="Tokens" value={formatTokens(s.total_input_tokens + s.total_output_tokens)} />
        <Tile label="Est. cost" value={formatCost(s.total_estimated_cost)} />
        <Tile label="Avg duration" value={formatDuration(s.avg_trace_duration_ms)} />
      </div>

      <div className="panel">
        <div className="panel-head">
          Recent traces
          <button className="btn ghost" onClick={() => navigate("/traces")}>
            View all →
          </button>
        </div>
        {recent.loading ? (
          <Loading what="Loading traces" />
        ) : recent.error ? (
          <ErrorState error={recent.error} />
        ) : recent.data!.traces.length === 0 ? (
          <Empty
            title="No traces yet"
            hint="Run the sample agent: cargo run -p sample-agent"
            icon="≋"
          />
        ) : (
          <RecentTable traces={recent.data!.traces} onOpen={(id) => navigate(`/traces/${id}`)} />
        )}
      </div>
    </>
  );
}

function RecentTable({ traces, onOpen }: { traces: TraceSummary[]; onOpen: (id: string) => void }) {
  return (
    <table className="ttable">
      <thead>
        <tr>
          <th>Status</th>
          <th>Operation</th>
          <th>Trace</th>
          <th className="num">Spans</th>
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
            <td>{t.root_operation ?? <span className="faint">—</span>}</td>
            <td className="mono muted">{shortId(t.trace_id, 12)}</td>
            <td className="num">{t.span_count}</td>
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
