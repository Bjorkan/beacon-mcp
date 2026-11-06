use crate::{
    client::BeaconClient, packet_tools::PacketTools, routing_tools::RoutingTools, tools::CoreTools,
};
use axum::{Router, routing::get};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct BeaconServer {
    pub tools: CoreTools,
    pub packet_tools: PacketTools,
    pub routing_tools: RoutingTools,
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
            tools: self
                .tools
                .list()
                .into_iter()
                .chain(self.packet_tools.list())
                .chain(self.routing_tools.list())
                .collect(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools
            .definition(name)
            .or_else(|| self.packet_tools.definition(name))
            .or_else(|| self.routing_tools.definition(name))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let result = match if self
            .routing_tools
            .definition(request.name.as_ref())
            .is_some()
        {
            self.routing_tools
                .execute(request.name.as_ref(), request.arguments)
                .await
        } else if self
            .packet_tools
            .definition(request.name.as_ref())
            .is_some()
        {
            self.packet_tools
                .execute(request.name.as_ref(), request.arguments)
                .await
        } else {
            self.tools
                .execute(request.name.as_ref(), request.arguments)
                .await
        } {
            Ok(value) => crate::result::success(value),
            Err(error) => crate::result::error(error),
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
                packet_tools: PacketTools::new(client.clone()),
                routing_tools: RoutingTools::new(client.clone()),
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
