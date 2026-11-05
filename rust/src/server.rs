use crate::{
    analytics_tools::AnalyticsTools, client::BeaconClient, packet_tools::PacketTools,
    routing_tools::RoutingTools, subscriptions::Live, tools::CoreTools,
};
use axum::{Router, routing::get};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, ListResourceTemplatesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
        ReadResourceResult, ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
        SubscriptionFilter, Tool,
    },
    service::{RequestContext, RoleServer, SubscriptionContext},
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct BeaconServer {
    pub tools: CoreTools,
    pub packet_tools: PacketTools,
    pub routing_tools: RoutingTools,
    pub analytics_tools: AnalyticsTools,
    pub live: Option<Arc<Live>>,
}

impl ServerHandler for BeaconServer {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources();
        if self.live.is_some() {
            capabilities = capabilities.enable_resources_subscribe();
        }
        let mut info = ServerConfig::new(capabilities.build());
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
                .chain(self.analytics_tools.list())
                .collect(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools
            .definition(name)
            .or_else(|| self.packet_tools.definition(name))
            .or_else(|| self.routing_tools.definition(name))
            .or_else(|| self.analytics_tools.definition(name))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: ["nodes", "observers"]
                .into_iter()
                .map(|kind| {
                    ResourceTemplate::new(
                        format!("beacon://{kind}/{{id}}"),
                        format!("beacon-{kind}"),
                    )
                    .with_title(format!(
                        "Beacon {} details",
                        if kind == "nodes" { "node" } else { "observer" }
                    ))
                    .with_description("Read the latest public information from Beacon REST")
                    .with_mime_type("application/json")
                })
                .collect(),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let (kind, id) = crate::resources::parse_uri(&request.uri)
            .ok_or_else(|| McpError::invalid_params("Invalid Beacon resource URI", None))?;
        let op = if kind == "nodes" {
            crate::client::Operation::Node
        } else {
            crate::client::Operation::Observer
        };
        let data = self
            .tools
            .client()
            .request(op, Some(&id), &[])
            .await
            .map_err(|error| {
                if error.code == crate::client::ErrorCode::NotFound {
                    McpError::resource_not_found("Beacon resource not found", None)
                } else {
                    McpError::internal_error("Beacon resource temporarily unavailable", None)
                }
            })?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(data.to_string(), format!("beacon://{kind}/{id}"))
                .with_mime_type("application/json"),
        ])
        .into())
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        self.live.as_ref()?;
        let accepted = requested
            .resource_subscriptions
            .as_ref()
            .map(|uris| {
                uris.iter()
                    .filter(|uri| crate::resources::parse_uri(uri).is_some())
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(
            SubscriptionFilter::builder()
                .resource_subscriptions(accepted)
                .build(),
        )
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let live = self
            .live
            .as_ref()
            .ok_or_else(|| McpError::internal_error("Subscriptions unavailable", None))?;
        let _permit = live
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| McpError::internal_error("Subscription stream limit reached", None))?;
        let mut receiver = live.subscribe();
        let accepted = context
            .accepted()
            .resource_subscriptions
            .clone()
            .unwrap_or_default();
        loop {
            tokio::select! {
                _=context.cancelled()=>break,
                _=live.shutdown.cancelled()=>break,
                update=receiver.recv()=>match update {
                    Ok(uri) if accepted.contains(&uri)=> {
                        if context.sink().notify_resource_updated(uri).await.is_err() {break;}
                    }
                    Ok(_)|Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{},
                    Err(tokio::sync::broadcast::error::RecvError::Closed)=>break,
                }
            }
        }
        Ok(())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let result = match if self
            .analytics_tools
            .definition(request.name.as_ref())
            .is_some()
        {
            self.analytics_tools
                .execute(request.name.as_ref(), request.arguments)
                .await
        } else if self
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
    router_with_live(shutdown, client, None)
}

pub fn router_with_live(
    shutdown: CancellationToken,
    client: BeaconClient,
    live: Option<Arc<Live>>,
) -> Router {
    let mcp: StreamableHttpService<BeaconServer, LocalSessionManager> = StreamableHttpService::new(
        move || {
            Ok(BeaconServer {
                tools: CoreTools::new(client.clone()),
                packet_tools: PacketTools::new(client.clone()),
                routing_tools: RoutingTools::new(client.clone()),
                analytics_tools: AnalyticsTools::new(client.clone()),
                live: live.clone(),
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
