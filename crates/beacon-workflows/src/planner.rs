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
    let mut admitted = Vec::new();
    for step in steps {
        if context.cancel.is_cancelled() {
            break;
        }
        if context.try_attempt().is_err() {
            break;
        }
        admitted.push(step);
    }
    let cancel = context.cancel.clone();
    let results: Vec<SectionResult> = stream::iter(admitted)
        .map(|step| {
            let cancel = cancel.clone();
            let progress = progress.clone();
            let key = step.operation_key;
            let future = run(step);
            async move {
                if cancel.is_cancelled() {
                    return SectionResult {
                        name: key,
                        status: SectionStatus::Timeout,
                        data: None,
                        error: Some("cancelled".to_owned()),
                        at: String::new(),
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
                            at: String::new(),
                        }
                    }
                    Err((status, error)) => SectionResult {
                        name: key,
                        status,
                        data: None,
                        error: Some(error),
                        at: String::new(),
                    },
                }
            }
        })
        .buffer_unordered(max_concurrent)
        .collect()
        .await;
    let mut sections = results;
    sections.sort_by_key(|section| section.name);
    let completion = if sections.is_empty() {
        Some(Completion::Unavailable)
    } else if sections
        .iter()
        .all(|section| section.status == SectionStatus::Ok)
    {
        Some(Completion::Complete)
    } else {
        Some(Completion::Partial)
    };
    Envelope {
        completion,
        coverage: format!("{} sections", sections.len()),
        sections,
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
            Capabilities::default(),
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
        let progressed = receiver.recv().await.unwrap();
        assert_eq!(progressed, "node.get");

        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = RequestContext::new(Tier::Summary, cancel, Capabilities::default());
        let envelope = execute(&cancelled, plan(&["node.get"]), None, |_| async {
            Ok(serde_json::json!({})) as Result<_, (SectionStatus, String)>
        })
        .await;
        assert!(envelope.sections.is_empty());
    }
}
