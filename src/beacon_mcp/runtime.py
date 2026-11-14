"""Official MCP v2 runtime with frozen discovery and incremental registration."""

import json
from collections.abc import Awaitable, Callable
from contextlib import AbstractAsyncContextManager
from importlib.resources import files
from typing import Any, cast

import mcp.types as types
from jsonschema import Draft202012Validator, ValidationError
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel import Server
from mcp.server.subscriptions import ListenHandler
from pydantic import TypeAdapter

from beacon_mcp import __version__
from beacon_mcp.client import BeaconClient, GatewayError, json_object

INTEGER = TypeAdapter(int)
INTEGERS = TypeAdapter(list[int])

Handler = Callable[[BeaconClient, dict[str, Any]], Awaitable[Any]]


def load_contracts() -> dict[str, Any]:
    result: dict[str, Any] = {}
    for document in sorted(files("beacon_mcp").joinpath("contracts").iterdir(), key=str):
        if not document.name.endswith("-tool-definitions.json"):
            continue
        decoded: object = json.loads(document.read_text())
        contracts = json_object(decoded)
        if contracts is None:
            raise ValueError(f"invalid tool contract document: {document.name}")
        result.update(contracts)
    return result


CONTRACTS = load_contracts()
TOOL_ORDER = json.loads(files("beacon_mcp").joinpath("contracts/tool-order.json").read_text())


def success(value: Any) -> types.CallToolResult:
    text = "Beacon request succeeded."
    if isinstance(value, dict):
        document = cast(dict[str, object], value)
        items = document.get("items")
        if isinstance(items, list):
            text = f"Beacon returned {len(cast(list[object], items))} item(s)."
    return types.CallToolResult(
        content=[types.TextContent(type="text", text=text)],
        structured_content=value,
        is_error=False,
    )


def tool_error(text: str, details: dict[str, Any] | None = None) -> types.CallToolResult:
    content: list[types.ContentBlock] = [types.TextContent(type="text", text=text)]
    if details is None:
        return types.CallToolResult(content=content, is_error=True)
    return types.CallToolResult(
        content=content, is_error=True, structured_content={"error": details}
    )


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
    if error.status is not None:
        details["status"] = error.status
    if error.upstream_code is not None:
        details["upstreamCode"] = error.upstream_code
    if error.retry_after is not None:
        details["retryAfter"] = error.retry_after
    return tool_error(error.message, details)


class Registry:
    """Connect frozen public tool schemas to their application handlers."""

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
            # jsonschema's public validate method is incompletely annotated in its stubs.
            validate = cast(Callable[[object], None], self.validators[name].validate)
            validate(arguments)
        except ValidationError as error:
            return tool_error(f"Invalid arguments: {error.message}")
        try:
            normalized = normalize_arguments(name, arguments)
            result = await self.handlers[name](client, normalized)
            return success(result)
        except GatewayError as error:
            return upstream_error(error)


def normalize_arguments(name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    """Convert JSON Schema integer-valued numbers after schema validation.

    JSON Schema accepts 1.0 as an integer. Handlers need Python integers for
    slicing and cursors, so normalize both scalar and array arguments here.
    Keep a separate dictionary because some handlers rewrite filter names.
    """
    rules = CONTRACTS[name]["inputSchema"]["properties"]
    normalized: dict[str, Any] = {}
    for key, value in arguments.items():
        rule = rules[key]
        if rule.get("type") == "integer":
            normalized[key] = INTEGER.validate_python(value)
        elif rule.get("items", {}).get("type") == "integer":
            normalized[key] = INTEGERS.validate_python(value)
        else:
            normalized[key] = value
    return normalized


def default_registry() -> Registry:
    # Import after Registry is defined: handler modules reference its type.
    from beacon_mcp.handlers import (
        activity,
        analytics,
        composite,
        core,
        details,
        packet,
        routing,
        statistics,
    )

    registry = Registry()
    for module in (core, packet, routing, details, activity, statistics, analytics, composite):
        module.register(registry)
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

    from beacon_mcp import prompts, resources

    resources.register(server, client)
    prompts.register(server)
    return server
