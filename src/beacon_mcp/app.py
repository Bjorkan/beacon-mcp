"""Foundation ASGI application with public composition and body-free access logging."""

from starlette.applications import Starlette

from beacon_mcp.asgi import create_asgi_app
from beacon_mcp.config import Config


def create_app(config: Config) -> Starlette:
    return create_asgi_app()
