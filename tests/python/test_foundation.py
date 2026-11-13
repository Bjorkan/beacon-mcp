import logging

import httpx
import pytest
from asgi_lifespan import LifespanManager

from beacon_mcp.app import create_app
from beacon_mcp.config import Config


def test_defaults():
    config = Config.from_env({"BEACON_BASE_URL": "https://beacon.example/prefix"})
    assert config.base_url == "https://beacon.example/prefix/"
    assert config.ws_url == "wss://beacon.example/ws"
    assert config.port == 3000 and config.timeout_ms == 10000
    assert config.subscriptions_enabled and not config.traffic_enabled


@pytest.mark.parametrize(
    "key,value",
    [
        ("BEACON_BASE_URL", "https://u:p@beacon.example"),
        ("BEACON_BASE_URL", "https://beacon.example/?"),
        ("BEACON_BASE_URL", "file:///tmp/x"),
        ("BEACON_BASE_URL", "https://x:bad"),
        ("HOST", "bad host"),
        ("HOST", ""),
        ("PORT", "80"),
        ("PORT", "65536"),
        ("LOG_LEVEL", "verbose"),
        ("BEACON_TIMEOUT_MS", "0"),
        ("BEACON_SUBSCRIPTIONS_ENABLED", "yes"),
        ("BEACON_WS_URL", "https://x/ws"),
        ("BEACON_SUBSCRIPTIONS_MAX_STREAMS", "0"),
        ("SHUTDOWN_GRACE_MS", "60001"),
    ],
)
def test_invalid_config_redacts_values(key, value):
    with pytest.raises(ValueError) as error:
        Config.from_env({"BEACON_BASE_URL": "https://beacon.example", key: value})
    assert "u:p" not in str(error.value)


async def test_health_without_upstream(caplog):
    config = Config.from_env({"BEACON_BASE_URL": "http://unreachable.invalid"})
    app = create_app(config)
    async with (
        LifespanManager(app),
        httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://test") as client,
    ):
        for path in ("/healthz", "/readyz"):
            response = await client.get(path)
            assert response.json() == {"status": "ok"}
        with caplog.at_level(logging.INFO, logger="beacon_mcp.access"):
            await client.post(
                "/?secret=query",
                headers={"Mcp-Method": "tools/call", "Mcp-Name": "secret=value"},
                content="private-body",
            )
    assert "mcp_method=tools/call tool=unspecified" in caplog.text
    assert all(
        secret not in caplog.text for secret in ("private-body", "secret=value", "secret=query")
    )
