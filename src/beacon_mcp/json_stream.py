"""Read bounded JSON responses and stop large arrays at the requested limit."""

import json
from collections.abc import Generator, Iterator
from dataclasses import dataclass
from typing import cast

import httpx
import ijson

from beacon_mcp.errors import GatewayError
from beacon_mcp.validation import json_object

MAX_BYTES: int = 5_242_880


@dataclass
class ArrayPage:
    """A bounded prefix; complete is true only after consuming the entire array."""

    items: list[object]
    complete: bool


async def read_bounded(response: httpx.Response) -> bytes:
    chunk: bytes
    length: str = response.headers.get("content-length", "")
    if length.isdigit() and int(length) > MAX_BYTES:
        raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
    data: bytearray = bytearray()
    async for chunk in response.aiter_bytes(chunk_size=8192):
        if len(data) + len(chunk) > MAX_BYTES:
            raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
        data.extend(chunk)
    return bytes(data)


async def read_array(response: httpx.Response, max_items: int) -> ArrayPage:
    chunk: bytes
    items: list[object] = []
    pending: list[object] = ijson.sendable_list()
    parser: Generator[None, bytes] = ijson.items_coro(pending, "item", use_float=True)
    mode: str | None = None
    consumed: int = 0
    envelope: bytearray = bytearray()
    guard: JsonDepthGuard = JsonDepthGuard()
    try:
        async for chunk in response.aiter_bytes(chunk_size=8192):
            consumed += len(chunk)
            if consumed > MAX_BYTES:
                if items and mode == "array":
                    return ArrayPage(items, False)
                raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
            if mode is None:
                stripped: bytes = chunk.lstrip()
                if not stripped:
                    continue
                mode = "array" if stripped.startswith(b"[") else "envelope"
            if mode == "envelope":
                envelope.extend(chunk)
                continue
            guard.feed(chunk)
            parser.send(chunk)
            items.extend(pending[: max_items - len(items)])
            del pending[:]
            if len(items) >= max_items:
                return ArrayPage(items, False)
        if mode == "envelope":
            value: object = decode_json(envelope)
            document: dict[str, object] | None = json_object(value)
            source: object = document.get("items") if document is not None else value
            if not isinstance(source, list):
                raise ValueError
            elements: list[object] = cast(list[object], source)
            return ArrayPage(elements[:max_items], len(elements) <= max_items)
        try:
            parser.close()
        except ijson.IncompleteJSONError:
            if items:
                return ArrayPage(items, False)
            raise
        if mode != "array":
            raise ValueError
        return ArrayPage(items, True)
    except (ValueError, ijson.JSONError, RecursionError):
        raise GatewayError("UNAVAILABLE", "Beacon returned an invalid JSON array") from None


class JsonDepthGuard:
    """Reject excessive nesting before the streaming parser allocates a tree."""

    def __init__(self) -> None:
        self.depth: int = 0
        self.quoted: bool = False
        self.escaped: bool = False

    def feed(self, chunk: bytes) -> None:
        byte: int
        for byte in chunk:
            if self.quoted:
                if self.escaped:
                    self.escaped = False
                elif byte == 92:
                    self.escaped = True
                elif byte == 34:
                    self.quoted = False
            elif byte == 34:
                self.quoted = True
            elif byte in (91, 123):
                self.depth += 1
                if self.depth > 128:
                    raise ValueError("excessive JSON nesting")
            elif byte in (93, 125):
                self.depth -= 1


def decode_json(data: bytes | bytearray) -> object:
    iterator: Iterator[object]
    depth: int

    def nonfinite(_value: str) -> None:
        raise ValueError("invalid JSON number")

    value: object = json.loads(data, parse_constant=nonfinite)
    pending: list[tuple[Iterator[object], int]] = [(iter([value]), 0)]
    while pending:
        iterator, depth = pending[-1]
        try:
            item: object = next(iterator)
        except StopIteration:
            pending.pop()
            continue
        if isinstance(item, (dict, list)):
            if depth >= 128:
                raise ValueError("excessive JSON nesting")
            nested: Iterator[object]
            if isinstance(item, dict):
                nested = iter(cast(dict[str, object], item).values())
            else:
                nested = iter(cast(list[object], item))
            pending.append((nested, depth + 1))
    return value
