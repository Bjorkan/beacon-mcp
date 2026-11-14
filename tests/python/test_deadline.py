"""Regression tests for the upstream client's end-to-end retry budget."""

import asyncio

import httpx
import pytest

from beacon_mcp.client import BeaconClient, GatewayError, Operation
from beacon_mcp.config import Config


async def test_total_timeout_includes_first_attempt_and_retry_backoff() -> None:
    attempts = 0

    async def upstream(_request: httpx.Request) -> httpx.Response:
        nonlocal attempts
        attempts += 1
        if attempts == 1:
            await asyncio.sleep(0.085)
            return httpx.Response(503, json={})
        return httpx.Response(200, json={"unexpected": "second request"})

    client = BeaconClient(
        Config.from_env({"BEACON_BASE_URL": "https://beacon.test", "BEACON_TIMEOUT_MS": "100"}),
        transport=httpx.MockTransport(upstream),
    )
    try:
        with pytest.raises(GatewayError) as caught:
            await client.request(Operation.ServerInfo)
        assert caught.value.code == "DEADLINE_EXCEEDED"
        # The first 85 ms response plus >=25 ms retry jitter exhaust the
        # global 100 ms budget; the second request must never be dispatched.
        assert attempts == 1
    finally:
        await client.aclose()


async def test_fast_transient_failure_still_retries_once() -> None:
    attempts = 0

    async def upstream(_request: httpx.Request) -> httpx.Response:
        nonlocal attempts
        attempts += 1
        if attempts == 1:
            return httpx.Response(503, json={})
        return httpx.Response(200, json={"ok": True})

    client = BeaconClient(
        Config.from_env({"BEACON_BASE_URL": "https://beacon.test", "BEACON_TIMEOUT_MS": "300"}),
        transport=httpx.MockTransport(upstream),
    )
    try:
        assert await client.request(Operation.ServerInfo) == {"ok": True}
        assert attempts == 2
    finally:
        await client.aclose()
