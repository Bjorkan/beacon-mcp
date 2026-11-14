"""Packet and channel reads with endpoint-specific cursors and payload aliases."""

from functools import partial
from typing import Any

from beacon_mcp.client import BeaconClient
from beacon_mcp.errors import invalid
from beacon_mcp.operations import Operation
from beacon_mcp.runtime import Registry
from beacon_mcp.tool_support import location, page, query

OPERATIONS = {
    "beacon_search_packets": Operation.Packets,
    "beacon_get_packet": Operation.Packet,
    "beacon_search_messages": Operation.Messages,
    "beacon_list_channels": Operation.Channels,
    "beacon_get_channel_messages": Operation.ChannelMessages,
}
PAYLOADS = {
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
    for name in OPERATIONS:
        registry.register(name, partial(execute, name))


async def execute(name: str, client: BeaconClient, arguments: dict[str, Any]) -> Any:
    arguments = dict(arguments)
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
    if any(
        a in arguments and b in arguments
        for a, b in (
            ("scope", "scopes"),
            ("payloadType", "payloadTypes"),
            ("routeType", "routeTypes"),
            ("channelId", "channelHash"),
        )
    ):
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
        alias = arguments.pop("payloadTypeName")
        if alias not in PAYLOADS:
            raise invalid("unknown payload type name")
        arguments["payloadTypes"] = PAYLOADS[alias]
    if "pageCursor" in arguments and len(arguments["pageCursor"].encode()) > (
        64 if name == "beacon_list_channels" else 512
    ):
        raise invalid("invalid pageCursor")
    operation = OPERATIONS[name]
    if operation == Operation.Packet:
        return await client.request(operation, arguments["hash"])
    id = str(arguments["channelId"]) if operation == Operation.ChannelMessages else None
    if name == "beacon_search_messages" and "channelId" in arguments:
        arguments["channelID"] = arguments.pop("channelId")
    limit = arguments.get("limit", 20)
    exclude = ("limit", "channelId") if id is not None else ("limit",)
    filters = query(arguments, exclude)
    result = page(await client.request(operation, id, [*filters, ("limit", str(limit))]), limit)
    cursor = result["pagination"].get("nextCursor", {})
    if operation == Operation.Channels and isinstance(cursor.get("pageCursor"), str):
        cursor.pop("cursor", None)
    if operation == Operation.Packets and "hashPrefix" in arguments:
        cursor.pop("pageCursor", None)
    if "nextCursor" in result["pagination"] and not cursor:
        del result["pagination"]["nextCursor"]
    return result
