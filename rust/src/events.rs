//! Internal, ephemeral event ingress. This is not an MCP Events delivery service:
//! there is no authentication, durable outbox, replay, or Events discovery yet.
use crate::resources::parse_uri;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventOccurrence {
    pub event_id: Uuid,
    pub name: &'static str,
    pub timestamp: String,
    pub data: EventData,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventData {
    pub resource_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observer_id: Option<String>,
}

impl EventOccurrence {
    pub fn from_upstream(event: &str, data: &Value) -> Option<Self> {
        let (name, kind, field) = match event {
            "nodeUpdate" => ("beacon.node.updated", "nodes", "nodeId"),
            "observerStatus" => ("beacon.observer.status_changed", "observers", "observerId"),
            _ => return None,
        };
        let raw = data.get(field)?.as_str()?;
        let uri = format!("beacon://{kind}/{raw}");
        let (_, id) = parse_uri(&uri)?;
        Some(Self {
            event_id: Uuid::new_v4(),
            name,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            data: EventData {
                resource_uri: format!("beacon://{kind}/{id}"),
                node_id: (kind == "nodes").then(|| id.clone()),
                observer_id: (kind == "observers").then_some(id),
            },
        })
    }

    /// Only exact, canonical UUID filters can match an occurrence.
    pub fn matches(&self, arguments: &Value) -> bool {
        let (field, id) = match self.name {
            "beacon.node.updated" => ("nodeId", self.data.node_id.as_deref()),
            "beacon.observer.status_changed" => ("observerId", self.data.observer_id.as_deref()),
            _ => return false,
        };
        let Some(object) = arguments.as_object() else {
            return false;
        };
        object.len() == 1 && object.get(field).and_then(Value::as_str) == id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapReason {
    UpstreamLagged,
    Disconnected,
    ConsumerLagged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngressMessage {
    Occurrence(EventOccurrence),
    Gap(GapReason),
}

pub struct EventReceiver(broadcast::Receiver<IngressMessage>);

impl EventReceiver {
    pub(crate) fn new(receiver: broadcast::Receiver<IngressMessage>) -> Self {
        Self(receiver)
    }

    pub async fn recv(&mut self) -> Result<IngressMessage, broadcast::error::RecvError> {
        match self.0.recv().await {
            Err(broadcast::error::RecvError::Lagged(_)) => {
                Ok(IngressMessage::Gap(GapReason::ConsumerLagged))
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::Live;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    const NODE: &str = "123e4567-e89b-42d3-a456-426614174000";
    const OBSERVER: &str = "123e4567-e89b-42d3-a456-426614174001";

    #[tokio::test]
    async fn occurrences_are_individual_while_resource_hints_are_debounced() {
        let live = Live::new(1, Duration::from_millis(20), CancellationToken::new());
        let mut occurrences = live.subscribe_occurrences();
        let mut hints = live.subscribe();
        for _ in 0..2 {
            live.received(
                "nodeUpdate",
                &serde_json::json!({"nodeId":NODE,"payload":"ignored"}),
            );
        }
        let IngressMessage::Occurrence(first) = occurrences.recv().await.unwrap() else {
            panic!()
        };
        let IngressMessage::Occurrence(second) = occurrences.recv().await.unwrap() else {
            panic!()
        };
        assert_ne!(first.event_id, second.event_id);
        assert_eq!(first.name, "beacon.node.updated");
        assert!(first.matches(&serde_json::json!({"nodeId":NODE})));
        assert!(!first.matches(&serde_json::json!({"nodeId":OBSERVER})));
        assert!(!first.matches(&serde_json::json!({"nodeId":NODE,"extra":1})));
        assert_eq!(first.data.node_id.as_deref(), Some(NODE));
        assert_eq!(
            serde_json::to_value(&first.data).unwrap(),
            serde_json::json!({"resourceUri":format!("beacon://nodes/{NODE}"),"nodeId":NODE})
        );
        assert!(chrono::DateTime::parse_from_rfc3339(&first.timestamp).is_ok());
        assert_eq!(
            hints.recv().await.unwrap(),
            format!("beacon://nodes/{NODE}")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), hints.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn validates_ids_and_reports_upstream_and_consumer_gaps() {
        let live = Live::new(1, Duration::ZERO, CancellationToken::new());
        let mut receiver = live.subscribe_occurrences();
        live.received(
            "observerStatus",
            &serde_json::json!({"observerId":OBSERVER,"status":"unknown"}),
        );
        let IngressMessage::Occurrence(observer) = receiver.recv().await.unwrap() else {
            panic!()
        };
        assert!(observer.matches(&serde_json::json!({"observerId":OBSERVER})));
        assert_eq!(observer.data.observer_id.as_deref(), Some(OBSERVER));
        for (event, data) in [
            ("nodeUpdate", serde_json::json!({"nodeId":"../admin"})),
            ("nodeUpdate", serde_json::json!({"nodeId":42})),
            ("packetObservation", serde_json::json!({"nodeId":NODE})),
        ] {
            live.received(event, &data);
        }
        live.gap(GapReason::UpstreamLagged);
        assert_eq!(
            receiver.recv().await.unwrap(),
            IngressMessage::Gap(GapReason::UpstreamLagged)
        );
        live.gap(GapReason::Disconnected);
        assert_eq!(
            receiver.recv().await.unwrap(),
            IngressMessage::Gap(GapReason::Disconnected)
        );
        for _ in 0..1100 {
            live.received("nodeUpdate", &serde_json::json!({"nodeId":NODE}));
        }
        assert_eq!(
            receiver.recv().await.unwrap(),
            IngressMessage::Gap(GapReason::ConsumerLagged)
        );
    }
}
