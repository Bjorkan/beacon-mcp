"""Eleven public statistics tools using the analytics timeout and array bounds."""

from beacon_mcp.handlers.extra_support import ResponseShape, register_operations
from beacon_mcp.mcp.runtime import Registry
from beacon_mcp.upstream.operations import Operation

OPERATIONS: dict[str, tuple[Operation, ResponseShape]] = {
    "beacon_get_clock_drift": (Operation.ClockDrift, "array"),
    "beacon_get_node_types": (Operation.NodeTypes, "array"),
    "beacon_get_observation_stats": (Operation.ObservationStats, "array"),
    "beacon_get_path_stats": (Operation.PathStats, "object"),
    "beacon_get_payload_breakdown": (Operation.PayloadBreakdown, "array"),
    "beacon_get_radio_presets": (Operation.RadioPresets, "array"),
    "beacon_get_scope_stats": (Operation.ScopeStats, "array"),
    "beacon_get_signal_stats": (Operation.SignalStats, "object"),
    "beacon_get_top_nodes": (Operation.TopNodes, "array"),
    "beacon_get_top_observers": (Operation.TopObservers, "array"),
    "beacon_get_top_talkers": (Operation.TopTalkers, "array"),
}


def register(registry: Registry) -> None:
    register_operations(registry, OPERATIONS)
