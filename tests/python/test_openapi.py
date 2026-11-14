"""Verify all public tool query arguments against the pinned Beacon Swagger."""

import importlib
from pathlib import Path
from typing import cast

import yaml
from pydantic import TypeAdapter

from beacon_mcp.client import Operation
from beacon_mcp.runtime import CONTRACTS

OBJECT = TypeAdapter(dict[str, object])
PARAMETERS = TypeAdapter(list[dict[str, object]])


def swagger_path(op: Operation) -> str:
    endpoint, kind, _ = op.value
    if not kind:
        return "/" + endpoint
    key, _, suffix = kind.partition("/")
    if key == "route":
        return "/routes/{iata}/{pathKey}/observations"
    parameter = {
        "uuid": "nodeId" if endpoint == "nodes" else "observerId",
        "hash": "packetHash",
        "tag": "tag",
        "number": "channelID" if endpoint == "channels" else "regionId",
        "iata": "iata",
        "scope": "name",
    }[key]
    return f"/{endpoint}/{{{parameter}}}" + ("/" + suffix if suffix else "")


def test_all_49_public_get_operations_match_pinned_swagger_queries() -> None:
    swagger = OBJECT.validate_python(yaml.safe_load(Path("vendor/beacon-openapi.yaml").read_text()))
    paths = OBJECT.validate_python(swagger["paths"])
    covered: set[Operation] = set()
    for module in ("core", "packet", "routing", "details", "activity", "statistics", "analytics"):
        raw_operations = getattr(importlib.import_module(f"beacon_mcp.handlers.{module}"), "OPERATIONS")
        operations = OBJECT.validate_python(raw_operations)
        for name, raw_operation in operations.items():
            op = (
                cast(tuple[object, ...], raw_operation)[0]
                if isinstance(raw_operation, tuple)
                else raw_operation
            )
            assert isinstance(op, Operation)
            path = swagger_path(op)
            endpoint = OBJECT.validate_python(paths[path])
            get_operation = OBJECT.validate_python(endpoint["get"])
            assert op not in covered
            covered.add(op)
            properties = OBJECT.validate_python(CONTRACTS[name]["inputSchema"]["properties"])
            for parameter in PARAMETERS.validate_python(get_operation.get("parameters", [])):
                if parameter.get("in") != "query":
                    continue
                key = parameter.get("name")
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
