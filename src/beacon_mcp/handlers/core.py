"""Nine original server, node, observer and location tools."""

from functools import partial
from typing import Any, cast

from beacon_mcp.client import BeaconClient, GatewayError, Operation, invalid
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


async def execute(name: str, client: BeaconClient, args: dict[str, Any]) -> Any:
    location(args)
    if "type" in args and "typeName" in args:
        raise invalid("type and typeName are mutually exclusive")
    if args.get("cursor", 0) and (
        args.get("sort", "lastSeen") != "lastSeen" or args.get("order", "desc") != "desc"
    ):
        raise invalid("numeric cursor works only with default lastSeen descending order")
    for key in ("region", "scope", "broker", "name"):
        if key in args and (
            not args[key].strip() or len(args[key].encode()) > (256 if key == "name" else 128)
        ):
            raise invalid("invalid text filter")
    if (
        name == "beacon_search_observers"
        and "type" in args
        and (not args["type"].strip() or len(args["type"].encode()) > 128)
    ):
        raise invalid("invalid type filter")
    if name == "beacon_get_observer_activity" and "range" in args:
        if duration(args["range"]) > 2880 and args.get("interval", "15m") in ("5m", "15m"):
            raise invalid("range must be 48h or less for intervals under 1h")
    if "pageCursor" in args and len(args["pageCursor"].encode()) > 512:
        raise invalid("invalid pageCursor")
    op = OPERATIONS[name]
    limit = args.get("limit", 20)
    filters = query(args, ("id", "limit"))
    if op in (Operation.Iatas, Operation.Regions):
        return array_result(await client.array_page(op, query=filters, max_items=limit + 1), limit)
    if op == Operation.Scopes:
        source = await client.request(op, query=[(k, v) for k, v in filters if k != "cursor"])
        if not isinstance(source, list):
            raise GatewayError("UNAVAILABLE", "Beacon returned invalid scopes")
        offset = args.get("cursor", 0)
        items = cast(list[object], source)[offset : offset + limit]
        next_offset = offset + len(items)
        pagination: dict[str, Any] = (
            {"hasMore": True, "nextCursor": {"cursor": next_offset}}
            if next_offset < len(source)
            else {"hasMore": False}
        )
        return {"items": items, "pagination": pagination}
    if op in (Operation.Nodes, Operation.Observers):
        return page(
            await client.request(op, query=[*filters, ("limit", str(limit))]),
            limit,
            op == Operation.Nodes,
        )
    return await client.request(op, args.get("id"), filters)
