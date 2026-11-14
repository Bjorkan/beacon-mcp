"""Plain ASGI composition, health routes and body-free access logging."""

import logging
import re
import time

from starlette.applications import Starlette
from starlette.middleware import Middleware
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import BaseRoute, Mount, Route
from starlette.types import ASGIApp, Lifespan, Message, Receive, Scope, Send

logger = logging.getLogger("beacon_mcp.access")


class AccessLog:
    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http" or scope["path"] != "/":
            await self.app(scope, receive, send)
            return
        headers = dict(scope["headers"])

        def safe(key: bytes) -> str:
            raw = headers.get(key, b"")
            return (
                raw.decode("ascii")
                if re.fullmatch(rb"[A-Za-z0-9_./-]{1,80}", raw)
                else "unspecified"
            )

        method = safe(b"mcp-method")
        tool = safe(b"mcp-name") if method == "tools/call" else "-"
        start = time.monotonic()

        async def logged_send(message: Message) -> None:
            if message["type"] == "http.response.start":
                logger.info(
                    "http_method=%s mcp_method=%s tool=%s http_status=%d duration_ms=%d",
                    scope["method"],
                    method,
                    tool,
                    message["status"],
                    int((time.monotonic() - start) * 1000),
                )
            await send(message)

        await self.app(scope, receive, logged_send)


async def health(_request: Request) -> JSONResponse:
    return JSONResponse({"status": "ok"})


def create_asgi_app(
    endpoint: ASGIApp | None = None, *, lifespan: Lifespan[Starlette] | None = None
) -> Starlette:
    routes: list[BaseRoute] = [Route("/healthz", health), Route("/readyz", health)]
    if endpoint is not None:
        routes.append(Mount("/", app=endpoint))
    return Starlette(routes=routes, lifespan=lifespan, middleware=[Middleware(AccessLog)])
