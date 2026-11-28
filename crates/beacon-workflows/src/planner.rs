//! Deterministic planner/executor: capability-aware, parallel, deduped,
//! per-section partial results, real progress only, cancellation-aware.

use std::collections::BTreeSet;

use beacon_api::registry::{OperationMeta, find_by_key};

use crate::context::RequestContext;
use crate::envelope::{Completion, Envelope, SectionResult, SectionStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub operation_key: &'static str,
    pub params: Vec<(String, String)>,
}

pub fn plan(operation_keys: &[&'static str]) -> Vec<Step> {
    let mut seen = BTreeSet::new();
    let mut steps = Vec::new();
    let mut keys: Vec<&'static str> = operation_keys.to_vec();
    keys.sort_unstable();
    for key in keys {
        if seen.insert(key) && find_by_key(key).is_some() {
            steps.push(Step {
                operation_key: key,
                params: Vec::new(),
            });
        }
    }
    steps
}

pub fn capability(
    meta: &OperationMeta,
    polling_v23: bool,
    activity_v22: bool,
) -> Option<SectionStatus> {
    if (meta.key.starts_with("node.polling")
        || meta.key.starts_with("node.poller")
        || meta.key.starts_with("node.telemetry"))
        && !polling_v23
    {
        return Some(SectionStatus::Unsupported);
    }
    if meta.key == "node.activity" && !activity_v22 {
        return Some(SectionStatus::Unsupported);
    }
    None
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn next_actions_for(status: SectionStatus) -> Option<String> {
    match status {
        SectionStatus::Ok => None,
        SectionStatus::NotFound => Some("verify identity and retry".to_owned()),
        SectionStatus::Unsupported => Some("check server version and capability".to_owned()),
        SectionStatus::Disabled => Some("enable the operation and retry".to_owned()),
        SectionStatus::Unavailable => Some("retry later with narrower window".to_owned()),
        SectionStatus::Unknown => Some("retry with explicit identifiers".to_owned()),
        SectionStatus::Timeout => Some("retry with narrower window".to_owned()),
        SectionStatus::BudgetExhausted => Some("retry with narrower scope".to_owned()),
    }
}

pub async fn execute<F, Fut>(
    context: &RequestContext,
    steps: Vec<Step>,
    progress: Option<tokio::sync::mpsc::Sender<String>>,
    mut run: F,
) -> Envelope
where
    F: FnMut(Step) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, (SectionStatus, String)>>,
{
    use futures_util::stream::{self, StreamExt};
    let max_concurrent = context.tier.max_concurrent();
    // Partition: capability-gated steps never touch upstream nor attempts.
    let mut gated: Vec<SectionResult> = Vec::new();
    let mut runnable: Vec<Step> = Vec::new();
    let mut budget_section: Option<SectionResult> = None;
    for step in steps {
        if context.cancel.is_cancelled() {
            break;
        }
        if let Some(meta) = find_by_key(step.operation_key)
            && let Some(status) = capability(
                &meta,
                context.capabilities.polling_v23,
                context.capabilities.activity_v22,
            )
        {
            gated.push(SectionResult {
                name: step.operation_key,
                status,
                data: None,
                error: Some("unsupported for server capability".to_owned()),
                at: now_iso(),
            });
            continue;
        }
        if context.try_attempt().is_err() {
            budget_section = Some(SectionResult {
                name: step.operation_key,
                status: SectionStatus::BudgetExhausted,
                data: None,
                error: Some("attempt budget exhausted".to_owned()),
                at: now_iso(),
            });
            break;
        }
        runnable.push(step);
    }
    let cancel = context.cancel.clone();
    let concurrent = context.concurrent.clone();
    let mut results: Vec<SectionResult> = stream::iter(runnable)
        .map(|step| {
            let cancel = cancel.clone();
            let progress = progress.clone();
            let concurrent = concurrent.clone();
            let key = step.operation_key;
            let future = run(step);
            async move {
                // Per-request + global permits bound fan-out; released on completion.
                let Ok(_request_permit) = concurrent.acquire_owned().await else {
                    return SectionResult {
                        name: key,
                        status: SectionStatus::BudgetExhausted,
                        data: None,
                        error: Some("concurrency unavailable".to_owned()),
                        at: now_iso(),
                    };
                };
                let _global_permit = crate::context::global_permit().await;
                if cancel.is_cancelled() {
                    return SectionResult {
                        name: key,
                        status: SectionStatus::Timeout,
                        data: None,
                        error: Some("cancelled".to_owned()),
                        at: now_iso(),
                    };
                }
                match future.await {
                    Ok(data) => {
                        if let Some(sender) = progress {
                            let _ = sender.send(key.to_owned()).await;
                        }
                        SectionResult {
                            name: key,
                            status: SectionStatus::Ok,
                            data: Some(data),
                            error: None,
                            at: now_iso(),
                        }
                    }
                    Err((status, error)) => SectionResult {
                        name: key,
                        status,
                        data: None,
                        error: Some(error),
                        at: now_iso(),
                    },
                }
            }
        })
        .buffer_unordered(max_concurrent)
        .collect()
        .await;
    results.extend(gated);
    if let Some(section) = budget_section {
        results.push(section);
    }
    results.sort_by_key(|section| section.name);
    let completion = if results.is_empty() {
        Some(Completion::Unavailable)
    } else if results
        .iter()
        .all(|section| section.status == SectionStatus::Ok)
    {
        Some(Completion::Complete)
    } else {
        Some(Completion::Partial)
    };
    let mut next_actions: Vec<String> = results
        .iter()
        .filter_map(|section| next_actions_for(section.status))
        .collect();
    next_actions.sort();
    next_actions.dedup();
    let sources: Vec<String> = results
        .iter()
        .map(|section| section.name.to_owned())
        .collect();
    let query = serde_json::json!({"operations": sources});
    Envelope {
        completion,
        coverage: format!("{} sections", results.len()),
        query,
        sources,
        next_actions,
        sections: results,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{capability, execute, plan};
    use crate::context::{Capabilities, RequestContext, Tier};
    use crate::envelope::SectionStatus;
    use beacon_api::registry::find_by_key;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn plans_are_deterministic_and_deduped() {
        let first = plan(&["node.get", "node.polling", "node.get"]);
        let second = plan(&["node.polling", "node.get", "node.get"]);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn capability_gating_skips_without_upstream() {
        let meta = find_by_key("node.polling").unwrap();
        assert_eq!(
            capability(&meta, false, true),
            Some(SectionStatus::Unsupported)
        );
        assert_eq!(capability(&meta, true, true), None);
    }

    #[tokio::test]
    async fn partial_results_and_progress_and_cancel() {
        let context = RequestContext::new(
            Tier::Summary,
            CancellationToken::new(),
            Capabilities {
                polling_v23: true,
                activity_v22: true,
                ..Default::default()
            },
        );
        let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
        let steps = plan(&["node.get", "node.polling"]);
        let envelope = execute(&context, steps, Some(sender), |step| async move {
            if step.operation_key == "node.get" {
                Ok(serde_json::json!({"ok": true}))
            } else {
                Err((SectionStatus::NotFound, "missing".to_owned()))
            }
        })
        .await;
        assert_eq!(envelope.sections.len(), 2);
        assert!(
            envelope
                .sections
                .iter()
                .any(|section| section.status == SectionStatus::Ok)
        );
        assert!(
            envelope
                .sections
                .iter()
                .all(|section| !section.at.is_empty())
        );
        assert!(!envelope.next_actions.is_empty());
        assert!(
            !envelope
                .query
                .get("operations")
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        let progressed = receiver.recv().await.unwrap();
        assert_eq!(progressed, "node.get");

        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = RequestContext::new(Tier::Summary, cancel, Capabilities::default());
        {
            use std::sync::atomic::{AtomicUsize, Ordering};
            let calls = AtomicUsize::new(0);
            let envelope = execute(&cancelled, plan(&["node.get"]), None, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({})) as Result<_, (SectionStatus, String)>
            })
            .await;
            assert!(envelope.sections.is_empty());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn capability_gating_makes_zero_upstream_calls() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let context = RequestContext::new(
            Tier::Summary,
            CancellationToken::new(),
            Capabilities::default(),
        );
        let envelope = execute(&context, plan(&["node.polling"]), None, |_| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({})) as Result<_, (SectionStatus, String)>
        })
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(envelope.sections.len(), 1);
        assert_eq!(envelope.sections[0].status, SectionStatus::Unsupported);
    }

    #[tokio::test]
    async fn budget_exhaustion_surfaces_typed_section_and_replay_is_deterministic() {
        let context = RequestContext::new(
            Tier::Summary,
            CancellationToken::new(),
            Capabilities {
                polling_v23: true,
                activity_v22: true,
                ..Default::default()
            },
        );
        // Exhaust the 6-attempt Summary budget with 8 steps.
        let many = plan(&[
            "node.get",
            "node.search",
            "node.neighbors",
            "node.observations",
            "node.activity",
            "observer.get",
            "observer.search",
            "packet.get",
        ]);
        let first = execute(&context, many.clone(), None, |_| async {
            Ok(serde_json::json!({"ok": true})) as Result<_, (SectionStatus, String)>
        })
        .await;
        assert!(
            first
                .sections
                .iter()
                .any(|section| section.status == SectionStatus::BudgetExhausted)
        );
        assert!(!first.next_actions.is_empty());

        // Replay on a fresh replica with same input yields same plan and shape.
        let fresh = RequestContext::new(
            Tier::Summary,
            CancellationToken::new(),
            Capabilities {
                polling_v23: true,
                activity_v22: true,
                ..Default::default()
            },
        );
        let second = execute(&fresh, many, None, |_| async {
            Ok(serde_json::json!({"ok": true})) as Result<_, (SectionStatus, String)>
        })
        .await;
        assert_eq!(first.sections.len(), second.sections.len());
    }

    #[tokio::test]
    async fn fault_statuses_carry_next_actions() {
        let context = RequestContext::new(
            Tier::Summary,
            CancellationToken::new(),
            Capabilities {
                polling_v23: true,
                activity_v22: true,
                ..Default::default()
            },
        );
        let envelope = execute(
            &context,
            plan(&["node.get", "node.search"]),
            None,
            |step| async move {
                if step.operation_key == "node.get" {
                    Err((SectionStatus::Timeout, "deadline".to_owned()))
                } else {
                    Err((SectionStatus::Unavailable, "upstream down".to_owned()))
                }
            },
        )
        .await;
        assert!(
            envelope
                .sections
                .iter()
                .all(|section| !section.at.is_empty())
        );
        assert!(
            envelope
                .next_actions
                .iter()
                .any(|action| action.contains("narrower"))
        );
    }
}
