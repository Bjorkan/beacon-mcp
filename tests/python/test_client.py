import asyncio
import json
from collections.abc import AsyncIterator, Awaitable, Callable
from typing import Never

import httpx
import pytest

from beacon_mcp.client import MAX_BYTES, BeaconClient, GatewayError, Operation
from beacon_mcp.config import Config

MockHandler = (
    Callable[[httpx.Request], httpx.Response] | Callable[[httpx.Request], Awaitable[httpx.Response]]
)


def client(handler: MockHandler) -> BeaconClient:
    return BeaconClient(
        Config.from_env({"BEACON_BASE_URL": "https://example.test/prefix"}),
        transport=httpx.MockTransport(handler),
    )


async def test_fixed_encoded_get() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "GET"
        assert request.url.path == "/prefix/api/v1/nodes"
        assert request.url.query == b"iatas=ARN%2CMMX&scope=%23se13"
        assert request.headers["user-agent"] == "beacon-mcp/2.2.0"
        return httpx.Response(200, json={"items": []})

    c = client(handler)
    try:
        assert await c.request(
            Operation.Nodes, query=[("iatas", "ARN,MMX"), ("scope", "#se13")]
        ) == {"items": []}
    finally:
        await c.aclose()


@pytest.mark.parametrize(
    "op,id",
    [
        (Operation.Node, "../admin"),
        (Operation.Packet, "not-hex"),
        (Operation.ScopeDetail, ".."),
        (Operation.ScopeDetail, "%2e%2e"),
        (Operation.ScopeDetail, "a/b"),
        (Operation.ScopeDetail, "a\\b"),
        (Operation.RouteObservations, "ARN:../../admin"),
        (Operation.RegionDetail, "0"),
    ],
)
async def test_reject_before_network(op: Operation, id: str) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("unsafe upstream request")

    c = client(forbidden)
    try:
        with pytest.raises(GatewayError, match="invalid"):
            await c.request(op, id)
    finally:
        await c.aclose()


@pytest.mark.parametrize(
    "status,code",
    [
        (400, "INVALID_ARGUMENT"),
        (404, "NOT_FOUND"),
        (429, "RESOURCE_EXHAUSTED"),
        (503, "DEADLINE_EXCEEDED"),
    ],
)
async def test_status_and_retry_after(status: int, code: str) -> None:
    calls = 0

    def handler(_request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        return httpx.Response(
            status,
            headers={"Retry-After": "10"},
            json={"error": {"code": "deadline_exceeded", "message": "secret SQL"}},
        )

    c = client(handler)
    try:
        with pytest.raises(GatewayError) as caught:
            await c.request(Operation.Nodes)
        assert caught.value.code == code and caught.value.retry_after == "10"
        assert calls == 1
        if status >= 500:
            assert "secret" not in caught.value.message
    finally:
        await c.aclose()


async def test_retry_exactly_once() -> None:
    calls = 0

    def handler(_request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        return httpx.Response(503, json={})

    c = client(handler)
    try:
        with pytest.raises(GatewayError):
            await c.request(Operation.ServerInfo)
        assert calls == 2
    finally:
        await c.aclose()


class Chunks(httpx.AsyncByteStream):
    def __init__(self, pieces: list[bytes]) -> None:
        self.pieces, self.count, self.closed = pieces, 0, False

    async def __aiter__(self) -> AsyncIterator[bytes]:
        for piece in self.pieces:
            self.count += 1
            yield piece

    async def aclose(self) -> None:
        self.closed = True


def fixed_response(response: httpx.Response) -> MockHandler:
    """Provide a typed handler for a fixed synthetic upstream response."""
    def handler(_request: httpx.Request) -> httpx.Response:
        return response

    return handler


async def test_incremental_early_close_and_nested_json() -> None:
    item = {"name": 'a,\\"b', "nested": [1, {"k": "v"}]}
    stream = Chunks([b"[" + (json.dumps(item).encode() + b",") * 200] * 1000)
    c = client(fixed_response(httpx.Response(200, stream=stream)))
    try:
        page = await c.array_page(Operation.RouteSearch, max_items=3)
        assert page.items == [item] * 3 and not page.complete
        assert stream.count == 1 and stream.closed
    finally:
        await c.aclose()


@pytest.mark.parametrize(
    "body,expected,complete",
    [(b'["ARN","MMX"]', ["ARN", "MMX"], True), (b'{"items":[1,2,3]}', [1, 2], False)],
)
async def test_array_and_envelope(body: bytes, expected: list[str | int], complete: bool) -> None:
    c = client(fixed_response(httpx.Response(200, content=body)))
    try:
        page = await c.array_page(Operation.Iatas, max_items=2 if not complete else 3)
        assert page.items == expected and page.complete == complete
    finally:
        await c.aclose()


@pytest.mark.parametrize(
    "body,code", [(b"invalid", "UNAVAILABLE"), (b"x" * (MAX_BYTES + 1), "RESPONSE_TOO_LARGE")]
)
async def test_bad_bodies(body: bytes, code: str) -> None:
    c = client(fixed_response(httpx.Response(200, content=body)))
    try:
        with pytest.raises(GatewayError) as caught:
            await c.request(Operation.ServerInfo)
        assert caught.value.code == code
    finally:
        await c.aclose()


async def test_total_deadline_and_cancellation() -> None:
    async def handler(_request: httpx.Request) -> httpx.Response:
        await asyncio.sleep(2)
        return httpx.Response(200, json={})

    c = BeaconClient(
        Config.from_env({"BEACON_BASE_URL": "https://x.test", "BEACON_TIMEOUT_MS": "100"}),
        transport=httpx.MockTransport(handler),
    )
    try:
        with pytest.raises(GatewayError) as caught:
            await c.request(Operation.ServerInfo)
        assert caught.value.code == "DEADLINE_EXCEEDED"
        pending = asyncio.create_task(c.request(Operation.ServerInfo))
        await asyncio.sleep(0)
        pending.cancel()
        with pytest.raises(asyncio.CancelledError):
            await pending
    finally:
        await c.aclose()


@pytest.mark.parametrize(
    "body,array",
    [
        (b"NaN", False),
        (b"[" * 129 + b"0" + b"]" * 129, True),
        (b"[" * 129 + b"0" + b"]" * 129, False),
    ],
)
async def test_nonfinite_and_excessively_nested_json(body: bytes, array: bool) -> None:
    c = client(fixed_response(httpx.Response(200, content=body)))
    try:
        with pytest.raises(GatewayError) as caught:
            if array:
                await c.array_page(Operation.Iatas, max_items=2)
            else:
                await c.request(Operation.ServerInfo)
        assert caught.value.code == "UNAVAILABLE"
    finally:
        await c.aclose()


async def test_truncated_array_retains_complete_items_with_explicit_truncation() -> None:
    c = client(fixed_response(httpx.Response(200, content=b'[{"id":1},')))
    try:
        result = await c.array_page(Operation.Iatas, max_items=3)
        assert result.items == [{"id": 1}] and not result.complete
    finally:
        await c.aclose()


@pytest.mark.parametrize("status,code", [(200, "UNAVAILABLE"), (429, "RESOURCE_EXHAUSTED")])
async def test_unexpected_compression_is_never_inflated_or_read(status: int, code: str) -> None:
    stream = Chunks([b"compressed body must never be read"])

    def upstream(request: httpx.Request) -> httpx.Response:
        assert request.headers["accept-encoding"] == "identity"
        return httpx.Response(
            status, headers={"Content-Encoding": "gzip", "Retry-After": "10"}, stream=stream
        )

    c = client(upstream)
    try:
        with pytest.raises(GatewayError) as caught:
            await c.request(Operation.ServerInfo)
        assert caught.value.code == code
        assert stream.count == 0 and stream.closed
        if status == 429:
            assert caught.value.retry_after == "10" and caught.value.status == 429
    finally:
        await c.aclose()


async def test_public_reads_never_retain_or_resend_cookies() -> None:
    calls = 0

    def upstream(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        assert "cookie" not in request.headers
        return httpx.Response(
            200, headers={"Set-Cookie": "tracking=synthetic-secret; Path=/"}, json={}
        )

    c = client(upstream)
    try:
        for _ in range(3):
            await c.request(Operation.ServerInfo)
        assert calls == 3 and len(c.http.cookies) == 0
    finally:
        await c.aclose()
