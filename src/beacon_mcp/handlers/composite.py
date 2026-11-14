"""Stateless analysis using at most three concurrent allowlisted GETs."""

import asyncio
from collections.abc import Coroutine
from typing import Any

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import invalid
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import Registry
from beacon_mcp.validation import identifier


def register(registry: Registry) -> None:
    registry.register("beacon_diagnose_node", diagnose_node)
    registry.register("beacon_compare_regions", compare_regions)


async def diagnose_node(client: BeaconClient, arguments: dict[str, Any]) -> dict[str, object]:
    node_id = identifier("uuid", arguments["id"])
    range_value = arguments.get("range", "24h")
    limit = arguments.get("neighborLimit", 10)
    node, activity, neighbors_page = await together(
        client.request(Operation.Node, node_id),
        client.request(Operation.NodeActivity, node_id, [("range", range_value)]),
        client.array_page(Operation.NodeNeighbors, node_id, max_items=limit + 1),
    )
    return {
        "node": node,
        "activity": activity,
        "neighbors": neighbors_page.items[:limit],
        "neighborsTruncated": not neighbors_page.complete or len(neighbors_page.items) > limit,
        "requestedRange": range_value,
        "note": "Read-only snapshot; Beacon may have incomplete geographic coverage.",
    }


async def compare_regions(client: BeaconClient, arguments: dict[str, Any]) -> dict[str, object]:
    iata_a = arguments["iataA"].strip().upper()
    iata_b = arguments["iataB"].strip().upper()
    if iata_a == iata_b:
        raise invalid("iataA and iataB must be different")
    overview_a, overview_b = await together(
        client.request(Operation.Overview, query=[("iatas", iata_a)]),
        client.request(Operation.Overview, query=[("iatas", iata_b)]),
    )
    return {
        "regionA": {"iata": iata_a, "overview": overview_a},
        "regionB": {"iata": iata_b, "overview": overview_b},
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
