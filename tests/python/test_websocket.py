import asyncio
import json

import httpx
import pytest
from mcp.server.subscriptions import ResourceUpdated
from websockets.asyncio.server import ServerConnection, serve
from websockets.exceptions import ConnectionClosed

from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from beacon_mcp.subscriptions import Live, bridge, connect_once

ID = "123e4567-e89b-42d3-a456-426614174000"


def config(**env: str) -> Config:
    return Config.from_env(
        {"BEACON_BASE_URL": "https://beacon.test", "BEACON_SUBSCRIPTIONS_DEBOUNCE_MS": "0", **env}
    )


async def test_debounce_validation_and_only_uri_retention():
    live = Live(config(BEACON_SUBSCRIPTIONS_DEBOUNCE_MS="10"))
    queue: asyncio.Queue[ResourceUpdated] = asyncio.Queue()
    unsubscribe = live.bus.subscribe(queue.put_nowait)
    for _ in range(5):
        live.received("nodeUpdate", {"nodeId": ID, "text": "never-store"})
    live.received("nodeUpdate", {"nodeId": "../admin"})
    live.received("packetObservation", {"packetHash": "AABBCCDDEEFF0011"})
    assert list(live.pending) == [f"beacon://nodes/{ID}"]
    assert (await asyncio.wait_for(queue.get(), 1)).uri == f"beacon://nodes/{ID}"
    assert queue.empty()
    unsubscribe()
    await live.close()


async def test_traffic_opt_in_and_pending_bound():
    live = Live(
        config(BEACON_SUBSCRIPTIONS_TRAFFIC_ENABLED="true", BEACON_SUBSCRIPTIONS_DEBOUNCE_MS="5000")
    )
    live.received("packetObservation", {"packetHash": "AABBCCDDEEFF0011", "content": "discard"})
    live.received("channelMessage", {"channelId": 25, "text": "discard"})
    live.received("channelMessage", {"channelId": True})
    assert set(live.pending) == {"beacon://packets/aabbccddeeff0011", "beacon://channels/25"}
    for i in range(2000):
        live.received("channelMessage", {"channelId": i + 1})
    assert len(live.pending) == 1024
    await live.close()
    assert not live.pending


@pytest.mark.parametrize("legacy", [False, True])
@pytest.mark.parametrize("traffic", [False, True])
async def test_ticket_handshake_and_reconnect(legacy: bool, traffic: bool) -> None:
    tickets: list[str | None] = []
    events: asyncio.Queue[ResourceUpdated] = asyncio.Queue()

    async def mock(socket: ServerConnection) -> None:
        tickets.append(socket.request.headers.get("X-Beacon-Ticket"))
        await socket.send(
            json.dumps(
                {"v": 1, "type": "event", "event": "observerStatus", "data": {"observerId": ID}}
            )
        )
        await socket.send(json.dumps({"v": 1, "type": "hello"}))
        request = json.loads(await socket.recv())
        assert isinstance(request, dict) and isinstance(request.get("scope"), dict)
        assert request["scope"]["events"] == ["nodeUpdate", "observerStatus"] + (
            ["packetObservation", "channelMessage"] if traffic else []
        )
        await socket.send(json.dumps({"v": 1, "type": "subscribed", "id": "mcp-changes"}))
        await socket.send(
            json.dumps({"v": 1, "type": "event", "event": "nodeUpdate", "data": {"nodeId": ID}})
        )
        if len(tickets) < 2:
            await socket.close()
        else:
            await socket.wait_closed()

    async with serve(mock, "127.0.0.1", 0) as server:
        port = server.sockets[0].getsockname()[1]
        cfg = config(
            BEACON_WS_URL=f"ws://127.0.0.1:{port}/ws",
            BEACON_SUBSCRIPTIONS_TRAFFIC_ENABLED=str(traffic).lower(),
        )
        calls = 0

        def ticket(_request: httpx.Request) -> httpx.Response:
            nonlocal calls
            calls += 1
            return httpx.Response(404 if legacy else 200, json={"ticket": f"ticket-{calls}"})

        client = BeaconClient(cfg, transport=httpx.MockTransport(ticket))
        live = Live(cfg)
        live.bus.subscribe(events.put_nowait)
        task = asyncio.create_task(bridge(client, live))
        try:
            for _ in range(2):
                assert (await asyncio.wait_for(events.get(), 4)).uri == f"beacon://nodes/{ID}"
            assert tickets == ([None, None] if legacy else ["ticket-1", "ticket-2"])
            assert calls == 2 and events.empty()
        finally:
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
            await live.close()
            await client.aclose()


@pytest.mark.parametrize("scenario", ["no-hello", "bad-ack", "oversized", "missing-pong"])
async def test_handshake_frame_and_heartbeat_fail_closed(scenario: str) -> None:
    async def mock(socket: ServerConnection) -> None:
        if scenario == "oversized":
            await socket.send("x" * 131073)
        elif scenario != "no-hello":
            await socket.send(json.dumps({"v": 1, "type": "hello"}))
            await socket.recv()
            await socket.send(
                json.dumps(
                    {
                        "v": 1,
                        "type": "subscribed",
                        "id": "wrong" if scenario == "bad-ack" else "mcp-changes",
                    }
                )
            )
        await socket.wait_closed()

    def not_found(_request: httpx.Request) -> httpx.Response:
        return httpx.Response(404)

    async with serve(mock, "127.0.0.1", 0) as server:
        cfg = config(BEACON_WS_URL=f"ws://127.0.0.1:{server.sockets[0].getsockname()[1]}/ws")
        client = BeaconClient(cfg, transport=httpx.MockTransport(not_found))
        live = Live(cfg)
        events: asyncio.Queue[ResourceUpdated] = asyncio.Queue()
        live.bus.subscribe(events.put_nowait)
        try:
            with pytest.raises((ValueError, ConnectionClosed)):
                await asyncio.wait_for(
                    connect_once(client, live, handshake_timeout=0.05, ping_interval=0.03), 1
                )
            assert events.empty()
        finally:
            await client.aclose()
            await live.close()
