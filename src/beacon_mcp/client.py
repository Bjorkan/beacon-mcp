"""Allowlisted public GET operations, bounded streaming and classified failures."""

import asyncio
import re
import secrets
from http import HTTPStatus
from http.cookiejar import CookieJar, DefaultCookiePolicy
from typing import Any, cast
from urllib.parse import quote, urlencode

import httpx

from beacon_mcp.config import Config
from beacon_mcp.errors import GatewayError, invalid
from beacon_mcp.json_stream import MAX_BYTES as MAX_BYTES
from beacon_mcp.json_stream import ArrayPage, decode_json, read_array, read_bounded
from beacon_mcp.operations import ARRAY_OPERATIONS, Operation
from beacon_mcp.validation import identifier, json_object

MAX_URL = 8_192


def classify(response: httpx.Response, body: Any) -> GatewayError:
    payload = json_object(body)
    error = json_object(payload.get("error")) if payload is not None else None
    details: dict[str, object] = error if error is not None else {}
    raw_code = details.get("code")
    upstream = raw_code if isinstance(raw_code, str) else None
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
    upstream_message = details.get("message")
    if status < 500 and isinstance(upstream_message, str):
        message = upstream_message
    return GatewayError(
        code,
        message,
        status=status,
        upstream_code=upstream,
        retry_after=response.headers.get("retry-after"),
    )


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
        # Public callers are typed, but reject invalid runtime values explicitly.
        # Narrowing from object prevents static tools from assuming the guard is dead.
        if not isinstance(cast(object, operation), Operation):
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
        # One end-to-end request budget, including jitter and an optional retry.
        deadline = asyncio.get_running_loop().time() + timeout
        for attempt in range(2):
            try:
                # HTTPX's inactivity timeout alone doesn't bound a slow-drip body.
                async with asyncio.timeout_at(deadline):
                    async with self.http.stream("GET", url, timeout=timeout) as response:
                        body = await read_response(response, max_items)
                        if response.is_success:
                            return body
                        can_retry = (
                            attempt == 0
                            and response.status_code in (502, 503, 504)
                            and "retry-after" not in response.headers
                        )
                        if not can_retry:
                            raise classify(response, body)
            except (TimeoutError, httpx.TimeoutException):
                raise GatewayError("DEADLINE_EXCEEDED", "Beacon request timed out") from None
            except httpx.TransportError:
                if attempt:
                    raise GatewayError("UNAVAILABLE", "Beacon is temporarily unavailable") from None
            try:
                # Include retry backoff in the same deadline; otherwise a slow
                # first attempt could double the configured request budget.
                async with asyncio.timeout_at(deadline):
                    await asyncio.sleep((25 + secrets.randbelow(50)) / 1000)
            except TimeoutError:
                raise GatewayError("DEADLINE_EXCEEDED", "Beacon request timed out") from None
        raise GatewayError("UNAVAILABLE", "Beacon retry attempts exhausted")


async def read_response(response: httpx.Response, max_items: int | None) -> Any:
    """Decode a response within the caller's deadline, without inflating it."""
    if response.status_code == 204 and max_items is None:
        return None
    # HTTPX otherwise inflates a chunk before applying our byte cap.
    # Match the reference client's uncompressed public GET transport.
    encoded = response.headers.get("content-encoding", "").strip().lower()
    if encoded not in ("", "identity"):
        if response.is_success:
            raise GatewayError("UNAVAILABLE", "Beacon returned unsupported content encoding")
        return None
    if response.is_success and max_items is not None:
        return await read_array(response, max_items)
    data = await read_bounded(response)
    try:
        return decode_json(data)
    except (ValueError, RecursionError):
        if response.is_success:
            raise GatewayError("UNAVAILABLE", "Beacon returned invalid JSON") from None
        return None
