# Trace schema (v0.1.0)

A span is one unit of work in an agent run. Spans form a tree through
`parent_span_id`, and every span in one run shares a `trace_id`. The schema is
versioned by `schema_version`, which I bump on any breaking change.

## Span types (`span_type`)

| Value         | Operation name | Meaning                              |
| ------------- | -------------- | ------------------------------------ |
| `agent_run`   | `agent.run`    | Top-level run                        |
| `model_call`  | `model.call`   | LLM / model provider request         |
| `tool_call`   | `tool.call`    | Tool / function invocation           |
| `approval`    | `approval`     | Human-or-policy approval gate        |
| `retry`       | `retry`        | Retry of a previous operation        |
| `error`       | `error`        | Standalone error event span          |

## Fields

| Field                    | Type            | Required | Sensitive | Notes |
| ------------------------ | --------------- | -------- | --------- | ----- |
| `schema_version`         | string          | yes      | no        | e.g. `0.1.0` |
| `trace_id`               | string (32 hex) | yes      | no        | 128-bit, OTel-style |
| `span_id`                | string (16 hex) | yes      | no        | 64-bit |
| `parent_span_id`         | string (16 hex) | no       | no        | omitted for root |
| `span_type`              | enum            | yes      | no        | see table above |
| `operation_name`         | string          | yes      | no        | human label |
| `start_time_unix_nanos`  | uint64          | yes      | no        | UTC epoch ns |
| `end_time_unix_nanos`    | uint64          | no       | no        | null while running/interrupted |
| `status`                 | enum            | yes      | no        | `unset` \| `ok` \| `error` |
| `provider`               | string          | no       | no        | model provider |
| `model`                  | string          | no       | no        | model id |
| `input_tokens`           | uint64          | no       | no        | |
| `output_tokens`          | uint64          | no       | no        | |
| `estimated_cost`         | float           | no       | no        | USD estimate |
| `tool_name`              | string          | no       | no        | |
| `retry_count`            | uint32          | no       | no        | |
| `error_type`             | string          | no       | no        | e.g. `TimeoutError` |
| `error_message`          | string          | no       | **masked**| secrets redacted |
| `attributes`             | map<string,any> | no       | **policy**| free-form; redacted |
| `events`                 | array           | no       | **policy**| point-in-time events |

`duration_ms` is derived (`end - start`). Consumers compute it, and the collector
also materializes it as a column so it can be queried.

### Sensitivity

Some fields never get stored in the clear. Authorization headers, API keys, tokens
and passwords (any attribute whose key matches the sensitive-key list, and any
value matching a secret pattern: bearer, `sk-`, AWS, GCP, GitHub, Slack, JWT) are
masked to `[REDACTED]`.

Others are off by default. The raw payload keys (`prompt`, `messages`, `input`,
`output`, `arguments`, `content`, and so on) are dropped to
`[omitted: payload capture disabled]` unless payload capture is explicitly turned
on.

See [`privacy.md`](privacy.md) for the full model.

## JSON example

```json
{
  "schema_version": "0.1.0",
  "trace_id": "302d53184a93cac326b8632a9252689a",
  "span_id": "e8219fa82a46de2d",
  "parent_span_id": "ddc73930913fa72e",
  "span_type": "tool_call",
  "operation_name": "fetch_page",
  "start_time_unix_nanos": 1794710314700000000,
  "end_time_unix_nanos": 1794710314756000000,
  "status": "error",
  "tool_name": "fetch_page",
  "error_type": "TimeoutError",
  "error_message": "GET timed out after 5000ms",
  "attributes": { "url": "https://arxiv.org/abs/2601.01234" }
}
```

## OTLP mapping

`GET /v1/traces/:id/export?format=otlp` returns an OTLP/JSON
`ExportTraceServiceRequest`. The agent concepts map onto attributes like this:

| AgentTrace field | OTLP attribute |
| ---------------- | -------------- |
| `provider`       | `gen_ai.system` |
| `model`          | `gen_ai.request.model` |
| `input_tokens`   | `gen_ai.usage.input_tokens` |
| `output_tokens`  | `gen_ai.usage.output_tokens` |
| `estimated_cost` | `agenttrace.estimated_cost` |
| `tool_name`      | `agenttrace.tool.name` |
| `retry_count`    | `agenttrace.retry_count` |
| `span_type`      | `agenttrace.span_type` |
| `error_type`     | `exception.type` |
| `status`         | OTLP status code (`unset`/`ok`/`error` → 0/1/2) |

Full OTLP ingestion is still a roadmap item. Only export is implemented today.
