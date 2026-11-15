//! Time helpers. All timestamps are UTC Unix epoch nanoseconds, matching the
//! OpenTelemetry wire format. Storing an absolute UTC instant (rather than a
//! local or monotonic value) keeps traces comparable across machines.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time as nanoseconds since the Unix epoch (UTC).
pub fn now_unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Duration in fractional milliseconds between two epoch-nanosecond timestamps.
/// Returns 0 if `end` precedes `start` (clock skew / unfinished span).
pub fn duration_ms(start_nanos: u64, end_nanos: u64) -> f64 {
    if end_nanos <= start_nanos {
        return 0.0;
    }
    (end_nanos - start_nanos) as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_is_non_negative() {
        assert_eq!(duration_ms(100, 50), 0.0);
        assert_eq!(duration_ms(0, 1_000_000), 1.0);
        assert_eq!(duration_ms(0, 2_500_000), 2.5);
    }

    #[test]
    fn now_is_after_2020() {
        // 2020-01-01 in epoch nanos.
        assert!(now_unix_nanos() > 1_577_836_800_000_000_000);
    }
}
