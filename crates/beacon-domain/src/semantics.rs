//! Cross-cutting semantic guards: firmware floors, drift, status, counts.

/// minFirmwareVersion is a floor: exact >= floor satisfies, never equality.
pub fn firmware_satisfies(exact: &str, floor: &str) -> bool {
    exact >= floor
}

/// Signed device-minus-server drift: + ahead, - behind.
pub fn clock_drift(device_ms: i64, server_ms: i64) -> i64 {
    device_ms - server_ms
}

pub fn clock_out_of_sync(drift_secs: f64, threshold_secs: f64) -> bool {
    drift_secs.abs() > threshold_secs
}

/// Observer status as-reported; staleness threshold unknown => no liveness verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportedStatus {
    Online,
    Offline,
}

pub fn status_verdict(status: ReportedStatus, _last_seen_ms: i64) -> &'static str {
    match status {
        ReportedStatus::Online => {
            "as-reported online; staleness threshold unknown, not a liveness proof"
        }
        ReportedStatus::Offline => "as-reported offline; not a permanent verdict",
    }
}

/// Packet counts (flood packets) vs observation counts (receptions) stay distinct.
pub struct CountPair {
    pub packets: u64,
    pub observations: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        CountPair, ReportedStatus, clock_drift, clock_out_of_sync, firmware_satisfies,
        status_verdict,
    };

    #[test]
    fn firmware_floor_and_signed_drift() {
        assert!(firmware_satisfies("1.2.3", "1.2.0"));
        assert!(!firmware_satisfies("1.1.9", "1.2.0"));
        assert_eq!(clock_drift(1000, 900), 100);
        assert_eq!(clock_drift(900, 1000), -100);
        assert!(clock_out_of_sync(120.0, 60.0));
        assert!(!clock_out_of_sync(30.0, 60.0));
    }

    #[test]
    fn status_is_reported_not_liveness_and_counts_distinct() {
        assert!(status_verdict(ReportedStatus::Online, 0).contains("not a liveness proof"));
        let counts = CountPair {
            packets: 5,
            observations: 9,
        };
        assert_ne!(counts.packets, counts.observations);
    }
}
