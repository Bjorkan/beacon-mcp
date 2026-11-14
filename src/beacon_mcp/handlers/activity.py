"""Node activity, observation cursors, historical backfill and route evidence."""

from beacon_mcp.handlers.extra_support import ResponseShape, register_operations
from beacon_mcp.mcp.runtime import Registry
from beacon_mcp.upstream.operations import Operation

OPERATIONS: dict[str, tuple[Operation, ResponseShape]] = {
    "beacon_backfill_messages": (Operation.MessagesBackfill, "array"),
    "beacon_get_node_activity": (Operation.NodeActivity, "object"),
    "beacon_get_node_neighbors": (Operation.NodeNeighbors, "array"),
    "beacon_get_node_observations": (Operation.NodeObservations, "page"),
    "beacon_get_observer_adverts": (Operation.ObserverAdverts, "page"),
    "beacon_get_observer_telemetry": (Operation.ObserverTelemetry, "object"),
    "beacon_backfill_packets": (Operation.PacketsBackfill, "array"),
    "beacon_get_route_observations": (Operation.RouteObservations, "object"),
}


def register(registry: Registry) -> None:
    register_operations(registry, OPERATIONS)
