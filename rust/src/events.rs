//! Internal, ephemeral event ingress. This is not an MCP Events delivery service:
//! there is no authentication, durable outbox, replay, or Events discovery yet.
use crate::resources::parse_uri;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

const UUID_PATTERN: &str =
    "^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    NodeUpdated,
    ObserverStatusChanged,
}

impl EventKind {
    pub const ALL: [Self; 2] = [Self::NodeUpdated, Self::ObserverStatusChanged];

    pub fn name(self) -> &'static str {
        match self {
            Self::NodeUpdated => "beacon.node.updated",
            Self::ObserverStatusChanged => "beacon.observer.status_changed",
        }
    }

    fn field(self) -> &'static str {
        match self {
            Self::NodeUpdated => "nodeId",
            Self::ObserverStatusChanged => "observerId",
        }
    }

    fn resource(self) -> &'static str {
        match self {
            Self::NodeUpdated => "nodes",
            Self::ObserverStatusChanged => "observers",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn descriptor(self, delivery: &[DeliveryMode]) -> Option<Value> {
        if delivery.is_empty() {
            return None;
        }
        let field = self.field();
        let description = match self {
            Self::NodeUpdated => {
                "A Beacon node was updated. Read its resource for current details; changed fields are not known."
            }
            Self::ObserverStatusChanged => {
                "A Beacon observer status changed. Read its resource for current details; status is not provided by this event."
            }
        };
        // The schema's canonical UUID constraint matches parse_uri and EventFilter.
        let id = serde_json::json!({"type":"string","format":"uuid","pattern":UUID_PATTERN});
        let mut input_properties = serde_json::Map::new();
        input_properties.insert(field.into(), id.clone());
        let mut payload_properties = input_properties.clone();
        payload_properties.insert("resourceUri".into(),serde_json::json!({"type":"string","pattern":format!("^beacon://{}/{}$",self.resource(),UUID_PATTERN.trim_start_matches('^').trim_end_matches('$'))}));
        Some(serde_json::json!({
            "name":self.name(), "description":description, "delivery":delivery,
            "inputSchema":{"type":"object","properties":input_properties,"required":[field],"additionalProperties":false},
            "payloadSchema":{"type":"object","properties":payload_properties,"required":[field,"resourceUri"],"additionalProperties":false}
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryMode {
    Webhook,
    Push,
    Poll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventFilter {
    kind: EventKind,
    id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    UnknownEvent,
    InvalidArguments,
}

impl EventFilter {
    pub fn parse(name: &str, arguments: &Value) -> Result<Self, FilterError> {
        let kind = EventKind::from_name(name).ok_or(FilterError::UnknownEvent)?;
        let object = arguments.as_object().ok_or(FilterError::InvalidArguments)?;
        if object.len() != 1 {
            return Err(FilterError::InvalidArguments);
        }
        let id = object
            .get(kind.field())
            .and_then(Value::as_str)
            .ok_or(FilterError::InvalidArguments)?;
        let uri = format!("beacon://{}/{id}", kind.resource());
        let (_, canonical) = parse_uri(&uri).ok_or(FilterError::InvalidArguments)?;
        if id != canonical {
            return Err(FilterError::InvalidArguments);
        }
        Ok(Self {
            kind,
            id: canonical,
        })
    }

    pub fn matches(&self, event: &EventOccurrence) -> bool {
        self.kind.name() == event.name
            && match self.kind {
                EventKind::NodeUpdated => event.data.node_id.as_deref() == Some(self.id.as_str()),
                EventKind::ObserverStatusChanged => {
                    event.data.observer_id.as_deref() == Some(self.id.as_str())
                }
            }
    }
}

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
        EventFilter::parse(self.name, arguments).is_ok_and(|filter| filter.matches(self))
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

    #[test]
    fn catalog_requires_a_real_delivery_mode_and_schema_agrees_with_filter() {
        for (kind, field, id) in [
            (EventKind::NodeUpdated, "nodeId", NODE),
            (EventKind::ObserverStatusChanged, "observerId", OBSERVER),
        ] {
            assert!(
                kind.descriptor(&[]).is_none(),
                "no unsupported delivery promise"
            );
            let descriptor = kind.descriptor(&[DeliveryMode::Webhook]).unwrap();
            assert_eq!(descriptor["name"], kind.name());
            assert_eq!(descriptor["delivery"], serde_json::json!(["webhook"]));
            assert_eq!(
                descriptor["inputSchema"]["required"],
                serde_json::json!([field])
            );
            assert_eq!(
                descriptor["payloadSchema"]["required"],
                serde_json::json!([field, "resourceUri"])
            );
            assert_eq!(descriptor["inputSchema"]["additionalProperties"], false);
            let pattern = descriptor["inputSchema"]["properties"][field]["pattern"]
                .as_str()
                .unwrap();
            let regex = regex::Regex::new(pattern).unwrap();
            assert!(regex.is_match(id));
            let filter = EventFilter::parse(kind.name(), &serde_json::json!({field:id})).unwrap();
            let upstream = match kind {
                EventKind::NodeUpdated => serde_json::json!({"nodeId":id}),
                EventKind::ObserverStatusChanged => serde_json::json!({"observerId":id}),
            };
            let occurrence = EventOccurrence::from_upstream(
                if kind == EventKind::NodeUpdated {
                    "nodeUpdate"
                } else {
                    "observerStatus"
                },
                &upstream,
            )
            .unwrap();
            assert!(filter.matches(&occurrence));
            assert_eq!(
                serde_json::to_value(&occurrence.data)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .len(),
                2
            );
            let payload_pattern =
                descriptor["payloadSchema"]["properties"]["resourceUri"]["pattern"]
                    .as_str()
                    .unwrap();
            assert!(
                regex::Regex::new(payload_pattern)
                    .unwrap()
                    .is_match(&occurrence.data.resource_uri)
            );
        }
        assert_eq!(
            EventFilter::parse("missing", &serde_json::json!({"nodeId":NODE})),
            Err(FilterError::UnknownEvent)
        );
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"nodeId":NODE,"extra":true}),
            serde_json::json!({"nodeId":NODE.to_uppercase()}),
            serde_json::json!({"nodeId":"../admin"}),
            serde_json::json!({"nodeId":null}),
            serde_json::json!([]),
        ] {
            assert_eq!(
                EventFilter::parse(EventKind::NodeUpdated.name(), &invalid),
                Err(FilterError::InvalidArguments)
            );
        }
        let observer = EventOccurrence::from_upstream(
            "observerStatus",
            &serde_json::json!({"observerId":OBSERVER}),
        )
        .unwrap();
        assert!(
            !EventFilter::parse(
                EventKind::NodeUpdated.name(),
                &serde_json::json!({"nodeId":NODE})
            )
            .unwrap()
            .matches(&observer)
        );
    }
}
