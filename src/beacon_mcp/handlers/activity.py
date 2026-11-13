"""Node activity, observation cursors, historical backfill and route evidence."""

from beacon_mcp.client import Operation
from beacon_mcp.extra_support import register_operations
from beacon_mcp.runtime import Registry

OPERATIONS = {
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
