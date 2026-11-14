"""The fixed allowlist of public Beacon GET endpoints and their timeout class."""

from enum import Enum


class Operation(Enum):
    """Each value contains an endpoint, identifier kind and analytics-timeout flag."""

    ServerInfo = ("info", "", False)
    WsTicket = ("ws-ticket", "", False)
    Iatas = ("iatas", "", False)
    Regions = ("regions", "", False)
    Scopes = ("scopes", "", False)
    Nodes = ("nodes", "", False)
    Node = ("nodes", "uuid", False)
    Observers = ("observers", "", False)
    Observer = ("observers", "uuid", False)
    ObserverActivity = ("observers", "uuid/activity", False)
    Packets = ("packets", "", False)
    Packet = ("packets", "hash", False)
    Messages = ("messages", "", False)
    Channels = ("channels", "", False)
    ChannelMessages = ("channels", "number/messages", False)
    Routes = ("routes", "", False)
    RouteSearch = ("routes/search", "", False)
    CrossRoutes = ("routes/cross", "", True)
    Traces = ("traces", "", False)
    Trace = ("traces", "tag", False)
    TopAdvertisers = ("stats/top-advertisers", "", False)
    Overview = ("stats/overview", "", True)
    Series = ("stats/series", "", True)
    ObserverComparison = ("stats/observer-comparison", "", True)
    Brokers = ("brokers", "", False)
    ChannelDetail = ("channels", "number", False)
    IataDetail = ("iatas", "iata", False)
    IataBorder = ("iatas", "iata/border", False)
    MessagesBackfill = ("messages/backfill", "", False)
    NodeActivity = ("nodes", "uuid/activity", True)
    NodeNeighbors = ("nodes", "uuid/neighbors", False)
    NodeObservations = ("nodes", "uuid/observations", False)
    ObserverAdverts = ("observers", "uuid/adverts", False)
    ObserverTelemetry = ("observers", "uuid/telemetry", False)
    ObserverDirectory = ("observers/directory", "", True)
    PacketsBackfill = ("packets/backfill", "", False)
    RegionDetail = ("regions", "number", False)
    RouteObservations = ("routes", "route", True)
    ScopeDetail = ("scopes", "scope", False)
    ClockDrift = ("stats/clock-drift", "", True)
    NodeTypes = ("stats/node-types", "", True)
    ObservationStats = ("stats/observations", "", True)
    PathStats = ("stats/paths", "", True)
    PayloadBreakdown = ("stats/payload-breakdown", "", True)
    RadioPresets = ("stats/radio-presets", "", True)
    ScopeStats = ("stats/scopes", "", True)
    SignalStats = ("stats/signal", "", True)
    TopNodes = ("stats/top-nodes", "", True)
    TopObservers = ("stats/top-observers", "", True)
    TopTalkers = ("stats/top-talkers", "", True)


ARRAY_OPERATIONS = frozenset(
    {
        Operation.Iatas,
        Operation.Regions,
        Operation.RouteSearch,
        Operation.CrossRoutes,
        Operation.TopAdvertisers,
        Operation.Brokers,
        Operation.MessagesBackfill,
        Operation.NodeNeighbors,
        Operation.PacketsBackfill,
        Operation.ClockDrift,
        Operation.NodeTypes,
        Operation.ObservationStats,
        Operation.PayloadBreakdown,
        Operation.RadioPresets,
        Operation.ScopeStats,
        Operation.TopNodes,
        Operation.TopObservers,
        Operation.TopTalkers,
    }
)
