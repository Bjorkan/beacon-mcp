"""Four original overview, time-series and observer-comparison tools."""

from functools import partial
from typing import Any

from beacon_mcp.client import BeaconClient, Operation, identifier, invalid
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import array_result, location, query

OPERATIONS = {
    "beacon_get_top_advertisers": Operation.TopAdvertisers,
    "beacon_get_network_overview": Operation.Overview,
    "beacon_get_network_series": Operation.Series,
    "beacon_compare_observers": Operation.ObserverComparison,
}


def register(registry: Registry) -> None:
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, args: dict[str, Any]) -> Any:
    args = dict(args)
    location(args, region_message="invalid region filter")
    if "region" in args and (not args["region"].strip() or len(args["region"].encode()) > 128):
        raise invalid("invalid region slug")
    if name == "beacon_compare_observers":
        for key in ("observerA", "observerB"):
            args[key] = identifier("uuid", args[key])
        if args["observerA"] == args["observerB"]:
            raise invalid("observerA and observerB must be different")
    limit = args.get("limit", 20)
    filters = query(args, ("limit",))
    op = OPERATIONS[name]
    if op == Operation.TopAdvertisers:
        return array_result(
            await client.array_page(
                op, query=[*filters, ("limit", str(limit))], max_items=limit + 1
            ),
            limit,
        )
    return await client.request(op, query=filters)
