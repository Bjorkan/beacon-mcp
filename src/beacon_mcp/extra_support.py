"""Shared public-GET execution for Beacon's additional endpoints."""

import re
from functools import partial
from typing import Any, cast

from beacon_mcp.client import BeaconClient, GatewayError, Operation, invalid, json_object
from beacon_mcp.runtime import CONTRACTS, Registry
from beacon_mcp.tool_support import array_result, location, page, query
from beacon_mcp.validation import duration

NO_LIMIT = frozenset(
    (
        Operation.Brokers,
        Operation.NodeNeighbors,
        Operation.NodeTypes,
        Operation.ObservationStats,
        Operation.PayloadBreakdown,
        Operation.RadioPresets,
        Operation.ScopeStats,
    )
)


def register_operations(registry: Registry, operations: dict[str, tuple[Operation, str]]) -> None:
    for name, (op, shape) in operations.items():
        registry.register(name, partial(execute_extra, name, op, shape))


async def execute_extra(
    name: str, op: Operation, shape: str, client: BeaconClient, args: dict[str, Any]
) -> Any:
    location(args, region_message="mutually exclusive location filters")
    if op == Operation.RouteObservations and (
        "range" in args
        and any(key in args for key in ("since", "until", "pageCursor"))
        or ("since" in args) != ("until" in args)
    ):
        raise invalid(
            "range is exclusive with the explicit window and cursor; since and until must be paired"
        )
    if "range" in args:
        duration(args["range"])
    for key, value in args.items():
        if key not in ("id", "pathKey", "range", "since", "until", "iata") and isinstance(
            value, str
        ):
            if (
                not value.strip()
                or len(value.encode()) > 512
                or re.search(r"[\x00-\x1f\x7f-\x9f]", value)
            ):
                raise invalid("invalid text filter")
    id = args.get("id")
    exclude = ("id", "pathKey", "limit")
    if op == Operation.RouteObservations:
        id = f"{args['iata'].upper()}:{args['pathKey']}"
        exclude += ("iata",)
    filters = query(args, exclude)
    limit = args.get("limit", 20)
    if "limit" in CONTRACTS[name]["inputSchema"]["properties"] and op not in NO_LIMIT:
        filters.append(("limit", str(limit)))
    if shape == "array":
        return array_result(await client.array_page(op, id, filters, max_items=limit + 1), limit)
    response = await client.request(op, id, filters)
    if shape == "page":
        result = page(response, limit)
        if op == Operation.ObserverDirectory and isinstance(response, dict):
            result.update(
                {
                    key: response[key]
                    for key in (
                        "windowStart",
                        "windowEnd",
                        "coverage",
                        "sort",
                        "effectiveSort",
                        "generatedAt",
                        "maxObservationCount",
                        "observerTypes",
                    )
                    if key in response
                }
            )
        return result
    if response is None:
        return {"available": False}
    document = json_object(response)
    if document is not None:
        return document
    if isinstance(response, list):
        return {"items": cast(list[object], response)}
    raise GatewayError("UNAVAILABLE", "Beacon returned an invalid result")
