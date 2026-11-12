use crate::client::{BeaconClient, ErrorCode, Operation};
use crate::resources::parse_uri;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Semaphore, broadcast};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Deserialize)]
struct WsMessage {
    v: u8,
    #[serde(rename = "type")]
    kind: WsMessageKind,
    id: Option<String>,
    event: Option<String>,
    data: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum WsMessageKind {
    Hello,
    Subscribed,
    Pong,
    Lagged,
    Event,
    #[serde(other)]
    Unknown,
}

/// Bounded, process-local bus. There is no replay; clients re-read a resource after updates.
pub struct Live {
    sender: broadcast::Sender<String>,
    pending: Arc<Mutex<HashSet<String>>>,
    pub permits: Arc<Semaphore>,
    pub shutdown: CancellationToken,
    debounce: Duration,
    traffic_enabled: bool,
}

impl Live {
    pub fn new(max_streams: usize, debounce: Duration, shutdown: CancellationToken) -> Arc<Self> {
        Self::configured(max_streams, debounce, shutdown, false)
    }

    /// Public packet/channel notifications are opt-in: they can be high volume.
    pub fn configured(
        max_streams: usize,
        debounce: Duration,
        shutdown: CancellationToken,
        traffic_enabled: bool,
    ) -> Arc<Self> {
        let (sender, _) = broadcast::channel(1024);
        Arc::new(Self {
            sender,
            pending: Arc::new(Mutex::new(HashSet::new())),
            permits: Arc::new(Semaphore::new(max_streams)),
            shutdown,
            debounce,
            traffic_enabled,
        })
    }

    pub fn traffic_enabled(&self) -> bool {
        self.traffic_enabled
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }
    pub fn received(&self, event: &str, data: &Value) {
        // Emit only canonical resource identifiers. Never retain WebSocket
        // message contents or packet payloads in the process-local bus.
        let uri = match event {
            "nodeUpdate" => {
                let Some(id) = data.get("nodeId").and_then(Value::as_str) else {
                    return;
                };
                format!("beacon://nodes/{id}")
            }
            "observerStatus" => {
                let Some(id) = data.get("observerId").and_then(Value::as_str) else {
                    return;
                };
                format!("beacon://observers/{id}")
            }
            "packetObservation" if self.traffic_enabled => {
                let Some(hash) = data.get("packetHash").and_then(Value::as_str) else {
                    return;
                };
                format!("beacon://packets/{hash}")
            }
            "channelMessage" if self.traffic_enabled => {
                let Some(id) = data.get("channelId").and_then(Value::as_i64) else {
                    return;
                };
                format!("beacon://channels/{id}")
            }
            _ => return,
        };
        let Some((kind, id)) = parse_uri(&uri) else {
            return;
        };
        let uri = format!("beacon://{kind}/{id}");
        let Ok(mut pending) = self.pending.lock() else {
            tracing::error!("WebSocket notification queue lock poisoned");
            return;
        };
        if pending.len() >= 1024 || !pending.insert(uri.clone()) {
            return;
        }
        drop(pending);
        let pending = self.pending.clone();
        let sender = self.sender.clone();
        let delay = self.debounce;
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            tokio::select! {
                _=shutdown.cancelled()=>{},
                _=tokio::time::sleep(delay)=>{let _=sender.send(uri.clone());},
            }
            match pending.lock() {
                Ok(mut notifications) => {
                    notifications.remove(&uri);
                }
                Err(_) => tracing::error!("WebSocket notification queue lock poisoned"),
            }
        });
    }
}

/// Fetch a fresh ticket for every connection, then reconnect with bounded delay.
pub async fn bridge(client: BeaconClient, ws_url: Url, live: Arc<Live>) {
    let mut retry = Duration::from_secs(1);
    loop {
        if live.shutdown.is_cancelled() {
            break;
        }
        let outcome = tokio::select! {
            _=live.shutdown.cancelled()=>break,
            result=connect_once(&client,&ws_url,&live,&mut retry)=>result,
        };
        if let Err(error) = outcome {
            tracing::warn!(
                reason = error,
                "Beacon WebSocket disconnected; update notifications may have been missed"
            );
        }
        tokio::select! {
            _=live.shutdown.cancelled()=>break,
            _=tokio::time::sleep(retry)=>{},
        }
        retry = (retry * 2).min(Duration::from_secs(30));
    }
}

async fn connect_once(
    client: &BeaconClient,
    url: &Url,
    live: &Live,
    retry: &mut Duration,
) -> Result<(), &'static str> {
    let ticket = match client.request(Operation::WsTicket, None, &[]).await {
        Ok(value) => Some(
            value
                .get("ticket")
                .and_then(Value::as_str)
                .ok_or("invalid WebSocket ticket")?
                .to_owned(),
        ),
        Err(error) if error.code == ErrorCode::NotFound => None,
        Err(_) => return Err("WebSocket ticket request failed"),
    };
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| "invalid WebSocket URL")?;
    if let Some(ticket) = ticket {
        request.headers_mut().insert(
            "X-Beacon-Ticket",
            ticket
                .parse()
                .map_err(|_| "invalid WebSocket ticket header")?,
        );
    }
    // Reject oversized frames before tungstenite allocates the default 16/64 MiB
    // frame/message buffers. A Beacon WS envelope must fit in 128 KiB.
    let bounds = WebSocketConfig::default()
        .max_message_size(Some(131_072))
        .max_frame_size(Some(131_072));
    let (mut socket, _) = tokio::time::timeout(
        Duration::from_secs(10),
        connect_async_with_config(request, Some(bounds), false),
    )
    .await
    .map_err(|_| "WebSocket connection timed out")?
    .map_err(|_| "WebSocket connection failed")?;
    let mut ready = false;
    let mut seen_hello = false;
    let mut ping_id = 0u64;
    let mut awaiting_pong = false;
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    ping.tick().await;
    let handshake = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(handshake);
    loop {
        tokio::select! {
            _=&mut handshake,if !ready => return Err("WebSocket subscription handshake timed out"),
            _=ping.tick(),if ready => {
                if awaiting_pong {return Err("WebSocket application pong timed out");}
                ping_id=ping_id.checked_add(1).ok_or("WebSocket ping sequence exhausted")?;
                socket.send(Message::Text(serde_json::json!({"v":1,"type":"ping","id":format!("mcp-ping-{ping_id}")}).to_string().into())).await.map_err(|_|"WebSocket ping failed")?;
                awaiting_pong=true;
            }
            next=socket.next()=>{
                let message=next.ok_or("WebSocket closed")?.map_err(|_|"WebSocket receive failed")?;
                let Message::Text(text)=message else {continue};
                if text.len()>131_072 {continue;}
                let Ok(value)=serde_json::from_str::<WsMessage>(&text) else {continue};
                if value.v!=1 {continue;}
                match value.kind {
                    WsMessageKind::Hello if !ready => {
                        seen_hello=true;
                        let mut events = vec!["nodeUpdate", "observerStatus"];
                        if live.traffic_enabled {
                            events.extend(["packetObservation", "channelMessage"]);
                        }
                        socket.send(Message::Text(serde_json::json!({"v":1,"type":"subscribe","id":"mcp-changes","scope":{"events":events}}).to_string().into())).await.map_err(|_|"WebSocket subscribe failed")?;
                    }
                    WsMessageKind::Subscribed if seen_hello && value.id.as_deref()==Some("mcp-changes")=>{
                        ready=true;
                        *retry=Duration::from_secs(1);
                        tracing::info!("Beacon WebSocket subscribed to resource updates");
                    },
                    WsMessageKind::Pong if value.id.as_deref()==Some(format!("mcp-ping-{ping_id}").as_str())=>{awaiting_pong=false;},
                    WsMessageKind::Lagged=>{tracing::warn!("Beacon WebSocket dropped events; no replay available");},
                    WsMessageKind::Event if ready=>{
                        if let (Some(event),Some(data))=(value.event.as_deref(),value.data.as_ref()) {
                            live.received(event,data);
                        }
                    }
                    _=>{},
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Live;
    use serde_json::json;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    #[tokio::test]
    async fn debounce_and_validate_beacon_update_events() {
        let live = Live::new(2, Duration::from_millis(15), CancellationToken::new());
        let mut events = live.subscribe();
        live.received(
            "nodeUpdate",
            &json!({"nodeId":"123e4567-e89b-42d3-a456-426614174000"}),
        );
        live.received(
            "nodeUpdate",
            &json!({"nodeId":"123e4567-e89b-42d3-a456-426614174000"}),
        );
        live.received("observerStatus", &json!({"observerId":"../admin"}));
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(300), events.recv())
                .await
                .unwrap()
                .unwrap(),
            "beacon://nodes/123e4567-e89b-42d3-a456-426614174000"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(40), events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn packet_and_channel_hints_are_disabled_by_default_and_keep_only_uris() {
        let shutdown = CancellationToken::new();
        let disabled = Live::new(1, Duration::ZERO, shutdown.clone());
        let mut quiet = disabled.subscribe();
        disabled.received(
            "packetObservation",
            &json!({"packetHash":"AABBCCDDEEFF0011", "content":"do-not-store"}),
        );
        disabled.received(
            "channelMessage",
            &json!({"channelId":25,"content":"do-not-store"}),
        );
        assert!(tokio::time::timeout(Duration::from_millis(30), quiet.recv())
            .await
            .is_err());

        let live = Live::configured(1, Duration::ZERO, shutdown, true);
        let mut events = live.subscribe();
        live.received("packetObservation", &json!({"packetHash":"AABBCCDDEEFF0011"}));
        live.received("channelMessage", &json!({"channelId":25,"content":"secret"}));
        live.received("packetObservation", &json!({"packetHash":"../../admin"}));
        live.received("channelMessage", &json!({"channelId":0}));
        let a = tokio::time::timeout(Duration::from_millis(300), events.recv())
            .await.unwrap().unwrap();
        let b = tokio::time::timeout(Duration::from_millis(300), events.recv())
            .await.unwrap().unwrap();
        let mut uris = [a, b];
        uris.sort();
        assert_eq!(
            uris,
            [
                "beacon://channels/25".to_owned(),
                "beacon://packets/aabbccddeeff0011".to_owned()
            ]
        );
        assert!(tokio::time::timeout(Duration::from_millis(30), events.recv())
            .await.is_err());
    }

}
