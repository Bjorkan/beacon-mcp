//! Time and coverage: bucket starts, UTC ISO + epoch-ms, retention, gaps.
//! Rates use counterSpanSecs; duty-cycle claims are forbidden.

use chrono::DateTime;

pub fn to_utc_iso(millis: i64) -> String {
    DateTime::from_timestamp_millis(millis)
        .map(|time| time.to_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

pub fn bucket_label(bucket_start_ms: i64) -> String {
    format!("bucket_start={}", to_utc_iso(bucket_start_ms))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    HalfOpen,
    InclusiveUntil,
    SinceOnly,
}

pub fn window_contains(kind: WindowKind, since: i64, until: Option<i64>, point: i64) -> bool {
    match kind {
        WindowKind::HalfOpen => {
            let end = until.unwrap_or(i64::MAX);
            point >= since && point < end
        }
        WindowKind::InclusiveUntil => {
            let end = until.unwrap_or(i64::MAX);
            point >= since && point <= end
        }
        WindowKind::SinceOnly => point >= since,
    }
}

pub fn retention_allows(operation: &str, range_hours: u64) -> bool {
    match operation {
        "node.activity" => (1..=720).contains(&range_hours),
        "node.telemetry" => matches!(range_hours, 24 | 72 | 168 | 720),
        "route.observations" => range_hours <= 24 * 30,
        "stats.series" => range_hours <= 24 * 90,
        _ => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketPresence {
    Present,
    SparseMissing,
}

pub fn bucket_presence(has_reading: bool) -> BucketPresence {
    if has_reading {
        BucketPresence::Present
    } else {
        BucketPresence::SparseMissing
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BucketPresence, WindowKind, bucket_label, bucket_presence, retention_allows, to_utc_iso,
        window_contains,
    };

    #[test]
    fn iso_and_bucket_labels() {
        assert_eq!(to_utc_iso(0), "1970-01-01T00:00:00Z");
        assert!(bucket_label(0).starts_with("bucket_start="));
    }

    #[test]
    fn window_kinds_and_retention() {
        assert!(window_contains(WindowKind::HalfOpen, 10, Some(20), 10));
        assert!(!window_contains(WindowKind::HalfOpen, 10, Some(20), 20));
        assert!(window_contains(
            WindowKind::InclusiveUntil,
            10,
            Some(20),
            20
        ));
        assert!(window_contains(WindowKind::SinceOnly, 10, None, 999));
        assert!(retention_allows("node.activity", 24));
        assert!(!retention_allows("node.activity", 721));
        assert!(!retention_allows("stats.series", 24 * 91));
    }

    #[test]
    fn sparse_is_missing_not_zero() {
        assert_eq!(bucket_presence(false), BucketPresence::SparseMissing);
        assert_eq!(bucket_presence(true), BucketPresence::Present);
    }
}
