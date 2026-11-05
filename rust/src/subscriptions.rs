use crate::client::{BeaconClient, ErrorCode, Operation};
use crate::resources::parse_uri;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Semaphore, broadcast};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use url::Url;

/// Bounded, process-local bus. There is no replay; clients re-read a resource after updates.
pub struct Live {
    sender: broadcast::Sender<String>,
    pending: Arc<Mutex<HashSet<String>>>,
    pub permits: Arc<Semaphore>,
    pub shutdown: CancellationToken,
    debounce: Duration,
}

impl Live {
    pub fn new(max_streams: usize, debounce: Duration, shutdown: CancellationToken) -> Arc<Self> {
        let (sender, _) = broadcast::channel(1024);
        Arc::new(Self {
            sender,
            pending: Arc::new(Mutex::new(HashSet::new())),
            permits: Arc::new(Semaphore::new(max_streams)),
            shutdown,
            debounce,
        })
    }
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }
    pub fn received(&self, event: &str, data: &Value) {
        let (kind, field) = match event {
            "nodeUpdate" => ("nodes", "nodeId"),
            "observerStatus" => ("observers", "observerId"),
            _ => return,
        };
        let Some(id) = data.get(field).and_then(Value::as_str) else {
            return;
        };
        let uri = format!("beacon://{kind}/{id}");
        let Some((kind, id)) = parse_uri(&uri) else {
            return;
        };
        let uri = format!("beacon://{kind}/{id}");
        let mut pending = self.pending.lock().expect("notification queue");
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
            pending.lock().expect("notification queue").remove(&uri);
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
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(request))
        .await
        .map_err(|_| "WebSocket connection timed out")?
        .map_err(|_| "WebSocket connection failed")?;
    let mut ready = false;
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
                ping_id+=1;
                socket.send(Message::Text(serde_json::json!({"v":1,"type":"ping","id":format!("mcp-ping-{ping_id}")}).to_string().into())).await.map_err(|_|"WebSocket ping failed")?;
                awaiting_pong=true;
            }
            next=socket.next()=>{
                let message=next.ok_or("WebSocket closed")?.map_err(|_|"WebSocket receive failed")?;
                let Message::Text(text)=message else {continue};
                if text.len()>131_072 {continue;}
                let Ok(value)=serde_json::from_str::<Value>(&text) else {continue};
                if value["v"]!=1 {continue;}
                match value["type"].as_str() {
                    Some("hello") if !ready => {
                        socket.send(Message::Text(serde_json::json!({"v":1,"type":"subscribe","id":"mcp-changes","scope":{"events":["nodeUpdate","observerStatus"]}}).to_string().into())).await.map_err(|_|"WebSocket subscribe failed")?;
                    }
                    Some("subscribed") if value["id"]=="mcp-changes"=>{
                        ready=true;
                        *retry=Duration::from_secs(1);
                        tracing::info!("Beacon WebSocket subscribed to resource updates");
                    },
                    Some("pong") if value["id"]==format!("mcp-ping-{ping_id}")=>{awaiting_pong=false;},
                    Some("lagged")=>{tracing::warn!("Beacon WebSocket dropped events; no replay available");},
                    Some("event") if ready=>{
                        if let Some(event)=value["event"].as_str() {live.received(event,&value["data"]);}
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
}
