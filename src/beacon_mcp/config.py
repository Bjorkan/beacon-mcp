"""Strict environment configuration; errors never include environment values."""

import ipaddress
import os
import re
from collections.abc import Mapping
from dataclasses import dataclass
from urllib.parse import urlsplit, urlunsplit


def public_url(value: str, name: str, schemes: tuple[str, ...]) -> str:
    try:
        parts = urlsplit(value)
        valid = (
            parts.scheme in schemes
            and parts.hostname
            and parts.port != 0
            and "@" not in parts.netloc
            and "?" not in value
            and "#" not in value
            and not re.search(r"[\s\x00-\x1f\x7f\\]", value)
        )
    except ValueError:
        valid = False
    if not valid:
        raise ValueError(f"invalid {name}")
    return value


@dataclass(frozen=True, repr=False)
class Config:
    base_url: str
    ws_url: str
    host: str = "0.0.0.0"
    port: int = 3000
    log_level: str = "info"
    timeout_ms: int = 10000
    stats_timeout_ms: int = 20000
    shutdown_grace_ms: int = 10000
    subscriptions_enabled: bool = True
    traffic_enabled: bool = False
    max_streams: int = 64
    debounce_ms: int = 500

    @classmethod
    def from_env(cls, env: Mapping[str, str] | None = None) -> "Config":
        env = os.environ if env is None else env

        def integer(key: str, default: int, low: int, high: int) -> int:
            value = env.get(key, str(default))
            if not re.fullmatch(r"\+?[0-9]+", value) or not low <= int(value) <= high:
                raise ValueError(f"invalid {key}")
            return int(value)

        def boolean(key: str, default: bool) -> bool:
            value = env.get(key, str(default).lower())
            if value not in ("true", "false"):
                raise ValueError(f"{key} must be true or false")
            return value == "true"

        base = public_url(env.get("BEACON_BASE_URL", ""), "BEACON_BASE_URL", ("http", "https"))
        base = base.rstrip("/") + "/"
        parts = urlsplit(base)
        ws = env.get("BEACON_WS_URL") or urlunsplit(
            ("wss" if parts.scheme == "https" else "ws", parts.netloc, "/ws", "", "")
        )
        public_url(ws, "BEACON_WS_URL", ("ws", "wss"))
        if not urlsplit(ws).path.endswith("/ws"):
            raise ValueError("BEACON_WS_URL must end in /ws")
        host = env.get("HOST", "0.0.0.0")
        try:
            ipaddress.ip_address(host)
        except ValueError:
            if len(host) > 253 or not all(
                re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?", label)
                for label in host.split(".")
            ):
                raise ValueError("HOST must be an IP address or hostname") from None
        level = env.get("LOG_LEVEL", "info")
        if level not in ("fatal", "error", "warn", "info", "debug", "trace", "silent"):
            raise ValueError("invalid LOG_LEVEL")
        enabled = boolean("BEACON_SUBSCRIPTIONS_ENABLED", True)
        traffic = boolean("BEACON_SUBSCRIPTIONS_TRAFFIC_ENABLED", False)
        if traffic and not enabled:
            raise ValueError("Traffic subscriptions require BEACON_SUBSCRIPTIONS_ENABLED=true")
        return cls(
            base_url=base,
            ws_url=ws,
            host=host,
            port=integer("PORT", 3000, 1024, 65535),
            log_level=level,
            timeout_ms=integer("BEACON_TIMEOUT_MS", 10000, 100, 120000),
            stats_timeout_ms=integer("BEACON_STATS_TIMEOUT_MS", 20000, 100, 300000),
            shutdown_grace_ms=integer("SHUTDOWN_GRACE_MS", 10000, 100, 60000),
            subscriptions_enabled=enabled,
            traffic_enabled=traffic,
            max_streams=integer("BEACON_SUBSCRIPTIONS_MAX_STREAMS", 64, 1, 1024),
            debounce_ms=integer("BEACON_SUBSCRIPTIONS_DEBOUNCE_MS", 500, 0, 5000),
        )
