"""Stateless analysis using at most three concurrent allowlisted GETs."""

import asyncio
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
    node_task = asyncio.create_task(client.request(Operation.Node, node_id))
    activity_task = asyncio.create_task(
        client.request(Operation.NodeActivity, node_id, [("range", range_value)])
    )
    neighbors_task = asyncio.create_task(
        client.array_page(Operation.NodeNeighbors, node_id, max_items=limit + 1)
    )
    await wait_for_tasks(node_task, activity_task, neighbors_task)
    neighbors_page = neighbors_task.result()
    return {
        "node": node_task.result(),
        "activity": activity_task.result(),
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
    overview_a = asyncio.create_task(client.request(Operation.Overview, query=[("iatas", iata_a)]))
    overview_b = asyncio.create_task(client.request(Operation.Overview, query=[("iatas", iata_b)]))
    await wait_for_tasks(overview_a, overview_b)
    return {
        "regionA": {"iata": iata_a, "overview": overview_a.result()},
        "regionB": {"iata": iata_b, "overview": overview_b.result()},
        "note": "Independent snapshots, not synchronized measurements.",
    }


async def wait_for_tasks(*tasks: asyncio.Task[object]) -> None:
    """Wait for related reads and always cancel and await unfinished siblings.

    Callers keep their individual tasks so each result retains its own type.
    """
    try:
        await asyncio.gather(*tasks)
    finally:
        for task in tasks:
            if not task.done():
                task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
