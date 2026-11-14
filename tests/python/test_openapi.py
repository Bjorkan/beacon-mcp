import importlib
from pathlib import Path

import yaml

from beacon_mcp.client import Operation
from beacon_mcp.runtime import CONTRACTS


def swagger_path(op):
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


def test_all_49_public_get_operations_match_pinned_swagger_queries():
    swagger = yaml.safe_load(Path("vendor/beacon-openapi.yaml").read_text())
    covered = set()
    for module in ("core", "packet", "routing", "details", "activity", "statistics", "analytics"):
        for name, value in importlib.import_module(
            f"beacon_mcp.handlers.{module}"
        ).OPERATIONS.items():
            op = value[0] if isinstance(value, tuple) else value
            path = swagger_path(op)
            operation = swagger["paths"][path]["get"]
            assert op not in covered
            covered.add(op)
            properties = CONTRACTS[name]["inputSchema"]["properties"]
            for parameter in operation.get("parameters", []):
                if parameter["in"] != "query":
                    continue
                key = parameter["name"]
                if name in ("beacon_search_messages", "beacon_get_channel_messages") and key in (
                    "region",
                    "regionId",
                ):
                    assert key not in properties
                    continue
                assert ("channelId" if key == "channelID" else key) in properties, (name, key)
    assert len(covered) == 49
    assert covered == set(Operation) - {Operation.WsTicket}
