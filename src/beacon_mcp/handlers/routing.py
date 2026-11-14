"""Saved routes, route segments, cross-IATA evidence and traces."""

from functools import partial
from typing import Any, cast

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import GatewayError, invalid
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import Pagination, array_result, location, query
from beacon_mcp.validation import json_object

OPERATIONS: dict[str, Operation] = {
    "beacon_list_routes": Operation.Routes,
    "beacon_search_routes": Operation.RouteSearch,
    "beacon_find_cross_iata_routes": Operation.CrossRoutes,
    "beacon_search_traces": Operation.Traces,
    "beacon_get_trace": Operation.Trace,
}


def register(registry: Registry) -> None:
    name: str
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, arguments: dict[str, Any]) -> object:
    key: str
    arguments = dict(arguments)
    validate_arguments(name, arguments)
    for key in ("from", "to", "fromHash", "toHash"):
        if key in arguments:
            arguments[key] = arguments[key].lower()
    operation: Operation = OPERATIONS[name]
    if operation == Operation.Trace:
        return await client.request(operation, arguments["tag"])
    limit: int = arguments.get("limit", 20)
    filters: list[tuple[str, str]] = query(arguments, ("limit",))
    if operation in (Operation.RouteSearch, Operation.CrossRoutes):
        return array_result(
            await client.array_page(operation, query=filters, max_items=limit + 1), limit
        )
    response: object = await client.request(operation, query=[*filters, ("limit", str(limit))])
    if not isinstance(response, list):
        raise GatewayError("UNAVAILABLE", "Beacon returned invalid route or trace list")
    items: list[object] = cast(list[object], response)[:limit]
    pagination: Pagination = {"hasMore": False}
    last: dict[str, object] | None = json_object(items[-1]) if len(items) == limit else None
    if last is not None:
        if operation == Operation.Routes:
            cursor: dict[str, object | None] = {
                "cursor": last.get("lastSeen"),
                "cursorId": last.get("id"),
            }
        else:
            cursor = {"cursor": last.get("lastHeardAt"), "cursorTag": last.get("traceTag")}
        if isinstance(cursor["cursor"], (int, float)) and all(
            value is not None for value in cursor.values()
        ):
            pagination = {"hasMore": True, "nextCursor": cursor}
    return {"items": items, "pagination": pagination}


def validate_arguments(name: str, arguments: dict[str, Any]) -> None:
    location(arguments, region_message="mutually exclusive or incomplete filters")
    if ("fromIata" in arguments) != ("toIata" in arguments):
        raise invalid("fromIata and toIata must be given together")
    conflicting_locations: bool = "fromIata" in arguments and "iatas" in arguments
    missing_cursor_time: bool = "cursor" not in arguments and any(
        key in arguments for key in ("cursorId", "cursorTag")
    )
    if conflicting_locations or missing_cursor_time:
        raise invalid("mutually exclusive or incomplete filters")
    if any(
        (not arguments[key].strip() or len(arguments[key].encode()) > 128)
        for key in ("region", "scope")
        if key in arguments
    ):
        raise invalid("invalid text filter")
    if (
        name == "beacon_find_cross_iata_routes"
        and "iatas" in arguments
        and len({value.upper() for value in arguments["iatas"]}) < 2
    ):
        raise invalid("cross-IATA search requires two distinct codes")
