"""Packet and channel reads with endpoint-specific cursors and payload aliases."""

from functools import partial
from typing import Any

from beacon_mcp.client import BeaconClient, Operation, invalid
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


async def execute(name: str, client: BeaconClient, args: dict[str, Any]) -> Any:
    args = dict(args)
    location(args, region_message="invalid region filter")
    if name == "beacon_list_channels" and args.get("cursor", 0) > 0 and "pageCursor" in args:
        raise invalid("positive cursor and pageCursor cannot be combined")
    if "hashPrefix" in args and "pageCursor" in args:
        raise invalid("hashPrefix pages with cursor, not pageCursor")
    if "payloadTypeName" in args and ("payloadType" in args or "payloadTypes" in args):
        raise invalid("payloadTypeName cannot be combined with payloadType or payloadTypes")
    if any(
        a in args and b in args
        for a, b in (
            ("scope", "scopes"),
            ("payloadType", "payloadTypes"),
            ("routeType", "routeTypes"),
            ("channelId", "channelHash"),
        )
    ):
        raise invalid("mutually exclusive filters")
    if any(
        (not args[key].strip() or len(args[key].encode()) > 128)
        for key in ("region", "scope")
        if key in args
    ):
        raise invalid("invalid text filter")
    if "scopes" in args and any(
        not value.strip() or len(value.encode()) > 128 or "," in value for value in args["scopes"]
    ):
        raise invalid("invalid scopes")
    if "payloadTypeName" in args:
        alias = args.pop("payloadTypeName")
        if alias not in PAYLOADS:
            raise invalid("unknown payload type name")
        args["payloadTypes"] = PAYLOADS[alias]
    if "pageCursor" in args and len(args["pageCursor"].encode()) > (
        64 if name == "beacon_list_channels" else 512
    ):
        raise invalid("invalid pageCursor")
    op = OPERATIONS[name]
    if op == Operation.Packet:
        return await client.request(op, args["hash"])
    id = str(args["channelId"]) if op == Operation.ChannelMessages else None
    if name == "beacon_search_messages" and "channelId" in args:
        args["channelID"] = args.pop("channelId")
    limit = args.get("limit", 20)
    exclude = ("limit", "channelId") if id is not None else ("limit",)
    filters = query(args, exclude)
    result = page(await client.request(op, id, [*filters, ("limit", str(limit))]), limit)
    cursor = result["pagination"].get("nextCursor", {})
    if op == Operation.Channels and isinstance(cursor.get("pageCursor"), str):
        cursor.pop("cursor", None)
    if op == Operation.Packets and "hashPrefix" in args:
        cursor.pop("pageCursor", None)
    if "nextCursor" in result["pagination"] and not cursor:
        del result["pagination"]["nextCursor"]
    return result
