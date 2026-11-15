// Display formatting helpers.

export function formatDuration(ms: number): string {
  if (!isFinite(ms) || ms < 0) return "-";
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${ms.toFixed(ms < 10 ? 2 : 0)}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(2)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${(s % 60).toFixed(0)}s`;
}

export function formatCost(cost: number): string {
  if (!cost) return "$0";
  if (cost < 0.01) return `$${cost.toFixed(4)}`;
  return `$${cost.toFixed(2)}`;
}

export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}

export function formatRelativeTime(nanos: number): string {
  const ms = nanos / 1_000_000;
  const diff = Date.now() - ms;
  if (diff < 0) return "just now";
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

export function formatAbsoluteTime(nanos: number): string {
  const ms = nanos / 1_000_000;
  return new Date(ms).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function shortId(id: string, len = 8): string {
  return id.slice(0, len);
}

export const SPAN_TYPE_LABEL: Record<string, string> = {
  agent_run: "Run",
  model_call: "Model",
  tool_call: "Tool",
  approval: "Approval",
  retry: "Retry",
  error: "Error",
};
