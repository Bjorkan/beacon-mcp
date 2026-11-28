//! Metric registry + telemetry point rules.
//! Missing = omitted field, never zero. Untrusted excluded unless opted in.
//! Rates divide increments by counterSpanSecs, never wall-clock width.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    PolledRepeater,
    ObserverReception,
    ServerComputation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aggregation {
    Mean,
    Max,
    Increment,
    Last,
    Count,
}

pub struct Metric {
    pub name: &'static str,
    pub unit: &'static str,
    pub source: Source,
    pub aggregation: Aggregation,
    pub range: &'static str,
    pub missing: &'static str,
    pub limitation: &'static str,
}

pub const REGISTRY: &[Metric] = &[
    Metric {
        name: "batteryMv",
        unit: "mV",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: "0..6000",
        missing: "omitted when no reading carried it",
        limitation: "no SOC derivation",
    },
    Metric {
        name: "temperatureC",
        unit: "°C",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: "-40..85",
        missing: "omitted when no reading carried it",
        limitation: "sparse buckets are missing, not zero",
    },
    Metric {
        name: "uptimeSeconds",
        unit: "s",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Max,
        range: ">=0",
        missing: "omitted when absent",
        limitation: "bucket-max, not exact sample time",
    },
    Metric {
        name: "packetsSent",
        unit: "count",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Increment,
        range: ">=0",
        missing: "omitted with counters absent",
        limitation: "rate needs counterSpanSecs",
    },
    Metric {
        name: "packetsReceived",
        unit: "count",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Increment,
        range: ">=0",
        missing: "omitted with counters absent",
        limitation: "rate needs counterSpanSecs",
    },
    Metric {
        name: "receiveErrors",
        unit: "count",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Increment,
        range: ">=0",
        missing: "omitted with counters absent",
        limitation: "rate needs counterSpanSecs",
    },
    Metric {
        name: "counterSpanSecs",
        unit: "s",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: ">0 when counters present",
        missing: "omitted when no increment could be placed in time",
        limitation: "only valid rate denominator",
    },
    Metric {
        name: "airtimeTxSecs",
        unit: "s",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Increment,
        range: ">=0",
        missing: "omitted when absent",
        limitation: "no duty-cycle claim",
    },
    Metric {
        name: "airtimeRxSecs",
        unit: "s",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Increment,
        range: ">=0",
        missing: "omitted when absent",
        limitation: "no duty-cycle claim",
    },
    Metric {
        name: "noiseFloorDb",
        unit: "dB",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Mean,
        range: "-130..0",
        missing: "omitted when absent",
        limitation: "repeater-measured",
    },
    Metric {
        name: "lastRssiDb",
        unit: "dBm",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: "-130..0",
        missing: "omitted when absent",
        limitation: "repeater-measured last RSSI, not observer-received",
    },
    Metric {
        name: "lastSnrDb",
        unit: "dB",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: "-20..15",
        missing: "omitted when absent",
        limitation: "repeater-measured last SNR, not observer-received",
    },
    Metric {
        name: "queueLength",
        unit: "count",
        source: Source::PolledRepeater,
        aggregation: Aggregation::Last,
        range: ">=0",
        missing: "omitted when absent",
        limitation: "point-in-time only",
    },
    Metric {
        name: "readings",
        unit: "count",
        source: Source::ServerComputation,
        aggregation: Aggregation::Count,
        range: ">=0",
        missing: "zero means no trusted readings",
        limitation: "trusted only",
    },
    Metric {
        name: "untrustedReadings",
        unit: "count",
        source: Source::ServerComputation,
        aggregation: Aggregation::Count,
        range: ">=0",
        missing: "zero when none",
        limitation: "excluded unless include=untrusted",
    },
    Metric {
        name: "rssiAvg",
        unit: "dBm",
        source: Source::ObserverReception,
        aggregation: Aggregation::Mean,
        range: "-130..0",
        missing: "omitted when absent",
        limitation: "observer-received distribution",
    },
    Metric {
        name: "snrAvg",
        unit: "dB",
        source: Source::ObserverReception,
        aggregation: Aggregation::Mean,
        range: "-20..15",
        missing: "omitted when absent",
        limitation: "observer-received distribution",
    },
    Metric {
        name: "clockDriftSeconds",
        unit: "s",
        source: Source::ServerComputation,
        aggregation: Aggregation::Last,
        range: "signed, + ahead",
        missing: "omitted when absent",
        limitation: "device-minus-server; threshold gates clockOutOfSync",
    },
    Metric {
        name: "observationCount",
        unit: "count",
        source: Source::ServerComputation,
        aggregation: Aggregation::Count,
        range: ">=0",
        missing: "zero is a real zero here",
        limitation: "server observations, not flood packets",
    },
];

pub fn find_metric(name: &str) -> Option<&'static Metric> {
    REGISTRY.iter().find(|metric| metric.name == name)
}

/// Packet rate from increments; None when span missing/zero.
pub fn packet_rate(sent_delta: u64, span_secs: Option<u64>) -> Option<f64> {
    match span_secs {
        Some(span) if span > 0 => Some(sent_delta as f64 / span as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{find_metric, packet_rate};

    #[test]
    fn registry_covers_signal_metrics_with_repeater_caveat() {
        let rssi = find_metric("lastRssiDb").unwrap();
        assert!(rssi.limitation.contains("repeater-measured"));
        let snr = find_metric("lastSnrDb").unwrap();
        assert!(snr.limitation.contains("repeater-measured"));
        assert!(
            find_metric("batteryMv")
                .unwrap()
                .limitation
                .contains("no SOC")
        );
        assert!(find_metric("counterSpanSecs").is_some());
    }

    #[test]
    fn rates_need_counter_span_never_wall_clock() {
        assert_eq!(packet_rate(120, Some(60)), Some(2.0));
        assert_eq!(packet_rate(120, Some(0)), None);
        assert_eq!(packet_rate(120, None), None);
    }
}
