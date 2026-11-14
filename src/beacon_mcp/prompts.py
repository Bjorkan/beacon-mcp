"""Pure read-only analysis prompts with no upstream calls or storage."""

import re

import mcp.types as types
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel import Server
from mcp.shared.exceptions import MCPError

from beacon_mcp.errors import GatewayError
from beacon_mcp.validation import identifier

PROMPTS = [
    {
        "name": "beacon_node_diagnostics",
        "description": "Investigate a node using public Beacon telemetry",
        "arguments": [{"name": "id", "description": "Canonical node UUID", "required": True}],
    },
    {
        "name": "beacon_region_comparison",
        "description": "Compare public network activity between two IATA areas",
        "arguments": [{"name": "iataA", "required": True}, {"name": "iataB", "required": True}],
    },
    {
        "name": "beacon_route_review",
        "description": "Review saved route evidence and related statistics",
        "arguments": [{"name": "iata", "required": True}, {"name": "pathKey", "required": True}],
    },
]


def register(server: Server) -> None:
    async def list_prompts(
        _ctx: ServerRequestContext, _params: types.PaginatedRequestParams
    ) -> types.ListPromptsResult:
        return types.ListPromptsResult(
            prompts=[types.Prompt.model_validate(prompt) for prompt in PROMPTS]
        )

    async def get(
        _ctx: ServerRequestContext, params: types.GetPromptRequestParams
    ) -> types.GetPromptResult:
        def argument(key: str) -> str:
            value = (params.arguments or {}).get(key)
            if not isinstance(value, str):
                raise MCPError(-32602, f"Missing {key}")
            if len(value.encode()) > 128 or re.search(r"[\x00-\x1f\x7f-\x9f]", value):
                raise MCPError(-32602, f"Invalid {key}")
            return value

        def iata(key: str) -> str:
            try:
                return identifier("iata", argument(key))
            except GatewayError:
                raise MCPError(-32602, "Invalid IATA") from None

        if params.name == "beacon_node_diagnostics":
            try:
                node_id = identifier("uuid", argument("id"))
            except GatewayError:
                raise MCPError(-32602, "Invalid node UUID") from None
            text = (
                f"Inspect public MeshCore node {node_id} using beacon_diagnose_node. "
                "Explain originated packet activity, neighbor observations, uncertainty and coverage. "
                "Do not infer that an unseen node is offline. No writes or persistence."
            )
        elif params.name == "beacon_region_comparison":
            iata_a = iata("iataA")
            iata_b = iata("iataB")
            if iata_a == iata_b:
                raise MCPError(-32602, "IATAs must be different")
            text = (
                f"Use beacon_compare_regions with iataA={iata_a} and iataB={iata_b}. "
                "Compare only actual fields, distinguish coverage from performance, "
                "and do not imply synchronized snapshots or causality. No writes or persistence."
            )
        elif params.name == "beacon_route_review":
            code = iata("iata")
            path_key = argument("pathKey")
            if not re.fullmatch(r"[0-9a-fA-F]{32}", path_key):
                raise MCPError(-32602, "Invalid pathKey")
            text = (
                f"Review saved MeshCore route in IATA {code}, pathKey {path_key.lower()}. "
                "Use beacon_get_route_observations for retained evidence and "
                "beacon_get_path_stats for context. Explain evidence gaps. "
                "Do not claim present reachability without recent evidence. Read-only requests."
            )
        else:
            raise MCPError(-32602, "Unknown Beacon prompt")
        return types.GetPromptResult(
            description="Public Beacon read-only analysis workflow",
            messages=[
                types.PromptMessage(role="user", content=types.TextContent(type="text", text=text))
            ],
        )

    server.add_request_handler("prompts/list", types.PaginatedRequestParams, list_prompts)
    server.add_request_handler("prompts/get", types.GetPromptRequestParams, get)
