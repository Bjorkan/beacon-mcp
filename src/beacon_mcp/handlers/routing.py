"""Saved routes, route segments, cross-IATA evidence and traces."""

from functools import partial
from typing import Any

from beacon_mcp.client import BeaconClient, GatewayError, Operation, invalid
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import array_result, location, query

OPERATIONS = {
    "beacon_list_routes": Operation.Routes,
    "beacon_search_routes": Operation.RouteSearch,
    "beacon_find_cross_iata_routes": Operation.CrossRoutes,
    "beacon_search_traces": Operation.Traces,
    "beacon_get_trace": Operation.Trace,
}


def register(registry: Registry) -> None:
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, args: dict[str, Any]) -> Any:
    args = dict(args)
    location(args, region_message="mutually exclusive or incomplete filters")
    if ("fromIata" in args) != ("toIata" in args):
        raise invalid("fromIata and toIata must be given together")
    if ("fromIata" in args and "iatas" in args) or any(
        key in args and "cursor" not in args for key in ("cursorId", "cursorTag")
    ):
        raise invalid("mutually exclusive or incomplete filters")
    if any(
        (not args[key].strip() or len(args[key].encode()) > 128)
        for key in ("region", "scope")
        if key in args
    ):
        raise invalid("invalid text filter")
    if (
        name == "beacon_find_cross_iata_routes"
        and "iatas" in args
        and len({value.upper() for value in args["iatas"]}) < 2
    ):
        raise invalid("cross-IATA search requires two distinct codes")
    for key in ("from", "to", "fromHash", "toHash"):
        if key in args:
            args[key] = args[key].lower()
    op = OPERATIONS[name]
    if op == Operation.Trace:
        return await client.request(op, args["tag"])
    limit = args.get("limit", 20)
    filters = query(args, ("limit",))
    if op in (Operation.RouteSearch, Operation.CrossRoutes):
        return array_result(await client.array_page(op, query=filters, max_items=limit + 1), limit)
    response = await client.request(op, query=[*filters, ("limit", str(limit))])
    if not isinstance(response, list):
        raise GatewayError("UNAVAILABLE", "Beacon returned invalid route or trace list")
    items = response[:limit]
    pagination: dict[str, Any] = {"hasMore": False}
    if len(items) == limit and isinstance(items[-1], dict):
        last = items[-1]
        cursor = (
            {"cursor": last.get("lastSeen"), "cursorId": last.get("id")}
            if op == Operation.Routes
            else {"cursor": last.get("lastHeardAt"), "cursorTag": last.get("traceTag")}
        )
        if isinstance(cursor["cursor"], (int, float)) and all(
            value is not None for value in cursor.values()
        ):
            pagination = {"hasMore": True, "nextCursor": cursor}
    return {"items": items, "pagination": pagination}
