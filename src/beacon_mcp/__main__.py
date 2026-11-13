"""CLI entry point and network-local container health check."""

import logging
import math
import os
import sys
from urllib.request import ProxyHandler, build_opener

import uvicorn

from beacon_mcp.app import create_app
from beacon_mcp.config import Config


def healthcheck() -> int:
    host = os.environ.get("HOST", "127.0.0.1")
    if host in ("0.0.0.0", "::"):
        host = "127.0.0.1" if host == "0.0.0.0" else "[::1]"
    elif ":" in host:
        host = f"[{host}]"
    try:
        with build_opener(ProxyHandler({})).open(
            f"http://{host}:{int(os.environ.get('PORT', '3000'))}/healthz", timeout=1
        ) as response:
            return 0 if response.status == 200 else 1
    except (OSError, ValueError):
        return 1


def main() -> None:
    if sys.argv[1:] == ["--healthcheck"]:
        sys.exit(healthcheck())
    try:
        config = Config.from_env()
    except ValueError as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
    level = {
        "fatal": 50,
        "error": 40,
        "warn": 30,
        "info": 20,
        "debug": 10,
        "trace": 10,
        "silent": 100,
    }[config.log_level]
    logging.basicConfig(level=level, format="%(levelname)s %(name)s %(message)s")
    # Libraries must never log URLs, headers or response contents, even at debug.
    for name in ("httpx", "httpcore", "websockets", "mcp", "httpx2"):
        logging.getLogger(name).setLevel(logging.WARNING)
    uvicorn.run(
        create_app(config),
        host=config.host,
        port=config.port,
        access_log=False,
        log_config=None,
        proxy_headers=False,
        timeout_graceful_shutdown=math.ceil(config.shutdown_grace_ms / 1000),
    )


if __name__ == "__main__":
    main()
