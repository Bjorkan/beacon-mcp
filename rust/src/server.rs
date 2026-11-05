use crate::{client::BeaconClient, tools::CoreTools};
use axum::{Router, routing::get};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// Foundation only. Tools are added with their implementation and tests in later layers.
#[derive(Clone)]
pub struct BeaconServer {
    pub tools: CoreTools,
}

impl ServerHandler for BeaconServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info.name = "beacon-mcp".into();
        info.server_info.version = "2.1.1".into();
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tools.list(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.definition(name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let result = match self
            .tools
            .execute(request.name.as_ref(), request.arguments)
            .await
        {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => CallToolResult::structured_error(serde_json::json!({"error": {
                "code": error.code.as_str(),
                "message": error.message,
                "status": error.status.map(|value| value.as_u16()),
                "upstreamCode": error.upstream_code,
                "retryAfter": error.retry_after,
            }})),
        };
        Ok(result.into())
    }
}

async fn health() -> axum::Json<Value> {
    axum::Json(json!({ "status": "ok" }))
}

pub fn router(shutdown: CancellationToken, client: BeaconClient) -> Router {
    let mcp: StreamableHttpService<BeaconServer, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(BeaconServer {
                tools: CoreTools::new(client.clone()),
            })
        },
        Default::default(),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_cancellation_token(shutdown)
            // The public gateway already exposes arbitrary Host values behind a TLS proxy.
            .disable_allowed_hosts(),
    );
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .fallback_service(mcp)
}
