// API client + types mirroring the AgentTrace span schema (docs/schema.md).

export type SpanType =
  | "agent_run"
  | "model_call"
  | "tool_call"
  | "approval"
  | "retry"
  | "error";

export type SpanStatus = "unset" | "ok" | "error";

export interface SpanEvent {
  name: string;
  time_unix_nanos: number;
  attributes?: Record<string, unknown>;
}

export interface Span {
  schema_version: string;
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  span_type: SpanType;
  operation_name: string;
  start_time_unix_nanos: number;
  end_time_unix_nanos?: number;
  status: SpanStatus;
  provider?: string;
  model?: string;
  input_tokens?: number;
  output_tokens?: number;
  estimated_cost?: number;
  tool_name?: string;
  retry_count?: number;
  error_type?: string;
  error_message?: string;
  attributes?: Record<string, unknown>;
  events?: SpanEvent[];
}

export interface TraceSummary {
  trace_id: string;
  root_operation?: string;
  root_span_type?: SpanType;
  status: "ok" | "error" | "running";
  start_time_unix_nanos: number;
  end_time_unix_nanos: number;
  duration_ms: number;
  span_count: number;
  error_count: number;
  input_tokens: number;
  output_tokens: number;
  estimated_cost: number;
  providers: string[];
  models: string[];
}

export interface Stats {
  trace_count: number;
  span_count: number;
  traces_with_errors: number;
  model_calls: number;
  tool_calls: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_estimated_cost: number;
  avg_trace_duration_ms: number;
}

export interface RuntimeConfig {
  capture_payloads: boolean;
  retention_days: number;
  max_body_bytes: number;
  bind_addr: string;
  loopback_only: boolean;
  schema_version: string;
}

export interface TraceFilters {
  status?: string;
  span_type?: string;
  provider?: string;
  model?: string;
  search?: string;
  min_duration_ms?: number;
  limit?: number;
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url, { headers: { accept: "application/json" } });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}${text ? `: ${text}` : ""}`);
  }
  return (await res.json()) as T;
}

export function listTraces(filters: TraceFilters = {}): Promise<{ traces: TraceSummary[] }> {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(filters)) {
    if (v !== undefined && v !== "" && v !== null) params.set(k, String(v));
  }
  const qs = params.toString();
  return getJson(`/v1/traces${qs ? `?${qs}` : ""}`);
}

export function getTrace(traceId: string): Promise<{ trace_id: string; spans: Span[] }> {
  return getJson(`/v1/traces/${traceId}`);
}

export function getStats(): Promise<Stats> {
  return getJson("/v1/stats");
}

export function getConfig(): Promise<RuntimeConfig> {
  return getJson("/v1/config");
}

export function exportUrl(traceId: string, format: "native" | "otlp"): string {
  return `/v1/traces/${traceId}/export?format=${format}`;
}
