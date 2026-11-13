"""Seven canonical resources; every read fetches fresh public Beacon data."""

import json

import mcp.types as types
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel import Server
from mcp.shared.exceptions import MCPError

from beacon_mcp.client import BeaconClient, GatewayError, Operation, identifier

RESOURCE_OPERATIONS = {
    "nodes": (Operation.Node, "uuid"),
    "observers": (Operation.Observer, "uuid"),
    "packets": (Operation.Packet, "hash"),
    "channels": (Operation.ChannelDetail, "number"),
    "traces": (Operation.Trace, "tag"),
    "iatas": (Operation.IataDetail, "iata"),
    "regions": (Operation.RegionDetail, "number"),
}


def parse_uri(uri: str) -> tuple[str, str] | None:
    if not uri.startswith("beacon://"):
        return None
    kind, separator, raw = uri[9:].partition("/")
    if not separator or kind not in RESOURCE_OPERATIONS:
        return None
    try:
        id = identifier(RESOURCE_OPERATIONS[kind][1], raw)
        if kind in ("channels", "regions"):
            id = str(int(id))
    except (GatewayError, ValueError):
        return None
    if f"beacon://{kind}/{id}".lower() != uri.lower():
        return None
    return kind, id


def rpc_error(code: int, message: str) -> MCPError:
    return MCPError(code, message)


def register(server: Server, client: BeaconClient) -> None:
    async def list_resources(
        _ctx: ServerRequestContext, _params: types.PaginatedRequestParams
    ) -> types.ListResourcesResult:
        return types.ListResourcesResult(resources=[])

    async def templates(
        _ctx: ServerRequestContext, _params: types.PaginatedRequestParams
    ) -> types.ListResourceTemplatesResult:
        return types.ListResourceTemplatesResult(
            resource_templates=[
                types.ResourceTemplate(
                    uri_template=f"beacon://{kind}/{{id}}",
                    name=f"beacon-{kind}",
                    title=f"Beacon {kind.rstrip('s')} details",
                    description="Read the latest public information from Beacon REST",
                    mime_type="application/json",
                )
                for kind in RESOURCE_OPERATIONS
            ]
        )

    async def read(
        _ctx: ServerRequestContext, params: types.ReadResourceRequestParams
    ) -> types.ReadResourceResult:
        parsed = parse_uri(str(params.uri))
        if parsed is None:
            raise rpc_error(-32602, "Invalid Beacon resource URI")
        kind, id = parsed
        try:
            data = await client.request(RESOURCE_OPERATIONS[kind][0], id)
        except GatewayError as error:
            if error.code == "NOT_FOUND":
                raise rpc_error(-32002, "Beacon resource not found") from None
            raise rpc_error(-32603, "Beacon resource temporarily unavailable") from None
        return types.ReadResourceResult(
            contents=[
                types.TextResourceContents(
                    uri=f"beacon://{kind}/{id}",
                    mime_type="application/json",
                    text=json.dumps(
                        data, separators=(",", ":"), sort_keys=True, ensure_ascii=False
                    ),
                )
            ]
        )

    server.add_request_handler("resources/list", types.PaginatedRequestParams, list_resources)
    server.add_request_handler("resources/templates/list", types.PaginatedRequestParams, templates)
    server.add_request_handler("resources/read", types.ReadResourceRequestParams, read)
