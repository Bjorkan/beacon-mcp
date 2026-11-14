"""Shared bounded transformations for public read-only tools."""

from collections.abc import Mapping
from typing import Any, NotRequired, TypedDict, cast

from beacon_mcp.errors import GatewayError, invalid
from beacon_mcp.upstream.json_stream import ArrayPage
from beacon_mcp.validation import json_object, timestamp


class Pagination(TypedDict):
    """Gateway pagination metadata; cursor values stay opaque upstream data."""

    hasMore: bool
    nextCursor: NotRequired[dict[str, object]]
    truncated: NotRequired[bool]


class Page(TypedDict):
    """A bounded tool result with optional upstream sort metadata."""

    items: list[object]
    pagination: Pagination
    sort: NotRequired[object]
    order: NotRequired[object]


def location(
    arguments: Mapping[str, object],
    *,
    region_message: str = "region and regionId are mutually exclusive",
) -> None:
    """Reject competing spellings of the same location filter."""
    if "region" in arguments and "regionId" in arguments:
        raise invalid(region_message)
    if "iata" in arguments and "iatas" in arguments:
        raise invalid("iata and iatas are mutually exclusive")


def query(arguments: dict[str, Any], exclude: tuple[str, ...] = ()) -> list[tuple[str, str]]:
    """Encode validated tool arguments using Beacon's query conventions."""
    key: str
    value: Any
    result: list[tuple[str, str]] = []
    for key, value in arguments.items():
        if key in exclude:
            continue
        if key in ("since", "until"):
            converted: str = str(timestamp(value))
        elif isinstance(value, list):
            elements: list[object] = cast(list[object], value)
            if key == "iatas":
                converted = ",".join(str(item).strip().upper() for item in elements)
            else:
                converted = ",".join(str(item) for item in elements)
        elif key in ("iata", "fromIata", "toIata"):
            converted = value.strip().upper()
        elif isinstance(value, bool):
            converted = str(value).lower()
        else:
            converted = str(value)
        result.append((key, converted))
    if "since" in arguments and "until" in arguments:
        since: int = timestamp(arguments["since"])
        until: int = timestamp(arguments["until"])
        if since >= until:
            raise invalid("since must be before until")
    return result


def page(value: object, limit: int, include_sort: bool = False) -> Page:
    """Limit a page while preserving the upstream cursor values exactly."""
    upstream_key: str
    argument_key: str
    document: dict[str, object] | None = json_object(value)
    raw_items: object = document.get("items") if document is not None else value
    if not isinstance(raw_items, list):
        raise GatewayError("UNAVAILABLE", "Beacon returned invalid page")
    items: list[object] = cast(list[object], raw_items)
    upstream_has_more: bool = document is not None and document.get("hasMore") is True
    has_more: bool = upstream_has_more or len(items) > limit
    pagination: Pagination = {"hasMore": has_more}
    if has_more and document is not None:
        cursor: dict[str, object] = {}
        for upstream_key, argument_key in (
            ("nextPageCursor", "pageCursor"),
            ("nextCursor", "cursor"),
        ):
            if upstream_key in document:
                cursor[argument_key] = document[upstream_key]
        if cursor:
            pagination["nextCursor"] = cursor
    result: Page = {"items": items[:limit], "pagination": pagination}
    if include_sort and document is not None:
        if "sort" in document:
            result["sort"] = document["sort"]
        if "order" in document:
            result["order"] = document["order"]
    return result


def array_result(value: ArrayPage, limit: int) -> Page:
    pagination: Pagination = {"hasMore": False}
    if not value.complete or len(value.items) > limit:
        pagination["truncated"] = True
    return {"items": value.items[:limit], "pagination": pagination}
