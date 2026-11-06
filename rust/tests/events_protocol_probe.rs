//! SDK compatibility probe only. The real gateway keeps Events unavailable
//! until authenticated, durable delivery and capability serialization exist.
use axum::Router;
use beacon_mcp_rust::events::{DeliveryMode, EventKind};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{CustomRequest, CustomResult, ServerCapabilities, ServerConfig},
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};

#[derive(Clone)]
struct ProtocolProbe;

impl ServerHandler for ProtocolProbe {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::default())
    }

    async fn on_custom_request(
        &self,
        request: CustomRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<CustomResult, McpError> {
        if request.method != "events/list" {
            return Err(McpError::new(
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                request.method,
                None,
            ));
        }
        Ok(CustomResult::new(
            json!({"events": EventKind::ALL.into_iter()
            .filter_map(|kind| kind.descriptor(&[DeliveryMode::Webhook]))
            .collect::<Vec<_>>()}),
        ))
    }
}

#[tokio::test]
async fn sdk_routes_custom_protocol_methods_and_checks_headers_in_both_lifecycles() {
    let service: StreamableHttpService<ProtocolProbe, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(ProtocolProbe),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_json_response(true)
                .disable_allowed_hosts(),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, Router::new().fallback_service(service))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    for version in ["2026-07-28", "2025-11-25"] {
        let mut params = json!({});
        if version == "2026-07-28" {
            params = json!({"_meta":{
                "io.modelcontextprotocol/protocolVersion":version,
                "io.modelcontextprotocol/clientInfo":{"name":"probe","version":"1"},
                "io.modelcontextprotocol/clientCapabilities":{}
            }});
        }
        let body = json!({"jsonrpc":"2.0","id":7,"method":"events/list","params":params});
        for (header, good) in [("events/list", true), ("tools/list", false)] {
            let response = client
                .post(&url)
                .header("Accept", "application/json, text/event-stream")
                .header("MCP-Protocol-Version", version)
                .header("Mcp-Method", header)
                .json(&body)
                .send()
                .await
                .unwrap();
            let status = response.status();
            let value: Value = response.json().await.unwrap();
            if good {
                assert_eq!(status, 200, "{value}");
                assert_eq!(
                    value["result"]["events"].as_array().unwrap().len(),
                    2,
                    "{value}"
                );
                assert_eq!(value["result"]["events"][0]["delivery"], json!(["webhook"]));
            } else if version == "2026-07-28" {
                assert_eq!(status, 400, "{value}");
                assert!(value["error"].is_object(), "{value}");
            }
        }
    }
    task.abort();
}
