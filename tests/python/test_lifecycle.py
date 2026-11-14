import asyncio
import os
import signal
import socket
import sys
from contextlib import suppress

import httpx

from tests.python.test_subscriptions import ID, event, listen


async def test_cli_uptime_healthcheck_silent_logs_and_sigterm_closes_sse() -> None:
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    env = {
        **os.environ,
        "HOST": "127.0.0.1",
        "PORT": str(port),
        "BEACON_BASE_URL": "http://127.0.0.1:9",
        "SHUTDOWN_GRACE_MS": "100",
        "LOG_LEVEL": "silent",
    }
    child = await asyncio.create_subprocess_exec(
        sys.executable,
        "-m",
        "beacon_mcp",
        env=env,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        async with httpx.AsyncClient(base_url=f"http://127.0.0.1:{port}", timeout=3) as http:
            for _ in range(100):
                try:
                    if (await http.get("/healthz")).status_code == 200:
                        break
                except httpx.ConnectError:
                    pass
                await asyncio.sleep(0.02)
            else:
                raise AssertionError("gateway failed to start")
            await asyncio.sleep(0.2)
            assert child.returncode is None
            check = await asyncio.create_subprocess_exec(
                sys.executable, "-m", "beacon_mcp", "--healthcheck", env=env
            )
            assert await check.wait() == 0
            async with listen(http, [f"beacon://nodes/{ID}"]) as response:
                lines = response.aiter_lines()
                await event(lines, "notifications/subscriptions/acknowledged")
                child.terminate()
                async with asyncio.timeout(4):
                    # Native Uvicorn/SDK cancellation may close a chunked stream abruptly.
                    with suppress(httpx.RemoteProtocolError):
                        async for _ in lines:
                            pass
            stdout, stderr = await asyncio.wait_for(child.communicate(), 3)
            assert child.returncode in (0, -signal.SIGTERM)
            assert not stdout and not stderr
    finally:
        if child.returncode is None:
            child.kill()
            await child.wait()
