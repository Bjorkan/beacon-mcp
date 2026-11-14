"""Shared public-GET execution for Beacon's additional endpoints."""

import re
from functools import partial
from typing import Any, Literal, cast

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import GatewayError, invalid
from beacon_mcp.json_stream import ArrayPage
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import CONTRACTS, Registry
from beacon_mcp.tool_support import array_result, location, page, query
from beacon_mcp.validation import duration, json_object

# These array endpoints ignore a query limit; the client still bounds reading.
type ResponseShape = Literal["array", "page", "object"]

NO_LIMIT: frozenset[Operation] = frozenset(
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


def register_operations(
    registry: Registry, operations: dict[str, tuple[Operation, ResponseShape]]
) -> None:
    name: str
    operation: Operation
    shape: ResponseShape
    for name, (operation, shape) in operations.items():
        registry.register(name, partial(execute_extra, name, operation, shape))


async def execute_extra(
    name: str,
    operation: Operation,
    shape: ResponseShape,
    client: BeaconClient,
    arguments: dict[str, Any],
) -> object:
    key: str
    validate_arguments(operation, arguments)
    resource_id: str | None = arguments.get("id")
    exclude: tuple[str, ...] = ("id", "pathKey", "limit")
    if operation == Operation.RouteObservations:
        resource_id = f"{arguments['iata'].upper()}:{arguments['pathKey']}"
        exclude += ("iata",)
    filters: list[tuple[str, str]] = query(arguments, exclude)
    limit: int = arguments.get("limit", 20)
    accepts_limit: bool = "limit" in CONTRACTS[name]["inputSchema"]["properties"]
    if accepts_limit and operation not in NO_LIMIT:
        filters.append(("limit", str(limit)))
    if shape == "array":
        response_page: ArrayPage = await client.array_page(
            operation, resource_id, filters, max_items=limit + 1
        )
        return array_result(response_page, limit)
    response: object = await client.request(operation, resource_id, filters)
    if shape == "page":
        result: dict[str, object] = dict(page(response, limit))
        document: dict[str, object] | None = json_object(response)
        if operation == Operation.ObserverDirectory and document is not None:
            for key in (
                "windowStart",
                "windowEnd",
                "coverage",
                "sort",
                "effectiveSort",
                "generatedAt",
                "maxObservationCount",
                "observerTypes",
            ):
                if key in document:
                    result[key] = document[key]
        return result
    if response is None:
        return {"available": False}
    document = json_object(response)
    if document is not None:
        return document
    if isinstance(response, list):
        return {"items": cast(list[object], response)}
    raise GatewayError("UNAVAILABLE", "Beacon returned an invalid result")


def validate_arguments(operation: Operation, arguments: dict[str, Any]) -> None:
    """Check endpoint constraints that the frozen JSON Schemas cannot express."""
    key: str
    value: Any
    location(arguments, region_message="mutually exclusive location filters")
    if operation == Operation.RouteObservations:
        range_conflicts: bool = "range" in arguments and any(
            key in arguments for key in ("since", "until", "pageCursor")
        )
        incomplete_window: bool = ("since" in arguments) != ("until" in arguments)
        if range_conflicts or incomplete_window:
            raise invalid(
                "range is exclusive with the explicit window and cursor; since and until must be paired"
            )
    if "range" in arguments:
        duration(arguments["range"])
    for key, value in arguments.items():
        if key in ("id", "pathKey", "range", "since", "until", "iata"):
            continue
        if not isinstance(value, str):
            continue
        blank: bool = not value.strip()
        too_long: bool = len(value.encode()) > 512
        has_control_characters: re.Match[str] | None = re.search(r"[\x00-\x1f\x7f-\x9f]", value)
        if blank or too_long or has_control_characters:
            raise invalid("invalid text filter")
