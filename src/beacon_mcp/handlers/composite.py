"""Stateless analysis using at most three concurrent allowlisted GETs."""

import asyncio
from collections.abc import Coroutine
from typing import Any

from beacon_mcp.client import BeaconClient, Operation, identifier, invalid
from beacon_mcp.runtime import Registry


def register(registry: Registry) -> None:
    registry.register("beacon_diagnose_node", diagnose_node)
    registry.register("beacon_compare_regions", compare_regions)


async def diagnose_node(client: BeaconClient, args: dict[str, Any]) -> Any:
    id = identifier("uuid", args["id"])
    range_value, limit = args.get("range", "24h"), args.get("neighborLimit", 10)
    node, activity, result = await together(
        client.request(Operation.Node, id),
        client.request(Operation.NodeActivity, id, [("range", range_value)]),
        client.array_page(Operation.NodeNeighbors, id, max_items=limit + 1),
    )
    return {
        "node": node,
        "activity": activity,
        "neighbors": result.items[:limit],
        "neighborsTruncated": not result.complete or len(result.items) > limit,
        "requestedRange": range_value,
        "note": "Read-only snapshot; Beacon may have incomplete geographic coverage.",
    }


async def compare_regions(client: BeaconClient, args: dict[str, Any]) -> Any:
    a, b = args["iataA"].strip().upper(), args["iataB"].strip().upper()
    if a == b:
        raise invalid("iataA and iataB must be different")
    left, right = await together(
        client.request(Operation.Overview, query=[("iatas", a)]),
        client.request(Operation.Overview, query=[("iatas", b)]),
    )
    return {
        "regionA": {"iata": a, "overview": left},
        "regionB": {"iata": b, "overview": right},
        "note": "Independent snapshots, not synchronized measurements.",
    }


async def together(*calls: Coroutine[Any, Any, Any]) -> list[Any]:
    tasks = [asyncio.create_task(call) for call in calls]
    try:
        return list(await asyncio.gather(*tasks))
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
