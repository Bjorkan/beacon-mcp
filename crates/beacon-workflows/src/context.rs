//! Request-scoped budgets: attempts, concurrency, deadlines.
//! Provisional tiers summary/standard/full: 6/12/20 attempts, 3/4/4 concurrent,
//! 12/20/30s deadlines. Retries, resolution, pagination and fan-out all count.
//! Tuning documented here; empirical verification deferred to load gates.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Summary,
    Standard,
    Full,
}

impl Tier {
    pub fn max_attempts(self) -> usize {
        match self {
            Self::Summary => 6,
            Self::Standard => 12,
            Self::Full => 20,
        }
    }

    pub fn max_concurrent(self) -> usize {
        match self {
            Self::Summary => 3,
            Self::Standard => 4,
            Self::Full => 4,
        }
    }

    pub fn deadline(self) -> Duration {
        match self {
            Self::Summary => Duration::from_secs(12),
            Self::Standard => Duration::from_secs(20),
            Self::Full => Duration::from_secs(30),
        }
    }
}

fn global_cap() -> &'static Arc<Semaphore> {
    static CAP: OnceLock<Arc<Semaphore>> = OnceLock::new();
    CAP.get_or_init(|| Arc::new(Semaphore::new(16)))
}

#[derive(Debug)]
pub struct BudgetExceeded {
    pub message: String,
}

#[derive(Debug)]
pub struct RequestContext {
    pub tier: Tier,
    pub start: Instant,
    pub deadline: Instant,
    pub cancel: CancellationToken,
    pub attempts: AtomicUsize,
    pub concurrent: Arc<Semaphore>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub server_version: String,
    pub activity_v22: bool,
    pub polling_v23: bool,
}

impl RequestContext {
    pub fn new(tier: Tier, cancel: CancellationToken, capabilities: Capabilities) -> Self {
        let start = Instant::now();
        let deadline = start + tier.deadline();
        Self {
            tier,
            start,
            deadline,
            cancel,
            attempts: AtomicUsize::new(0),
            concurrent: Arc::new(Semaphore::new(tier.max_concurrent())),
            capabilities,
        }
    }

    pub fn try_attempt(&self) -> Result<(), BudgetExceeded> {
        if self.cancel.is_cancelled() || Instant::now() >= self.deadline {
            return Err(BudgetExceeded {
                message: "deadline exceeded".to_owned(),
            });
        }
        let used = self.attempts.fetch_add(1, Ordering::SeqCst);
        if used >= self.tier.max_attempts() {
            return Err(BudgetExceeded {
                message: "attempt budget exhausted".to_owned(),
            });
        }
        Ok(())
    }

    pub fn attempts_used(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

pub async fn global_permit() -> Option<tokio::sync::OwnedSemaphorePermit> {
    global_cap().clone().acquire_owned().await.ok()
}

#[cfg(test)]
mod tests {
    use super::{RequestContext, Tier};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn tiers_have_documented_budgets() {
        assert_eq!(Tier::Summary.max_attempts(), 6);
        assert_eq!(Tier::Standard.max_attempts(), 12);
        assert_eq!(Tier::Full.max_attempts(), 20);
        assert_eq!(Tier::Summary.max_concurrent(), 3);
        assert_eq!(Tier::Full.max_concurrent(), 4);
    }

    #[test]
    fn attempts_and_deadline_enforced() {
        let context =
            RequestContext::new(Tier::Summary, CancellationToken::new(), Default::default());
        for _ in 0..6 {
            assert!(context.try_attempt().is_ok());
        }
        assert!(context.try_attempt().is_err());
    }

    #[tokio::test]
    async fn cancellation_stops_work() {
        let cancel = CancellationToken::new();
        let context = RequestContext::new(Tier::Full, cancel.clone(), Default::default());
        cancel.cancel();
        assert!(context.try_attempt().is_err());
    }
}
