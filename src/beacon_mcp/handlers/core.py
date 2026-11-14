"""Nine original server, node, observer and location tools."""

from functools import partial
from typing import Any, cast

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import GatewayError, invalid
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import array_result, location, page, query
from beacon_mcp.validation import duration

OPERATIONS = {
    "beacon_get_server_info": Operation.ServerInfo,
    "beacon_list_iatas": Operation.Iatas,
    "beacon_list_regions": Operation.Regions,
    "beacon_list_scopes": Operation.Scopes,
    "beacon_search_nodes": Operation.Nodes,
    "beacon_get_node": Operation.Node,
    "beacon_search_observers": Operation.Observers,
    "beacon_get_observer": Operation.Observer,
    "beacon_get_observer_activity": Operation.ObserverActivity,
}


def register(registry: Registry) -> None:
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, arguments: dict[str, Any]) -> Any:
    validate_arguments(name, arguments)
    operation = OPERATIONS[name]
    limit = arguments.get("limit", 20)
    filters = query(arguments, ("id", "limit"))
    if operation in (Operation.Iatas, Operation.Regions):
        return array_result(
            await client.array_page(operation, query=filters, max_items=limit + 1), limit
        )
    if operation == Operation.Scopes:
        source = await client.request(
            operation, query=[(k, v) for k, v in filters if k != "cursor"]
        )
        return scope_page(source, arguments.get("cursor", 0), limit)
    if operation in (Operation.Nodes, Operation.Observers):
        return page(
            await client.request(operation, query=[*filters, ("limit", str(limit))]),
            limit,
            operation == Operation.Nodes,
        )
    return await client.request(operation, arguments.get("id"), filters)


def validate_arguments(name: str, arguments: dict[str, Any]) -> None:
    location(arguments)
    if "type" in arguments and "typeName" in arguments:
        raise invalid("type and typeName are mutually exclusive")
    if arguments.get("cursor", 0) and (
        arguments.get("sort", "lastSeen") != "lastSeen" or arguments.get("order", "desc") != "desc"
    ):
        raise invalid("numeric cursor works only with default lastSeen descending order")
    for key in ("region", "scope", "broker", "name"):
        if key in arguments and (
            not arguments[key].strip()
            or len(arguments[key].encode()) > (256 if key == "name" else 128)
        ):
            raise invalid("invalid text filter")
    if (
        name == "beacon_search_observers"
        and "type" in arguments
        and (not arguments["type"].strip() or len(arguments["type"].encode()) > 128)
    ):
        raise invalid("invalid type filter")
    if name == "beacon_get_observer_activity" and "range" in arguments:
        if duration(arguments["range"]) > 2880 and arguments.get("interval", "15m") in (
            "5m",
            "15m",
        ):
            raise invalid("range must be 48h or less for intervals under 1h")
    if "pageCursor" in arguments and len(arguments["pageCursor"].encode()) > 512:
        raise invalid("invalid pageCursor")


def scope_page(source: object, offset: int, limit: int) -> dict[str, Any]:
    """Beacon returns all scope names; expose a bounded local offset page."""
    if not isinstance(source, list):
        raise GatewayError("UNAVAILABLE", "Beacon returned invalid scopes")

    scopes = cast(list[object], source)
    items = scopes[offset : offset + limit]
    next_offset = offset + len(items)
    pagination: dict[str, Any] = {"hasMore": False}
    if next_offset < len(scopes):
        pagination = {"hasMore": True, "nextCursor": {"cursor": next_offset}}
    return {"items": items, "pagination": pagination}
