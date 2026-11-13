"""Official MCP v2 runtime with frozen discovery and incremental registration."""

import importlib
import json
import pkgutil
from collections.abc import Awaitable, Callable
from contextlib import AbstractAsyncContextManager
from importlib.resources import files
from typing import Any

import mcp.types as types
from jsonschema import Draft202012Validator, ValidationError
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel import Server
from mcp.server.subscriptions import ListenHandler
from pydantic import TypeAdapter

from beacon_mcp import __version__, handlers
from beacon_mcp.client import BeaconClient, GatewayError

INTEGER = TypeAdapter(int)
INTEGERS = TypeAdapter(list[int])

Handler = Callable[[BeaconClient, dict[str, Any]], Awaitable[Any]]


def load_contracts() -> dict[str, Any]:
    result = {}
    for document in sorted(files("beacon_mcp").joinpath("contracts").iterdir(), key=str):
        if document.name.endswith("-tool-definitions.json"):
            result.update(json.loads(document.read_text()))
    return result


CONTRACTS = load_contracts()
TOOL_ORDER = json.loads(files("beacon_mcp").joinpath("contracts/tool-order.json").read_text())


def success(value: Any) -> types.CallToolResult:
    count = (
        len(value["items"])
        if isinstance(value, dict) and isinstance(value.get("items"), list)
        else None
    )
    text = f"Beacon returned {count} item(s)." if count is not None else "Beacon request succeeded."
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structured_content=value,
        is_error=False,
    )


def tool_error(text: str, details: dict[str, Any] | None = None) -> types.CallToolResult:
    fields: dict[str, Any] = {
        "content": [types.TextContent(type="text", text=text)],
        "is_error": True,
    }
    if details is not None:
        fields["structured_content"] = {"error": details}
    return types.CallToolResult(**fields)


def upstream_error(error: GatewayError) -> types.CallToolResult:
    kind = {
        "INVALID_ARGUMENT": "BeaconValidationError" if error.status else "BeaconInputError",
        "NOT_FOUND": "BeaconNotFoundError",
        "DEADLINE_EXCEEDED": "BeaconTimeoutError",
        "RESPONSE_TOO_LARGE": "BeaconResponseTooLargeError",
        "RESOURCE_EXHAUSTED": "BeaconRateLimitError",
        "UNAVAILABLE": "BeaconUpstreamError",
        "INTERNAL": "BeaconInternalError",
    }[error.code]
    details: dict[str, Any] = {"code": error.code, "type": kind}
    for key, value in (
        ("status", error.status),
        ("upstreamCode", error.upstream_code),
        ("retryAfter", error.retry_after),
    ):
        if value is not None:
            details[key] = value
    return tool_error(error.message, details)


class Registry:
    def __init__(self) -> None:
        self.handlers: dict[str, Handler] = {}
        self.validators: dict[str, Draft202012Validator] = {}

    def register(self, name: str, handler: Handler) -> None:
        if name in self.handlers or name not in CONTRACTS:
            raise ValueError("duplicate or uncontracted tool")
        self.handlers[name] = handler
        self.validators[name] = Draft202012Validator(CONTRACTS[name]["inputSchema"])

    def list_tools(self) -> list[types.Tool]:
        return [
            types.Tool.model_validate(CONTRACTS[name])
            for name in TOOL_ORDER
            if name in self.handlers
        ]

    async def call(
        self, client: BeaconClient, name: str, arguments: dict[str, Any]
    ) -> types.CallToolResult:
        if name not in self.handlers:
            return tool_error("Unknown Beacon tool")
        try:
            # Low-level Server validates MCP params, not arbitrary advertised JSON Schemas.
            # Use the standard JSON Schema validator for this application's frozen arguments.
            self.validators[name].validate(arguments)
        except ValidationError as error:
            return tool_error(f"Invalid arguments: {error.message}")
        try:
            rules = CONTRACTS[name]["inputSchema"]["properties"]
            arguments = {
                key: INTEGER.validate_python(value)
                if rules[key].get("type") == "integer"
                else INTEGERS.validate_python(value)
                if rules[key].get("items", {}).get("type") == "integer"
                else value
                for key, value in arguments.items()
            }
            return success(await self.handlers[name](client, arguments))
        except GatewayError as error:
            return upstream_error(error)


def default_registry() -> Registry:
    registry = Registry()
    for module in sorted(pkgutil.iter_modules(handlers.__path__), key=lambda module: module.name):
        importlib.import_module(f"beacon_mcp.handlers.{module.name}").register(registry)
    return registry


def create_server(
    client: BeaconClient,
    registry: Registry | None = None,
    *,
    on_subscriptions_listen: ListenHandler | None = None,
    lifespan: Callable[[Server], AbstractAsyncContextManager[dict[str, Any]]] | None = None,
) -> Server:
    registry = registry if registry is not None else default_registry()

    async def list_tools(
        _ctx: ServerRequestContext, _params: types.PaginatedRequestParams | None
    ) -> types.ListToolsResult:
        return types.ListToolsResult(tools=registry.list_tools())

    async def call_tool(
        _ctx: ServerRequestContext, params: types.CallToolRequestParams
    ) -> types.CallToolResult:
        return await registry.call(client, params.name, params.arguments or {})

    options: dict[str, Any] = {"lifespan": lifespan} if lifespan is not None else {}

    def input_schema(name: str) -> dict[str, Any] | None:
        return CONTRACTS[name]["inputSchema"] if name in registry.handlers else None

    server = Server(
        "beacon-mcp",
        version=__version__,
        on_list_tools=list_tools,
        on_call_tool=call_tool,
        get_tool_input_schema=input_schema,
        on_subscriptions_listen=on_subscriptions_listen,
        **options,
    )

    return server
