"""Packet and channel reads with endpoint-specific cursors and payload aliases."""

from functools import partial
from typing import Any

from beacon_mcp.errors import invalid
from beacon_mcp.handlers.tool_support import Page, location, page, query
from beacon_mcp.mcp.runtime import Registry
from beacon_mcp.upstream.client import BeaconClient
from beacon_mcp.upstream.operations import Operation

OPERATIONS: dict[str, Operation] = {
    "beacon_search_packets": Operation.Packets,
    "beacon_get_packet": Operation.Packet,
    "beacon_search_messages": Operation.Messages,
    "beacon_list_channels": Operation.Channels,
    "beacon_get_channel_messages": Operation.ChannelMessages,
}
PAYLOADS: dict[str, list[int]] = {
    "request": [0],
    "response": [1],
    "text_message": [2],
    "txt_msg": [2],
    "acknowledgement": [3],
    "advert": [4],
    "group_text": [5],
    "grp_txt": [5],
    "group_data": [6],
    "anonymous_request": [7],
    "anon_req": [7],
    "path": [8],
    "trace": [9],
    "multipart": [10],
    "control": [11],
    "reserved": [12, 13, 14],
    "raw_custom": [15],
}


def register(registry: Registry) -> None:
    name: str
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, arguments: dict[str, Any]) -> object:
    arguments = dict(arguments)
    prepare_arguments(name, arguments)
    operation: Operation = OPERATIONS[name]
    if operation == Operation.Packet:
        return await client.request(operation, arguments["hash"])
    channel_id: str | None = None
    if operation == Operation.ChannelMessages:
        channel_id = str(arguments["channelId"])
    if name == "beacon_search_messages" and "channelId" in arguments:
        arguments["channelID"] = arguments.pop("channelId")
    limit: int = arguments.get("limit", 20)
    exclude: tuple[str, ...] = ("limit", "channelId") if channel_id is not None else ("limit",)
    filters: list[tuple[str, str]] = query(arguments, exclude)
    response: object = await client.request(
        operation, channel_id, [*filters, ("limit", str(limit))]
    )
    result: Page = page(response, limit)
    cursor: dict[str, object] = result["pagination"].get("nextCursor", {})
    if operation == Operation.Channels and isinstance(cursor.get("pageCursor"), str):
        cursor.pop("cursor", None)
    if operation == Operation.Packets and "hashPrefix" in arguments:
        cursor.pop("pageCursor", None)
    if "nextCursor" in result["pagination"] and not cursor:
        del result["pagination"]["nextCursor"]
    return result


def prepare_arguments(name: str, arguments: dict[str, Any]) -> None:
    first: str
    second: str
    location(arguments, region_message="invalid region filter")
    if (
        name == "beacon_list_channels"
        and arguments.get("cursor", 0) > 0
        and "pageCursor" in arguments
    ):
        raise invalid("positive cursor and pageCursor cannot be combined")
    if "hashPrefix" in arguments and "pageCursor" in arguments:
        raise invalid("hashPrefix pages with cursor, not pageCursor")
    if "payloadTypeName" in arguments and (
        "payloadType" in arguments or "payloadTypes" in arguments
    ):
        raise invalid("payloadTypeName cannot be combined with payloadType or payloadTypes")
    for first, second in (
        ("scope", "scopes"),
        ("payloadType", "payloadTypes"),
        ("routeType", "routeTypes"),
        ("channelId", "channelHash"),
    ):
        if first in arguments and second in arguments:
            raise invalid("mutually exclusive filters")
    if any(
        (not arguments[key].strip() or len(arguments[key].encode()) > 128)
        for key in ("region", "scope")
        if key in arguments
    ):
        raise invalid("invalid text filter")
    if "scopes" in arguments and any(
        not value.strip() or len(value.encode()) > 128 or "," in value
        for value in arguments["scopes"]
    ):
        raise invalid("invalid scopes")
    if "payloadTypeName" in arguments:
        alias: str = arguments.pop("payloadTypeName")
        if alias not in PAYLOADS:
            raise invalid("unknown payload type name")
        arguments["payloadTypes"] = PAYLOADS[alias]
    if "pageCursor" in arguments and len(arguments["pageCursor"].encode()) > (
        64 if name == "beacon_list_channels" else 512
    ):
        raise invalid("invalid pageCursor")
