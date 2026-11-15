//! Redaction: strip secrets and (by default) raw payloads from spans *before*
//! they are exported or persisted.
//!
//! Two layers run together:
//! 1. **Key rules** — any attribute whose key looks sensitive (authorization,
//!    api_key, password, ...) has its value replaced wholesale.
//! 2. **Value rules** — every remaining string value is scanned for secret-shaped
//!    substrings (bearer tokens, `sk-...` keys, AWS keys, ...) which are masked.
//!
//! Raw payload fields (prompts, messages, tool arguments, completions) are
//! dropped unless `capture_payloads` is explicitly enabled. Redaction is applied
//! at the SDK before export, and again at the collector before storage, so a
//! misconfigured agent cannot leak secrets into the database.
//!
//! This module never logs the values it redacts.

use crate::span::SpanData;
use regex::Regex;
use serde_json::Value;

/// Marker written in place of a redacted value.
pub const REDACTED: &str = "[REDACTED]";
/// Marker written in place of a dropped raw payload.
pub const OMITTED: &str = "[omitted: payload capture disabled]";

/// Configurable redaction policy. Use [`RedactionPolicy::strict`] for the safe
/// default, or build a custom one.
#[derive(Clone)]
pub struct RedactionPolicy {
    /// If false (default), keys in `payload_keys` are dropped before storage.
    pub capture_payloads: bool,
    /// Attribute keys (lowercased) whose values are always fully masked.
    pub redacted_keys: Vec<String>,
    /// Attribute keys (lowercased) treated as raw payloads, dropped unless
    /// `capture_payloads` is true.
    pub payload_keys: Vec<String>,
    /// Regexes applied to every string value; matches are masked in place.
    pub value_patterns: Vec<Regex>,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

impl RedactionPolicy {
    /// The recommended default: payloads off, broad secret detection on.
    pub fn strict() -> Self {
        let redacted_keys = [
            "authorization",
            "proxy-authorization",
            "api_key",
            "apikey",
            "x-api-key",
            "api-key",
            "secret",
            "client_secret",
            "password",
            "passwd",
            "token",
            "access_token",
            "refresh_token",
            "id_token",
            "session_token",
            "private_key",
            "secret_key",
            "aws_secret_access_key",
            "cookie",
            "set-cookie",
            "credentials",
            "bearer",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let payload_keys = [
            "prompt",
            "prompts",
            "messages",
            "input",
            "inputs",
            "output",
            "outputs",
            "completion",
            "response",
            "request_body",
            "response_body",
            "arguments",
            "tool_arguments",
            "tool_input",
            "tool_output",
            "content",
            "raw",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let value_patterns = default_value_patterns();

        RedactionPolicy {
            capture_payloads: false,
            redacted_keys,
            payload_keys,
            value_patterns,
        }
    }

    /// A policy that keeps raw payloads but still masks secret-shaped values and
    /// sensitive keys. Opt-in; documented as higher risk.
    pub fn allow_payloads() -> Self {
        RedactionPolicy {
            capture_payloads: true,
            ..Self::strict()
        }
    }

    /// Redact a span in place: typed fields, attributes, events and error text.
    pub fn redact_span(&self, span: &mut SpanData) {
        // Typed free-text field.
        if let Some(msg) = span.error_message.take() {
            span.error_message = Some(self.redact_string(&msg));
        }
        // Attribute bag.
        let attrs = std::mem::take(&mut span.attributes);
        span.attributes = self.redact_map(attrs);
        // Event attributes.
        for ev in &mut span.events {
            let a = std::mem::take(&mut ev.attributes);
            ev.attributes = self.redact_map(a);
        }
    }

    /// Whether an attribute key should have its value fully masked. Matches the
    /// exact sensitive-key list, plus conservative substrings that are almost
    /// never benign (chosen to avoid masking fields like `input_tokens` or
    /// `token_count`).
    fn is_sensitive_key(&self, lower_key: &str) -> bool {
        if self.redacted_keys.iter().any(|rk| lower_key == rk) {
            return true;
        }
        const INDICATORS: &[&str] = &[
            "secret",
            "password",
            "passwd",
            "api_key",
            "apikey",
            "api-key",
            "access_key",
            "private_key",
            "authorization",
            "credential",
            "access_token",
            "refresh_token",
            "auth_token",
        ];
        INDICATORS.iter().any(|ind| lower_key.contains(ind))
    }

    fn is_dropped_payload_key(&self, lower_key: &str) -> bool {
        !self.capture_payloads && self.payload_keys.iter().any(|pk| lower_key == pk)
    }

    fn redact_map(
        &self,
        map: std::collections::BTreeMap<String, Value>,
    ) -> std::collections::BTreeMap<String, Value> {
        let mut out = std::collections::BTreeMap::new();
        for (k, v) in map {
            let lk = k.to_lowercase();
            if self.is_sensitive_key(&lk) {
                out.insert(k, Value::String(REDACTED.to_string()));
            } else if self.is_dropped_payload_key(&lk) {
                out.insert(k, Value::String(OMITTED.to_string()));
            } else {
                out.insert(k, self.redact_value(v));
            }
        }
        out
    }

    fn redact_value(&self, v: Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.redact_string(&s)),
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(|x| self.redact_value(x)).collect())
            }
            Value::Object(obj) => {
                let mut out = serde_json::Map::new();
                for (k, val) in obj {
                    let lk = k.to_lowercase();
                    if self.is_sensitive_key(&lk) {
                        out.insert(k, Value::String(REDACTED.to_string()));
                    } else if self.is_dropped_payload_key(&lk) {
                        out.insert(k, Value::String(OMITTED.to_string()));
                    } else {
                        out.insert(k, self.redact_value(val));
                    }
                }
                Value::Object(out)
            }
            other => other,
        }
    }

    /// Mask any secret-shaped substrings in a string.
    pub fn redact_string(&self, s: &str) -> String {
        let mut out = s.to_string();
        for re in &self.value_patterns {
            out = re.replace_all(&out, REDACTED).into_owned();
        }
        out
    }
}

fn default_value_patterns() -> Vec<Regex> {
    // Conservative, high-signal patterns. Kept narrow to avoid masking real data.
    let raw = [
        r"(?i)bearer\s+[A-Za-z0-9._\-]{8,}", // Authorization: Bearer ...
        r"sk-ant-[A-Za-z0-9\-_]{16,}",       // Anthropic keys
        r"sk-[A-Za-z0-9_\-]{16,}",           // OpenAI keys incl. sk-proj-/sk-svcacct-
        r"AKIA[0-9A-Z]{16}",                 // AWS access key id
        r"AIza[0-9A-Za-z\-_]{20,}",          // Google API key
        r"gh[pousr]_[A-Za-z0-9]{20,}",       // GitHub tokens
        r"xox[baprs]-[A-Za-z0-9\-]{10,}",    // Slack tokens
        r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}", // JWT
    ];
    raw.iter()
        .map(|p| Regex::new(p).expect("builtin redaction regex must compile"))
        .collect()
}

/// Convenience: assert that no known secret markers appear in a serialized span.
/// Used in tests and as a last-resort guard.
pub fn contains_obvious_secret(json: &str) -> bool {
    let policy = RedactionPolicy::strict();
    policy.value_patterns.iter().any(|re| re.is_match(json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{SpanId, TraceId};
    use crate::span::{SpanData, SpanType};
    use serde_json::json;

    fn span_with_attrs(attrs: serde_json::Value) -> SpanData {
        let mut s = SpanData::new(
            TraceId::generate(),
            SpanId::generate(),
            None,
            SpanType::ModelCall,
            "model.call",
        );
        if let Value::Object(obj) = attrs {
            for (k, v) in obj {
                s.attributes.insert(k, v);
            }
        }
        s
    }

    #[test]
    fn masks_sensitive_keys() {
        let mut s = span_with_attrs(json!({
            "authorization": "Bearer abc123def456ghi789",
            "api_key": "super-secret-value",
            "model": "gpt-4o"
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        assert_eq!(s.attributes["authorization"], json!(REDACTED));
        assert_eq!(s.attributes["api_key"], json!(REDACTED));
        assert_eq!(s.attributes["model"], json!("gpt-4o"));
    }

    #[test]
    fn drops_payloads_by_default() {
        let mut s = span_with_attrs(json!({
            "prompt": "my private user prompt",
            "messages": [{"role": "user", "content": "hello"}]
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        assert_eq!(s.attributes["prompt"], json!(OMITTED));
        assert_eq!(s.attributes["messages"], json!(OMITTED));
    }

    #[test]
    fn keeps_payloads_when_opted_in_but_still_masks_secrets() {
        let mut s = span_with_attrs(json!({
            "prompt": "here is my key sk-abcdefghijklmnopqrstuvwxyz123456 ok"
        }));
        RedactionPolicy::allow_payloads().redact_span(&mut s);
        let p = s.attributes["prompt"].as_str().unwrap();
        assert!(p.contains("here is my key"));
        assert!(p.contains(REDACTED));
        assert!(!p.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn masks_nested_values_and_secret_patterns() {
        let mut s = span_with_attrs(json!({
            "headers": { "Authorization": "Bearer sk-ant-api03-xxxxxxxxxxxxxxxxxxxx" },
            "note": "token AKIA1234567890ABCDEF leaked"
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !contains_obvious_secret(&json),
            "serialized span still has a secret: {json}"
        );
        assert!(!json.contains("AKIA1234567890ABCDEF"));
    }

    #[test]
    fn masks_hyphenated_openai_project_keys() {
        // Regression: `sk-proj-...` (and the unflagged key name `openai_key`)
        // previously slipped through because the regex stopped at the hyphen.
        let mut s = span_with_attrs(json!({
            "openai_key": "sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
            "svc": "sk-svcacct-ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("sk-proj-ABCDEFGHIJKL"),
            "sk-proj key leaked: {json}"
        );
        assert!(
            !json.contains("sk-svcacct-ABCDEFGHIJKL"),
            "sk-svcacct key leaked: {json}"
        );
    }

    #[test]
    fn masks_sensitive_key_substrings() {
        let mut s = span_with_attrs(json!({
            "db_password": "hunter2",
            "openai_api_key": "whatever",
            "x_auth_token": "zzz"
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        assert_eq!(s.attributes["db_password"], json!(REDACTED));
        assert_eq!(s.attributes["openai_api_key"], json!(REDACTED));
        assert_eq!(s.attributes["x_auth_token"], json!(REDACTED));
    }

    #[test]
    fn does_not_over_redact_token_counts() {
        // Benign fields that contain "token" must survive (not caught by "token").
        let mut s = span_with_attrs(json!({
            "input_tokens": 1200,
            "output_tokens": 340,
            "token_count": 55,
            "idempotency_key": "req-123"
        }));
        RedactionPolicy::strict().redact_span(&mut s);
        assert_eq!(s.attributes["input_tokens"], json!(1200));
        assert_eq!(s.attributes["output_tokens"], json!(340));
        assert_eq!(s.attributes["token_count"], json!(55));
        assert_eq!(s.attributes["idempotency_key"], json!("req-123"));
    }

    #[test]
    fn redacts_error_message() {
        let mut s = SpanData::new(
            TraceId::generate(),
            SpanId::generate(),
            None,
            SpanType::Error,
            "error",
        );
        s.error_message = Some("failed with token AKIA1234567890ABCDEF".into());
        RedactionPolicy::strict().redact_span(&mut s);
        assert!(!s
            .error_message
            .as_ref()
            .unwrap()
            .contains("AKIA1234567890ABCDEF"));
    }
}
