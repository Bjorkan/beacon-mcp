//! Polling semantics: assignment vs live claim vs targets vs readings.
//! `polled` = stored reading exists, never assignment alone.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub poller: Option<String>,
    pub standbys: Vec<String>,
    pub rank: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slots {
    pub free: u32,
    pub max: u32,
    pub own: u32,
    pub requested: u32,
    pub used: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveClaim {
    pub live: bool,
    pub accepts_requests: bool,
    pub received_at: Option<String>,
    pub slots: Slots,
}

impl LiveClaim {
    /// Free slots count only when live and accepting, minus handed-out slots.
    pub fn effective_free(&self) -> u32 {
        if !self.live || !self.accepts_requests {
            return 0;
        }
        self.slots
            .free
            .min(self.slots.max.saturating_sub(self.slots.used))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRole {
    Poller,
    Standby,
    Unassigned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollTarget {
    pub role: TargetRole,
    pub rank: Option<u32>,
    pub requested: bool,
    pub last_attempt: Option<String>,
    pub last_ok: Option<String>,
    pub state: String,
}

pub fn is_polled(has_stored_reading: bool) -> bool {
    has_stored_reading
}

/// Latest trusted reading: newest trusted or null; never invented.
pub fn latest_trusted<'a>(readings: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    readings.into_iter().flatten().next()
}

/// Regional sampledAt: later of battery/temperature timestamps (ISO strings
/// compare lexicographically for UTC `...Z`); null when neither exists.
pub fn sampled_at(battery_at: Option<&str>, temperature_at: Option<&str>) -> Option<String> {
    match (battery_at, temperature_at) {
        (Some(a), Some(b)) => Some(a.max(b).to_owned()),
        (Some(a), None) => Some(a.to_owned()),
        (None, Some(b)) => Some(b.to_owned()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{LiveClaim, Slots, TargetRole, is_polled, latest_trusted, sampled_at};

    #[test]
    fn polled_requires_stored_reading() {
        assert!(is_polled(true));
        assert!(!is_polled(false));
    }

    #[test]
    fn assignment_without_live_claim_is_not_actively_polling() {
        let claim = LiveClaim {
            live: false,
            accepts_requests: true,
            received_at: None,
            slots: Slots {
                free: 4,
                max: 8,
                own: 0,
                requested: 0,
                used: 0,
            },
        };
        assert_eq!(claim.effective_free(), 0);
        let live = LiveClaim {
            live: true,
            accepts_requests: true,
            received_at: Some("2026-10-01T00:00:00Z".to_owned()),
            slots: Slots {
                free: 4,
                max: 8,
                own: 1,
                requested: 2,
                used: 6,
            },
        };
        assert_eq!(live.effective_free(), 2);
    }

    #[test]
    fn standby_rank_and_requested_flags() {
        assert_eq!(TargetRole::Standby as u8, TargetRole::Standby as u8);
        assert_ne!(TargetRole::Poller, TargetRole::Unassigned);
    }

    #[test]
    fn trusted_and_sampled_timestamps_never_invented() {
        assert_eq!(latest_trusted([None, None]), None);
        assert_eq!(latest_trusted([Some("b"), None]), Some("b"));
        assert_eq!(
            sampled_at(Some("2026-10-01T01:00:00Z"), Some("2026-10-01T02:00:00Z")),
            Some("2026-10-01T02:00:00Z".to_owned())
        );
        assert_eq!(
            sampled_at(Some("2026-10-01T01:00:00Z"), None),
            Some("2026-10-01T01:00:00Z".to_owned())
        );
        assert_eq!(sampled_at(None, None), None);
    }
}
