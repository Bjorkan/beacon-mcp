"""Seven detail and directory tools."""

from beacon_mcp.handlers.extra_support import ResponseShape, register_operations
from beacon_mcp.mcp.runtime import Registry
from beacon_mcp.upstream.operations import Operation

OPERATIONS: dict[str, tuple[Operation, ResponseShape]] = {
    "beacon_list_brokers": (Operation.Brokers, "array"),
    "beacon_get_channel": (Operation.ChannelDetail, "object"),
    "beacon_get_iata": (Operation.IataDetail, "object"),
    "beacon_get_iata_border": (Operation.IataBorder, "object"),
    "beacon_list_observer_directory": (Operation.ObserverDirectory, "page"),
    "beacon_get_region": (Operation.RegionDetail, "object"),
    "beacon_get_scope": (Operation.ScopeDetail, "object"),
}


def register(registry: Registry) -> None:
    register_operations(registry, OPERATIONS)
