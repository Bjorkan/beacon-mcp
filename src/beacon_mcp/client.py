"""Allowlisted public GET operations, bounded streaming and classified failures."""

import asyncio
import json
import re
import secrets
import uuid
from dataclasses import dataclass
from enum import Enum
from http import HTTPStatus
from http.cookiejar import CookieJar, DefaultCookiePolicy
from typing import Any
from urllib.parse import quote, urlencode

import httpx
import ijson

from beacon_mcp.config import Config

MAX_BYTES = 5_242_880
MAX_URL = 8_192


class Operation(Enum):
    ServerInfo = ("info", "", False)
    WsTicket = ("ws-ticket", "", False)
    Iatas = ("iatas", "", False)
    Regions = ("regions", "", False)
    Scopes = ("scopes", "", False)
    Nodes = ("nodes", "", False)
    Node = ("nodes", "uuid", False)
    Observers = ("observers", "", False)
    Observer = ("observers", "uuid", False)
    ObserverActivity = ("observers", "uuid/activity", False)
    Packets = ("packets", "", False)
    Packet = ("packets", "hash", False)
    Messages = ("messages", "", False)
    Channels = ("channels", "", False)
    ChannelMessages = ("channels", "number/messages", False)
    Routes = ("routes", "", False)
    RouteSearch = ("routes/search", "", False)
    CrossRoutes = ("routes/cross", "", True)
    Traces = ("traces", "", False)
    Trace = ("traces", "tag", False)
    TopAdvertisers = ("stats/top-advertisers", "", False)
    Overview = ("stats/overview", "", True)
    Series = ("stats/series", "", True)
    ObserverComparison = ("stats/observer-comparison", "", True)
    Brokers = ("brokers", "", False)
    ChannelDetail = ("channels", "number", False)
    IataDetail = ("iatas", "iata", False)
    IataBorder = ("iatas", "iata/border", False)
    MessagesBackfill = ("messages/backfill", "", False)
    NodeActivity = ("nodes", "uuid/activity", True)
    NodeNeighbors = ("nodes", "uuid/neighbors", False)
    NodeObservations = ("nodes", "uuid/observations", False)
    ObserverAdverts = ("observers", "uuid/adverts", False)
    ObserverTelemetry = ("observers", "uuid/telemetry", False)
    ObserverDirectory = ("observers/directory", "", True)
    PacketsBackfill = ("packets/backfill", "", False)
    RegionDetail = ("regions", "number", False)
    RouteObservations = ("routes", "route", True)
    ScopeDetail = ("scopes", "scope", False)
    ClockDrift = ("stats/clock-drift", "", True)
    NodeTypes = ("stats/node-types", "", True)
    ObservationStats = ("stats/observations", "", True)
    PathStats = ("stats/paths", "", True)
    PayloadBreakdown = ("stats/payload-breakdown", "", True)
    RadioPresets = ("stats/radio-presets", "", True)
    ScopeStats = ("stats/scopes", "", True)
    SignalStats = ("stats/signal", "", True)
    TopNodes = ("stats/top-nodes", "", True)
    TopObservers = ("stats/top-observers", "", True)
    TopTalkers = ("stats/top-talkers", "", True)


ARRAY_OPERATIONS = frozenset(
    Operation[n]
    for n in (
        "Iatas",
        "Regions",
        "RouteSearch",
        "CrossRoutes",
        "TopAdvertisers",
        "Brokers",
        "MessagesBackfill",
        "NodeNeighbors",
        "PacketsBackfill",
        "ClockDrift",
        "NodeTypes",
        "ObservationStats",
        "PayloadBreakdown",
        "RadioPresets",
        "ScopeStats",
        "TopNodes",
        "TopObservers",
        "TopTalkers",
    )
)


class GatewayError(Exception):
    def __init__(
        self,
        code: str,
        message: str,
        *,
        status: int | None = None,
        upstream_code: str | None = None,
        retry_after: str | None = None,
    ):
        super().__init__(message)
        self.code, self.message, self.status = code, message, status
        self.upstream_code, self.retry_after = upstream_code, retry_after


def invalid(message: str) -> GatewayError:
    return GatewayError("INVALID_ARGUMENT", message)


def identifier(kind: str, value: str) -> str:
    if kind == "uuid":
        try:
            parsed = uuid.UUID(value)
            if (
                len(value) == 36
                and parsed.variant == uuid.RFC_4122
                and 1 <= (parsed.version or 0) <= 8
            ):
                return str(parsed)
        except (ValueError, AttributeError):
            pass
        raise invalid("invalid UUID")
    if kind in ("hash", "tag"):
        if re.fullmatch(rf"[0-9a-fA-F]{{{16 if kind == 'hash' else 8}}}", value):
            return value.lower()
    elif kind == "number":
        if re.fullmatch(r"\+?[0-9]+", value) and 0 < int(value) <= 2_147_483_647:
            return value
    elif kind == "iata":
        if re.fullmatch(r"[A-Za-z0-9]{3}", value):
            return value.upper()
    elif kind == "scope":
        if (
            value.strip()
            and len(value.encode()) <= 128
            and value not in (".", "..")
            and not re.search(r"[/\\%\x00-\x1f\x7f-\x9f]", value)
        ):
            return value
    raise invalid("invalid identifier")


def classify(response: httpx.Response, body: Any) -> GatewayError:
    error = body.get("error", {}) if isinstance(body, dict) else {}
    error = error if isinstance(error, dict) else {}
    upstream = error.get("code") if isinstance(error.get("code"), str) else None
    status = response.status_code
    code = {400: "INVALID_ARGUMENT", 404: "NOT_FOUND", 429: "RESOURCE_EXHAUSTED"}.get(
        status, "UNAVAILABLE"
    )
    if status == 503 and upstream == "deadline_exceeded":
        code = "DEADLINE_EXCEEDED"
    try:
        reason = HTTPStatus(status).phrase
    except ValueError:
        reason = "<unknown status code>"
    message = f"Beacon request failed with HTTP {status} {reason}"
    if status < 500 and isinstance(error.get("message"), str):
        message = error["message"]
    return GatewayError(
        code,
        message,
        status=status,
        upstream_code=upstream,
        retry_after=response.headers.get("retry-after"),
    )


@dataclass
class ArrayPage:
    items: list[Any]
    complete: bool


class NoCookies(DefaultCookiePolicy):
    """Public reads never retain or resend upstream session/tracking cookies."""

    def set_ok(self, cookie: Any, request: Any) -> bool:
        return False


class BeaconClient:
    def __init__(self, config: Config, *, transport: httpx.AsyncBaseTransport | None = None):
        self.config = config
        self.http = httpx.AsyncClient(
            cookies=CookieJar(policy=NoCookies()),
            follow_redirects=False,
            trust_env=False,
            transport=transport,
            headers={"User-Agent": "beacon-mcp/2.2.0", "Accept-Encoding": "identity"},
            limits=httpx.Limits(max_connections=100, max_keepalive_connections=20),
        )

    async def aclose(self) -> None:
        await self.http.aclose()

    def url(self, operation: Operation, id: str | None, query: list[tuple[str, str]]) -> str:
        if not isinstance(operation, Operation):
            raise invalid("invalid public operation")
        path, kind, _ = operation.value
        url = self.config.base_url + "api/v1/" + path
        if kind:
            if id is None:
                raise invalid("missing identifier")
            if kind == "route":
                parts = id.split(":")
                if (
                    len(parts) != 2
                    or not re.fullmatch(r"[A-Za-z0-9]{3}", parts[0])
                    or not re.fullmatch(r"[0-9a-fA-F]{32}", parts[1])
                ):
                    raise invalid("invalid route identifier")
                url += f"/{parts[0].upper()}/{parts[1].lower()}/observations"
            else:
                key, _, suffix = kind.partition("/")
                url += "/" + quote(identifier(key, id), safe="")
                if suffix:
                    url += "/" + suffix
        elif id is not None:
            raise invalid("unexpected identifier")
        if query:
            url += "?" + urlencode(query)
        if len(url.encode()) > MAX_URL:
            raise invalid("Beacon request filters are too large")
        return url

    async def request(
        self,
        operation: Operation,
        id: str | None = None,
        query: list[tuple[str, str]] | None = None,
    ) -> Any:
        return await self._get(operation, id, query or [], None)

    async def array_page(
        self,
        operation: Operation,
        id: str | None = None,
        query: list[tuple[str, str]] | None = None,
        max_items: int = 21,
    ) -> ArrayPage:
        if operation not in ARRAY_OPERATIONS or not 1 <= max_items <= 51:
            raise invalid("invalid array page request")
        return await self._get(operation, id, query or [], max_items)

    async def _get(
        self,
        operation: Operation,
        id: str | None,
        query: list[tuple[str, str]],
        max_items: int | None,
    ) -> Any:
        url = self.url(operation, id, query)
        timeout = (
            self.config.stats_timeout_ms if operation.value[2] else self.config.timeout_ms
        ) / 1000
        for attempt in range(2):
            try:
                # HTTPX's inactivity timeout alone doesn't bound a slow-drip body.
                async with asyncio.timeout(timeout):
                    async with self.http.stream("GET", url, timeout=timeout) as response:
                        if response.status_code == 204 and max_items is None:
                            return None
                        # HTTPX otherwise inflates a chunk before applying our byte cap.
                        # Match the reference client's uncompressed public GET transport.
                        encoded = response.headers.get("content-encoding", "").strip().lower()
                        if encoded not in ("", "identity"):
                            if response.is_success:
                                raise GatewayError(
                                    "UNAVAILABLE", "Beacon returned unsupported content encoding"
                                )
                            body = None
                        else:
                            if response.is_success and max_items is not None:
                                return await read_array(response, max_items)
                            data = await read_bounded(response)
                            try:
                                body = decode_json(data)
                            except (ValueError, RecursionError):
                                if response.is_success:
                                    raise GatewayError(
                                        "UNAVAILABLE", "Beacon returned invalid JSON"
                                    ) from None
                                body = None
                        if response.is_success:
                            return body
                        if (
                            attempt == 0
                            and response.status_code in (502, 503, 504)
                            and "retry-after" not in response.headers
                        ):
                            pass
                        else:
                            raise classify(response, body)
            except (TimeoutError, httpx.TimeoutException):
                raise GatewayError("DEADLINE_EXCEEDED", "Beacon request timed out") from None
            except httpx.TransportError:
                if attempt:
                    raise GatewayError("UNAVAILABLE", "Beacon is temporarily unavailable") from None
            await asyncio.sleep((25 + secrets.randbelow(50)) / 1000)
        raise GatewayError("UNAVAILABLE", "Beacon retry attempts exhausted")


async def read_bounded(response: httpx.Response) -> bytes:
    length = response.headers.get("content-length", "")
    if length.isdigit() and int(length) > MAX_BYTES:
        raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
    data = bytearray()
    async for chunk in response.aiter_bytes(chunk_size=8192):
        if len(data) + len(chunk) > MAX_BYTES:
            raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
        data.extend(chunk)
    return bytes(data)


async def read_array(response: httpx.Response, max_items: int) -> ArrayPage:
    items: list[Any] = []
    pending = ijson.sendable_list()
    parser = ijson.items_coro(pending, "item", use_float=True)
    mode: str | None = None
    consumed = 0
    envelope = bytearray()
    guard = JsonDepthGuard()
    try:
        async for chunk in response.aiter_bytes(chunk_size=8192):
            consumed += len(chunk)
            if consumed > MAX_BYTES:
                if items and mode == "array":
                    return ArrayPage(items, False)
                raise GatewayError("RESPONSE_TOO_LARGE", "Beacon response exceeded 5 MiB")
            if mode is None:
                stripped = chunk.lstrip()
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
            value = decode_json(envelope)
            source = value.get("items") if isinstance(value, dict) else value
            if not isinstance(source, list):
                raise ValueError
            return ArrayPage(source[:max_items], len(source) <= max_items)
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
        self.depth = 0
        self.quoted = False
        self.escaped = False

    def feed(self, chunk: bytes) -> None:
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


def decode_json(data: bytes | bytearray) -> Any:
    def nonfinite(_value: str) -> None:
        raise ValueError("invalid JSON number")

    value = json.loads(data, parse_constant=nonfinite)
    pending = [(iter([value]), 0)]
    while pending:
        iterator, depth = pending[-1]
        try:
            item = next(iterator)
        except StopIteration:
            pending.pop()
            continue
        if isinstance(item, (dict, list)):
            if depth >= 128:
                raise ValueError("excessive JSON nesting")
            pending.append((iter(item.values() if isinstance(item, dict) else item), depth + 1))
    return value
