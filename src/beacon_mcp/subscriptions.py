"""One upstream socket and a bounded, process-local bus of canonical URI hints."""

import asyncio
import json
import logging
import re
from typing import Any

from mcp.server.subscriptions import InMemorySubscriptionBus, ResourceUpdated
from websockets.asyncio.client import connect

from beacon_mcp.client import BeaconClient, GatewayError, Operation
from beacon_mcp.config import Config
from beacon_mcp.resources import parse_uri

logger = logging.getLogger("beacon_mcp.subscriptions")


class Live:
    def __init__(self, config: Config):
        self.config = config
        self.bus = InMemorySubscriptionBus()
        self.pending: dict[str, asyncio.Task[None]] = {}
        self.closed = False
        self.reconnect_delay = 1.0

    def received(self, event: str, data: Any) -> None:
        if self.closed or not isinstance(data, dict):
            return
        if event == "nodeUpdate":
            kind, value = "nodes", data.get("nodeId")
        elif event == "observerStatus":
            kind, value = "observers", data.get("observerId")
        elif event == "packetObservation" and self.config.traffic_enabled:
            kind, value = "packets", data.get("packetHash")
        elif event == "channelMessage" and self.config.traffic_enabled:
            kind, value = "channels", data.get("channelId")
            if not isinstance(value, int) or isinstance(value, bool):
                return
            value = str(value)
        else:
            return
        if not isinstance(value, str):
            return
        parsed = parse_uri(f"beacon://{kind}/{value}")
        if parsed is None:
            return
        uri = f"beacon://{parsed[0]}/{parsed[1]}"
        if uri in self.pending or len(self.pending) >= 1024:
            return
        # Task captures only a canonical URI, never the event envelope or payload.
        self.pending[uri] = asyncio.create_task(self._publish_later(uri))

    async def _publish_later(self, uri: str) -> None:
        try:
            await asyncio.sleep(self.config.debounce_ms / 1000)
            await self.bus.publish(ResourceUpdated(uri))
        finally:
            self.pending.pop(uri, None)

    async def close(self) -> None:
        self.closed = True
        tasks = list(self.pending.values())
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        self.pending.clear()


async def bridge(client: BeaconClient, live: Live) -> None:
    while not live.closed:
        try:
            await connect_once(client, live)
        except asyncio.CancelledError:
            raise
        except Exception:
            # Network exceptions can contain tickets or whole frames.
            logger.warning("Beacon WebSocket disconnected; updates may have been missed")
        await asyncio.sleep(live.reconnect_delay)
        live.reconnect_delay = min(30, live.reconnect_delay * 2)


async def connect_once(
    client: BeaconClient, live: Live, *, handshake_timeout: float = 10, ping_interval: float = 30
) -> None:
    headers = {}
    try:
        response = await client.request(Operation.WsTicket)
        ticket = response.get("ticket") if isinstance(response, dict) else None
        if not isinstance(ticket, str) or not re.fullmatch(r"[\x20-\x7e]{1,4096}", ticket):
            raise ValueError("invalid WebSocket ticket")
        headers["X-Beacon-Ticket"] = ticket
    except GatewayError as error:
        if error.code != "NOT_FOUND":
            raise
    async with connect(
        live.config.ws_url,
        additional_headers=headers,
        open_timeout=10,
        close_timeout=1,
        max_size=131072,
        max_queue=16,
        compression=None,
        proxy=None,
        ping_interval=None,
    ) as socket:
        ready, seen_hello, awaiting_pong = False, False, False
        ping_id = 0
        loop = asyncio.get_running_loop()
        handshake_deadline = loop.time() + handshake_timeout
        next_ping = loop.time() + ping_interval
        while not live.closed:
            deadline = next_ping if ready else handshake_deadline
            try:
                async with asyncio.timeout(max(0, deadline - loop.time())):
                    frame = await socket.recv()
            except TimeoutError:
                if not ready:
                    raise ValueError("WebSocket subscription handshake timed out") from None
                if awaiting_pong:
                    raise ValueError("WebSocket application pong timed out") from None
                ping_id += 1
                await socket.send(json.dumps({"v": 1, "type": "ping", "id": f"mcp-ping-{ping_id}"}))
                awaiting_pong = True
                next_ping = loop.time() + ping_interval
                continue
            if not isinstance(frame, str):
                continue
            try:
                message = json.loads(frame)
            except (ValueError, RecursionError):
                continue
            if (
                not isinstance(message, dict)
                or type(message.get("v")) is not int
                or message["v"] != 1
            ):
                continue
            kind = message.get("type")
            if kind == "hello" and not ready:
                seen_hello = True
                events = ["nodeUpdate", "observerStatus"]
                if live.config.traffic_enabled:
                    events += ["packetObservation", "channelMessage"]
                await socket.send(
                    json.dumps(
                        {
                            "v": 1,
                            "type": "subscribe",
                            "id": "mcp-changes",
                            "scope": {"events": events},
                        }
                    )
                )
            elif kind == "subscribed" and seen_hello and message.get("id") == "mcp-changes":
                ready = True
                live.reconnect_delay = 1.0
                next_ping = loop.time() + ping_interval
                logger.info("Beacon WebSocket subscribed to resource updates")
            elif kind == "pong" and message.get("id") == f"mcp-ping-{ping_id}":
                awaiting_pong = False
            elif kind == "lagged":
                logger.warning("Beacon WebSocket dropped events; no replay available")
            elif kind == "event" and ready:
                live.received(message.get("event", ""), message.get("data"))
