//! Pure stateless MCP prompt templates. No upstream request or user data storage.
use rmcp::{
    ErrorData as McpError,
    model::{
        GetPromptRequestParams, GetPromptResponse, GetPromptResult, ListPromptsResult, Prompt,
        PromptArgument,
        PromptMessage, Role,
    },
};
use uuid::{Uuid, Variant};

pub fn list() -> ListPromptsResult {
    ListPromptsResult {
        prompts: vec![
            Prompt::new(
                "beacon_node_diagnostics",
                Some("Investigate a node using public Beacon telemetry"),
                Some(vec![
                    PromptArgument::new("id")
                        .with_description("Canonical node UUID")
                        .with_required(true),
                ]),
            ),
            Prompt::new(
                "beacon_region_comparison",
                Some("Compare public network activity between two IATA areas"),
                Some(vec![
                    PromptArgument::new("iataA").with_required(true),
                    PromptArgument::new("iataB").with_required(true),
                ]),
            ),
            Prompt::new(
                "beacon_route_review",
                Some("Review saved route evidence and related statistics"),
                Some(vec![
                    PromptArgument::new("iata").with_required(true),
                    PromptArgument::new("pathKey").with_required(true),
                ]),
            ),
        ],
        ..Default::default()
    }
}

fn argument<'a>(request: &'a GetPromptRequestParams, key: &str) -> Result<&'a str, McpError> {
    let value = request
        .arguments
        .as_ref()
        .and_then(|args| args.get(key))
        .and_then(|value| value.as_str())
        .ok_or_else(|| McpError::invalid_params(format!("Missing {key}"), None))?;
    if value.len() > 128 || value.chars().any(char::is_control) {
        return Err(McpError::invalid_params(format!("Invalid {key}"), None));
    }
    Ok(value)
}

fn iata(input: &str) -> Result<String, McpError> {
    if input.len() != 3 || !input.bytes().all(|value| value.is_ascii_alphanumeric()) {
        return Err(McpError::invalid_params("Invalid IATA", None));
    }
    Ok(input.to_ascii_uppercase())
}

pub fn get(request: &GetPromptRequestParams) -> Result<GetPromptResponse, McpError> {
    let message = match request.name.as_str() {
        "beacon_node_diagnostics" => {
            let id = argument(request, "id")?;
            let uuid = Uuid::parse_str(id)
                .map_err(|_| McpError::invalid_params("Invalid node UUID", None))?;
            if id.len() != 36
                || uuid.get_variant() != Variant::RFC4122
                || !(1..=8).contains(&uuid.get_version_num())
            {
                return Err(McpError::invalid_params("Invalid node UUID", None));
            }
            format!(
                "Inspect public MeshCore node {uuid} using beacon_diagnose_node. Explain originated packet activity, neighbor observations, uncertainty and coverage. Do not infer that an unseen node is offline. No writes or persistence."
            )
        }
        "beacon_region_comparison" => {
            let a = iata(argument(request, "iataA")?)?;
            let b = iata(argument(request, "iataB")?)?;
            if a == b {
                return Err(McpError::invalid_params("IATAs must be different", None));
            }
            format!(
                "Use beacon_compare_regions with iataA={a} and iataB={b}. Compare only actual fields, distinguish coverage from performance, and do not imply synchronized snapshots or causality. No writes or persistence."
            )
        }
        "beacon_route_review" => {
            let code = iata(argument(request, "iata")?)?;
            let key = argument(request, "pathKey")?;
            if key.len() != 32 || !key.bytes().all(|value| value.is_ascii_hexdigit()) {
                return Err(McpError::invalid_params("Invalid pathKey", None));
            }
            format!(
                "Review saved MeshCore route in IATA {code}, pathKey {}. Use beacon_get_route_observations for retained evidence and beacon_get_path_stats for context. Explain evidence gaps. Do not claim present reachability without recent evidence. Read-only requests.",
                key.to_ascii_lowercase()
            )
        }
        _ => return Err(McpError::invalid_params("Unknown Beacon prompt", None)),
    };
    Ok(GetPromptResponse::Complete(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, message)])
            .with_description("Public Beacon read-only analysis workflow"),
    ))
}
