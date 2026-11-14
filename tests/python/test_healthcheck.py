"""The container probe must work without importing server dependencies."""

import asyncio
import os
import sys
from pathlib import Path

import pytest

SOURCE_DIRECTORY: str = str(Path("src").resolve())


@pytest.mark.parametrize("status,expected_exit", [(200, 0), (503, 1)])
async def test_healthcheck_works_without_site_packages(status: int, expected_exit: int) -> None:
    requests: list[bytes] = []

    async def respond(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        try:
            headers: bytes = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), 2)
            requests.append(headers.split(b"\r\n", 1)[0])
            writer.write(
                f"HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".encode()
            )
            await writer.drain()
        finally:
            writer.close()
            await writer.wait_closed()

    server: asyncio.Server = await asyncio.start_server(respond, "127.0.0.1", 0)
    async with server:
        port: int = server.sockets[0].getsockname()[1]
        env: dict[str, str] = {
            **os.environ,
            "HOST": "127.0.0.1",
            "PORT": str(port),
            "PYTHONPATH": SOURCE_DIRECTORY,
        }
        env.pop("BEACON_BASE_URL", None)
        # -S removes installed dependencies; the probe needs only the stdlib.
        child: asyncio.subprocess.Process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-S",
            "-m",
            "beacon_mcp",
            "--healthcheck",
            env=env,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout: bytes
            stderr: bytes
            stdout, stderr = await asyncio.wait_for(child.communicate(), 5)
            assert child.returncode == expected_exit
            assert not stdout and not stderr
            assert requests == [b"GET /healthz HTTP/1.1"]
        finally:
            if child.returncode is None:
                child.kill()
                await child.wait()
