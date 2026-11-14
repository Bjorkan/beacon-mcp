"""Shared bounded transformations for public read-only tools."""

from typing import Any

from beacon_mcp.client import ArrayPage, GatewayError, invalid
from beacon_mcp.validation import timestamp


def location(
    args: dict[str, Any], *, region_message: str = "region and regionId are mutually exclusive"
) -> None:
    if "region" in args and "regionId" in args:
        raise invalid(region_message)
    if "iata" in args and "iatas" in args:
        raise invalid("iata and iatas are mutually exclusive")


def query(args: dict[str, Any], exclude: tuple[str, ...] = ()) -> list[tuple[str, str]]:
    result = []
    for key, value in args.items():
        if key in exclude:
            continue
        if key in ("since", "until"):
            converted = str(timestamp(value))
        elif isinstance(value, list):
            converted = ",".join(
                str(item).strip().upper() if key == "iatas" else str(item) for item in value
            )
        elif key in ("iata", "fromIata", "toIata"):
            converted = value.strip().upper()
        elif isinstance(value, bool):
            converted = str(value).lower()
        else:
            converted = str(value)
        result.append((key, converted))
    if "since" in args and "until" in args and timestamp(args["since"]) >= timestamp(args["until"]):
        raise invalid("since must be before until")
    return result


def page(value: Any, limit: int, include_sort: bool = False) -> dict[str, Any]:
    source = value.get("items") if isinstance(value, dict) else value
    if not isinstance(source, list):
        raise GatewayError("UNAVAILABLE", "Beacon returned invalid page")
    has_more = (value.get("hasMore") is True if isinstance(value, dict) else False) or len(
        source
    ) > limit
    pagination: dict[str, Any] = {"hasMore": has_more}
    if has_more and isinstance(value, dict):
        cursor = {
            new: value[old]
            for old, new in (("nextPageCursor", "pageCursor"), ("nextCursor", "cursor"))
            if old in value
        }
        if cursor:
            pagination["nextCursor"] = cursor
    result = {"items": source[:limit], "pagination": pagination}
    if include_sort and isinstance(value, dict):
        result.update({key: value[key] for key in ("sort", "order") if key in value})
    return result


def array_result(value: ArrayPage, limit: int) -> dict[str, Any]:
    pagination = {"hasMore": False}
    if not value.complete or len(value.items) > limit:
        pagination["truncated"] = True
    return {"items": value.items[:limit], "pagination": pagination}
