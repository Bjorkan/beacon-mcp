"""Verify all public tool query arguments against the pinned Beacon Swagger."""

import importlib
from pathlib import Path
from typing import cast

import yaml
from pydantic import TypeAdapter

from beacon_mcp.client import Operation
from beacon_mcp.runtime import CONTRACTS

OBJECT: TypeAdapter[dict[str, object]] = TypeAdapter(dict[str, object])
PARAMETERS: TypeAdapter[list[dict[str, object]]] = TypeAdapter(list[dict[str, object]])


def swagger_path(op: Operation) -> str:
    endpoint: str
    kind: str
    _: bool | str
    key: str
    suffix: str
    endpoint, kind, _ = op.value
    if not kind:
        return "/" + endpoint
    key, _, suffix = kind.partition("/")
    if key == "route":
        return "/routes/{iata}/{pathKey}/observations"
    parameter: str = {
        "uuid": "nodeId" if endpoint == "nodes" else "observerId",
        "hash": "packetHash",
        "tag": "tag",
        "number": "channelID" if endpoint == "channels" else "regionId",
        "iata": "iata",
        "scope": "name",
    }[key]
    return f"/{endpoint}/{{{parameter}}}" + ("/" + suffix if suffix else "")


def test_all_49_public_get_operations_match_pinned_swagger_queries() -> None:
    module: str
    name: str
    raw_operation: object
    parameter: dict[str, object]
    swagger: dict[str, object] = OBJECT.validate_python(
        yaml.safe_load(Path("vendor/beacon-openapi.yaml").read_text())
    )
    paths: dict[str, object] = OBJECT.validate_python(swagger["paths"])
    covered: set[Operation] = set()
    for module in ("core", "packet", "routing", "details", "activity", "statistics", "analytics"):
        raw_operations: object = importlib.import_module(f"beacon_mcp.handlers.{module}").OPERATIONS
        operations: dict[str, object] = OBJECT.validate_python(raw_operations)
        for name, raw_operation in operations.items():
            op: object = (
                cast(tuple[object, ...], raw_operation)[0]
                if isinstance(raw_operation, tuple)
                else raw_operation
            )
            assert isinstance(op, Operation)
            path: str = swagger_path(op)
            endpoint: dict[str, object] = OBJECT.validate_python(paths[path])
            get_operation: dict[str, object] = OBJECT.validate_python(endpoint["get"])
            assert op not in covered
            covered.add(op)
            properties: dict[str, object] = OBJECT.validate_python(
                CONTRACTS[name]["inputSchema"]["properties"]
            )
            for parameter in PARAMETERS.validate_python(get_operation.get("parameters", [])):
                if parameter.get("in") != "query":
                    continue
                key: object = parameter.get("name")
                assert isinstance(key, str)
                if name in ("beacon_search_messages", "beacon_get_channel_messages") and key in (
                    "region",
                    "regionId",
                ):
                    assert key not in properties
                    continue
                assert ("channelId" if key == "channelID" else key) in properties, (name, key)
    assert len(covered) == 49
    assert covered == set(Operation) - {Operation.WsTicket}
