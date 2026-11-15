//! SQLite storage for spans.
//!
//! This is the persistence boundary. Every span is **validated and redacted here**
//! before it is written, so even a misconfigured or malicious client cannot land
//! raw secrets in the database. Spans are stored individually (supporting
//! incomplete/interrupted traces) and indexed by trace, time and type.
//!
//! Re-sent spans (same `span_id`) upsert, which makes ingestion idempotent and
//! tolerant of duplicates and out-of-order delivery.

use agenttrace::span::SpanData;
use agenttrace::RedactionPolicy;
use anyhow::{Context, Result};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};
use serde::Serialize;
use std::sync::{Arc, Mutex};

const CURRENT_SCHEMA_VERSION: i64 = 1;

/// Handle to the span database. Cloneable; all clones share one connection.
#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
    policy: RedactionPolicy,
}

/// Filters for listing traces.
#[derive(Debug, Default, Clone)]
pub struct TraceFilter {
    pub status: Option<String>,    // "ok" | "error" | "running"
    pub span_type: Option<String>, // trace contains a span of this type
    pub provider: Option<String>,
    pub model: Option<String>,
    pub search: Option<String>, // matches operation name, tool name or trace id
    pub time_from: Option<i64>, // start >= (epoch nanos)
    pub time_to: Option<i64>,   // start <= (epoch nanos)
    pub min_duration_ms: Option<f64>,
    pub max_duration_ms: Option<f64>,
    pub limit: i64,
    pub offset: i64,
}

/// One-row summary of a trace for the explorer list.
#[derive(Debug, Serialize)]
pub struct TraceSummary {
    pub trace_id: String,
    pub root_operation: Option<String>,
    pub root_span_type: Option<String>,
    pub status: String, // ok | error | running
    pub start_time_unix_nanos: i64,
    pub end_time_unix_nanos: i64,
    pub duration_ms: f64,
    pub span_count: i64,
    pub error_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub estimated_cost: f64,
    pub providers: Vec<String>,
    pub models: Vec<String>,
}

/// Overview metrics for the dashboard.
#[derive(Debug, Serialize, Default)]
pub struct Stats {
    pub trace_count: i64,
    pub span_count: i64,
    pub traces_with_errors: i64,
    pub model_calls: i64,
    pub tool_calls: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_estimated_cost: f64,
    pub avg_trace_duration_ms: f64,
}

impl Storage {
    /// Open (or create) the database at `path` and run migrations.
    pub fn open(path: &std::path::Path, policy: RedactionPolicy) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening database at {}", path.display()))?;
        Self::init(conn, policy)
    }

    /// In-memory database (tests).
    pub fn open_in_memory(policy: RedactionPolicy) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, policy)
    }

    fn init(conn: Connection, policy: RedactionPolicy) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let storage = Storage {
            conn: Arc::new(Mutex::new(conn)),
            policy,
        };
        storage.migrate()?;
        Ok(storage)
    }

    /// Apply schema migrations based on `PRAGMA user_version`.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS spans (
                    span_id                 TEXT PRIMARY KEY,
                    trace_id                TEXT NOT NULL,
                    parent_span_id          TEXT,
                    span_type               TEXT NOT NULL,
                    operation_name          TEXT NOT NULL,
                    start_time_unix_nanos   INTEGER NOT NULL,
                    end_time_unix_nanos     INTEGER,
                    duration_ms             REAL,
                    status                  TEXT NOT NULL,
                    provider                TEXT,
                    model                   TEXT,
                    input_tokens            INTEGER,
                    output_tokens           INTEGER,
                    estimated_cost          REAL,
                    tool_name               TEXT,
                    retry_count             INTEGER,
                    error_type              TEXT,
                    error_message           TEXT,
                    schema_version          TEXT NOT NULL,
                    data                    TEXT NOT NULL,
                    ingested_at_unix_nanos  INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_spans_trace ON spans(trace_id);
                CREATE INDEX IF NOT EXISTS idx_spans_start ON spans(start_time_unix_nanos);
                CREATE INDEX IF NOT EXISTS idx_spans_type  ON spans(span_type);
                CREATE INDEX IF NOT EXISTS idx_spans_provider ON spans(provider);
                "#,
            )?;
            conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
        }
        Ok(())
    }

    /// Validate, redact and persist one span. Returns an error (for the caller to
    /// count/report) if the span is structurally invalid.
    pub fn store_span(&self, mut span: SpanData) -> Result<()> {
        span.validate().map_err(|e| anyhow::anyhow!(e))?;
        // Enforce redaction at the persistence boundary.
        self.policy.redact_span(&mut span);

        let duration = span.duration_ms();
        let data = serde_json::to_string(&span)?;
        let now = agenttrace::time::now_unix_nanos() as i64;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT OR REPLACE INTO spans (
                span_id, trace_id, parent_span_id, span_type, operation_name,
                start_time_unix_nanos, end_time_unix_nanos, duration_ms, status,
                provider, model, input_tokens, output_tokens, estimated_cost,
                tool_name, retry_count, error_type, error_message,
                schema_version, data, ingested_at_unix_nanos
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21
            )"#,
            params![
                span.span_id.to_hex(),
                span.trace_id.to_hex(),
                span.parent_span_id.map(|p| p.to_hex()),
                serde_json::to_value(span.span_type)?
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                span.operation_name,
                span.start_time_unix_nanos as i64,
                span.end_time_unix_nanos.map(|v| v as i64),
                duration,
                serde_json::to_value(span.status)?
                    .as_str()
                    .unwrap_or("unset")
                    .to_string(),
                span.provider,
                span.model,
                span.input_tokens.map(|v| v as i64),
                span.output_tokens.map(|v| v as i64),
                span.estimated_cost,
                span.tool_name,
                span.retry_count.map(|v| v as i64),
                span.error_type,
                span.error_message,
                span.schema_version,
                data,
                now,
            ],
        )?;
        Ok(())
    }

    /// All spans of one trace, ordered by start time (for detail + replay).
    pub fn get_trace_spans(&self, trace_id: &str) -> Result<Vec<SpanData>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT data FROM spans WHERE trace_id = ?1 ORDER BY start_time_unix_nanos ASC",
        )?;
        let rows = stmt.query_map(params![trace_id], |r| r.get::<_, String>(0))?;
        let mut spans = Vec::new();
        for row in rows {
            let json = row?;
            if let Ok(span) = serde_json::from_str::<SpanData>(&json) {
                spans.push(span);
            }
        }
        Ok(spans)
    }

    /// List trace summaries matching `filter`.
    pub fn list_traces(&self, filter: &TraceFilter) -> Result<Vec<TraceSummary>> {
        let mut where_clauses: Vec<String> = Vec::new();
        let mut having_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<SqlValue> = Vec::new();

        // Per-span membership filters constrain which traces qualify.
        if let Some(p) = &filter.provider {
            where_clauses
                .push("trace_id IN (SELECT trace_id FROM spans WHERE provider = ?)".into());
            binds.push(SqlValue::Text(p.clone()));
        }
        if let Some(m) = &filter.model {
            where_clauses.push("trace_id IN (SELECT trace_id FROM spans WHERE model = ?)".into());
            binds.push(SqlValue::Text(m.clone()));
        }
        if let Some(t) = &filter.span_type {
            where_clauses
                .push("trace_id IN (SELECT trace_id FROM spans WHERE span_type = ?)".into());
            binds.push(SqlValue::Text(t.clone()));
        }
        if let Some(s) = &filter.search {
            where_clauses.push(
                "(trace_id LIKE ? OR trace_id IN (SELECT trace_id FROM spans WHERE operation_name LIKE ? OR tool_name LIKE ?))"
                    .into(),
            );
            let like = format!("%{s}%");
            binds.push(SqlValue::Text(like.clone()));
            binds.push(SqlValue::Text(like.clone()));
            binds.push(SqlValue::Text(like));
        }

        // Aggregate-level filters.
        if let Some(from) = filter.time_from {
            having_clauses.push("t_start >= ?".into());
            binds.push(SqlValue::Integer(from));
        }
        if let Some(to) = filter.time_to {
            having_clauses.push("t_start <= ?".into());
            binds.push(SqlValue::Integer(to));
        }
        if let Some(min) = filter.min_duration_ms {
            having_clauses.push("((t_end - t_start) / 1000000.0) >= ?".into());
            binds.push(SqlValue::Real(min));
        }
        if let Some(max) = filter.max_duration_ms {
            having_clauses.push("((t_end - t_start) / 1000000.0) <= ?".into());
            binds.push(SqlValue::Real(max));
        }
        match filter.status.as_deref() {
            Some("error") => having_clauses.push("error_count > 0".into()),
            Some("ok") => having_clauses.push("error_count = 0 AND open_count = 0".into()),
            Some("running") => having_clauses.push("open_count > 0".into()),
            _ => {}
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };
        let having_sql = if having_clauses.is_empty() {
            String::new()
        } else {
            format!("HAVING {}", having_clauses.join(" AND "))
        };

        let limit = if filter.limit <= 0 { 50 } else { filter.limit };
        let sql = format!(
            r#"
            SELECT trace_id,
                MIN(start_time_unix_nanos) AS t_start,
                MAX(COALESCE(end_time_unix_nanos, start_time_unix_nanos)) AS t_end,
                COUNT(*) AS span_count,
                SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS error_count,
                SUM(CASE WHEN end_time_unix_nanos IS NULL THEN 1 ELSE 0 END) AS open_count,
                SUM(COALESCE(input_tokens, 0)) AS in_tok,
                SUM(COALESCE(output_tokens, 0)) AS out_tok,
                SUM(COALESCE(estimated_cost, 0)) AS cost,
                (SELECT operation_name FROM spans r WHERE r.trace_id = spans.trace_id AND r.parent_span_id IS NULL ORDER BY start_time_unix_nanos LIMIT 1) AS root_op,
                (SELECT span_type FROM spans r WHERE r.trace_id = spans.trace_id AND r.parent_span_id IS NULL ORDER BY start_time_unix_nanos LIMIT 1) AS root_type,
                group_concat(DISTINCT provider) AS providers,
                group_concat(DISTINCT model) AS models
            FROM spans
            {where_sql}
            GROUP BY trace_id
            {having_sql}
            ORDER BY t_start DESC
            LIMIT ? OFFSET ?
            "#
        );
        binds.push(SqlValue::Integer(limit));
        binds.push(SqlValue::Integer(filter.offset.max(0)));

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(binds.iter()), |r| {
            let t_start: i64 = r.get(1)?;
            let t_end: i64 = r.get(2)?;
            let error_count: i64 = r.get(4)?;
            let open_count: i64 = r.get(5)?;
            let status = if error_count > 0 {
                "error"
            } else if open_count > 0 {
                "running"
            } else {
                "ok"
            };
            let providers: Option<String> = r.get(11)?;
            let models: Option<String> = r.get(12)?;
            Ok(TraceSummary {
                trace_id: r.get(0)?,
                start_time_unix_nanos: t_start,
                end_time_unix_nanos: t_end,
                duration_ms: (t_end - t_start) as f64 / 1_000_000.0,
                span_count: r.get(3)?,
                error_count,
                input_tokens: r.get(6)?,
                output_tokens: r.get(7)?,
                estimated_cost: r.get(8)?,
                root_operation: r.get(9)?,
                root_span_type: r.get(10)?,
                status: status.to_string(),
                providers: split_concat(providers),
                models: split_concat(models),
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Overview metrics.
    pub fn stats(&self) -> Result<Stats> {
        let conn = self.conn.lock().unwrap();
        let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
        Ok(Stats {
            span_count: count("SELECT COUNT(*) FROM spans")?,
            trace_count: count("SELECT COUNT(DISTINCT trace_id) FROM spans")?,
            traces_with_errors: count(
                "SELECT COUNT(DISTINCT trace_id) FROM spans WHERE status = 'error'",
            )?,
            model_calls: count("SELECT COUNT(*) FROM spans WHERE span_type = 'model_call'")?,
            tool_calls: count("SELECT COUNT(*) FROM spans WHERE span_type = 'tool_call'")?,
            total_input_tokens: count("SELECT COALESCE(SUM(input_tokens),0) FROM spans")?,
            total_output_tokens: count("SELECT COALESCE(SUM(output_tokens),0) FROM spans")?,
            total_estimated_cost: conn
                .query_row("SELECT COALESCE(SUM(estimated_cost),0) FROM spans", [], |r| {
                    r.get(0)
                })?,
            avg_trace_duration_ms: conn
                .query_row(
                    r#"SELECT COALESCE(AVG(dur),0) FROM (
                      SELECT (MAX(COALESCE(end_time_unix_nanos, start_time_unix_nanos)) - MIN(start_time_unix_nanos)) / 1000000.0 AS dur
                      FROM spans GROUP BY trace_id
                   )"#,
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0.0),
        })
    }

    /// Delete spans whose start time is older than `retention_days`. Returns the
    /// number of spans removed. A value of 0 disables pruning.
    pub fn prune(&self, retention_days: u64) -> Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = agenttrace::time::now_unix_nanos() as i64
            - (retention_days as i64) * 86_400 * 1_000_000_000;
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "DELETE FROM spans WHERE start_time_unix_nanos < ?1",
            params![cutoff],
        )?;
        Ok(n)
    }

    pub fn trace_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(
            conn.query_row("SELECT COUNT(DISTINCT trace_id) FROM spans", [], |r| {
                r.get(0)
            })?,
        )
    }
}

fn split_concat(v: Option<String>) -> Vec<String> {
    v.map(|s| {
        s.split(',')
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agenttrace::id::{SpanId, TraceId};
    use agenttrace::span::{SpanStatus, SpanType};

    fn store() -> Storage {
        Storage::open_in_memory(RedactionPolicy::strict()).unwrap()
    }

    fn span(trace: TraceId, parent: Option<SpanId>, st: SpanType, name: &str) -> SpanData {
        let mut s = SpanData::new(trace, SpanId::generate(), parent, st, name);
        s.end_time_unix_nanos = Some(s.start_time_unix_nanos + 2_000_000);
        s.status = SpanStatus::Ok;
        s
    }

    #[test]
    fn stores_and_reads_back_a_trace() {
        let st = store();
        let t = TraceId::generate();
        let root = span(t, None, SpanType::AgentRun, "run");
        let root_id = root.span_id;
        st.store_span(root).unwrap();
        st.store_span(span(t, Some(root_id), SpanType::ModelCall, "chat"))
            .unwrap();
        let spans = st.get_trace_spans(&t.to_hex()).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(st.trace_count().unwrap(), 1);
    }

    #[test]
    fn redacts_at_persistence_boundary() {
        let st = store();
        let t = TraceId::generate();
        let mut s = span(t, None, SpanType::ModelCall, "chat");
        s.attributes
            .insert("api_key".into(), serde_json::json!("secret123"));
        st.store_span(s).unwrap();
        let back = &st.get_trace_spans(&t.to_hex()).unwrap()[0];
        assert_eq!(back.attributes["api_key"], serde_json::json!("[REDACTED]"));
    }

    #[test]
    fn rejects_invalid_span() {
        let st = store();
        let mut s = span(TraceId::generate(), None, SpanType::AgentRun, "run");
        s.operation_name = "".into();
        assert!(st.store_span(s).is_err());
    }

    #[test]
    fn upsert_is_idempotent() {
        let st = store();
        let t = TraceId::generate();
        let s = span(t, None, SpanType::AgentRun, "run");
        let s2 = s.clone();
        st.store_span(s).unwrap();
        st.store_span(s2).unwrap();
        assert_eq!(st.get_trace_spans(&t.to_hex()).unwrap().len(), 1);
    }

    #[test]
    fn filters_by_status_and_provider() {
        let st = store();
        // ok trace with anthropic model
        let t1 = TraceId::generate();
        let root1 = span(t1, None, SpanType::AgentRun, "ok-run");
        let r1 = root1.span_id;
        st.store_span(root1).unwrap();
        let mut m = span(t1, Some(r1), SpanType::ModelCall, "chat");
        m.provider = Some("anthropic".into());
        st.store_span(m).unwrap();

        // error trace
        let t2 = TraceId::generate();
        let root2 = span(t2, None, SpanType::AgentRun, "bad-run");
        let r2 = root2.span_id;
        st.store_span(root2).unwrap();
        let mut e = span(t2, Some(r2), SpanType::ToolCall, "search");
        e.status = SpanStatus::Error;
        st.store_span(e).unwrap();

        let all = st
            .list_traces(&TraceFilter {
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), 2);

        let errs = st
            .list_traces(&TraceFilter {
                status: Some("error".into()),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].status, "error");

        let anth = st
            .list_traces(&TraceFilter {
                provider: Some("anthropic".into()),
                limit: 50,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(anth.len(), 1);
        assert!(anth[0].providers.contains(&"anthropic".to_string()));
    }

    #[test]
    fn stats_counts_calls() {
        let st = store();
        let t = TraceId::generate();
        let root = span(t, None, SpanType::AgentRun, "run");
        let rid = root.span_id;
        st.store_span(root).unwrap();
        st.store_span(span(t, Some(rid), SpanType::ModelCall, "chat"))
            .unwrap();
        st.store_span(span(t, Some(rid), SpanType::ToolCall, "search"))
            .unwrap();
        let s = st.stats().unwrap();
        assert_eq!(s.trace_count, 1);
        assert_eq!(s.model_calls, 1);
        assert_eq!(s.tool_calls, 1);
        assert_eq!(s.span_count, 3);
    }
}
