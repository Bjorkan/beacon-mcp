"""Four original overview, time-series and observer-comparison tools."""

from functools import partial
from typing import Any

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import invalid
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import array_result, location, query
from beacon_mcp.validation import identifier

OPERATIONS: dict[str, Operation] = {
    "beacon_get_top_advertisers": Operation.TopAdvertisers,
    "beacon_get_network_overview": Operation.Overview,
    "beacon_get_network_series": Operation.Series,
    "beacon_compare_observers": Operation.ObserverComparison,
}


def register(registry: Registry) -> None:
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, arguments: dict[str, Any]) -> Any:
    arguments = dict(arguments)
    location(arguments, region_message="invalid region filter")
    if "region" in arguments and (
        not arguments["region"].strip() or len(arguments["region"].encode()) > 128
    ):
        raise invalid("invalid region slug")
    if name == "beacon_compare_observers":
        for key in ("observerA", "observerB"):
            arguments[key] = identifier("uuid", arguments[key])
        if arguments["observerA"] == arguments["observerB"]:
            raise invalid("observerA and observerB must be different")
    limit = arguments.get("limit", 20)
    filters = query(arguments, ("limit",))
    operation = OPERATIONS[name]
    if operation == Operation.TopAdvertisers:
        return array_result(
            await client.array_page(
                operation, query=[*filters, ("limit", str(limit))], max_items=limit + 1
            ),
            limit,
        )
    return await client.request(operation, query=filters)
