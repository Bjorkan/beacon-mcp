import { z } from "zod/v4";

const optionalString = z.string().nullish();
const optionalNumber = z.number().nullish();
const optionalInteger = z.number().int().nullish();
const epochMs = z
  .number()
  .int()
  .nullish()
  .describe("Unix epoch milliseconds (UTC)");
const optionalBoolean = z.boolean().nullish();
const optionalStrings = z.array(z.string()).nullish();
const serverInfo = z.looseObject({
  minAppVersion: z
    .string()
    .regex(/^\d+\.\d+\.\d+$/)
    .nullable()
    .optional(),
  serverVersion: z
    .string()
    .regex(/^\d+\.\d+\.\d+$/)
    .optional(),
});
const payloadTypeName = z
  .enum([
    "request",
    "response",
    "text_message",
    "acknowledgement",
    "advert",
    "group_text",
    "group_data",
    "anonymous_request",
    "path",
    "trace",
    "multipart",
    "control",
    "reserved",
    "raw_custom",
    "unknown",
  ])
  .nullish();

const nodeIata = z.looseObject({
  iata: optionalString,
  lastHeard: epochMs,
});

const resolvedNode = z.looseObject({
  id: optionalString,
  latitude: optionalNumber,
  longitude: optionalNumber,
  name: optionalString,
  publicKey: optionalString,
});

const resolvedHop = z.looseObject({
  confidence: optionalString,
  nodes: z.array(resolvedNode).nullish(),
  snr: optionalNumber,
});

const routeHop = z.looseObject({
  hashBytes: optionalString,
  node: resolvedNode.nullish(),
  nodeId: optionalString,
});

const packetPathLength = z.looseObject({
  hashSize: optionalInteger,
  hopCount: optionalInteger,
  raw: optionalString,
});

const packetLatestObserver = z.looseObject({
  displayName: optionalString,
  iata: optionalString,
  id: optionalString,
  pathBytes: optionalString,
  pathLength: packetPathLength.nullish(),
  resolvedDestination: resolvedHop.nullish(),
  resolvedPath: z.array(resolvedHop).nullish(),
  resolvedSource: resolvedHop.nullish(),
});

const iataArea = z
  .looseObject({
    displayName: optionalString,
    iata: optionalString,
    lat: optionalNumber,
    lon: optionalNumber,
  })
  .describe(
    "Beacon geographic area identified by an IATA-style code; it is a network partition and not necessarily an airport",
  );

const regionSummary = z.looseObject({
  id: optionalInteger,
  name: optionalString,
  slug: optionalString,
});

const nodeSummary = z.looseObject({
  iatas: z.array(nodeIata).nullish(),
  id: optionalString,
  isObserver: optionalBoolean,
  knownNeighborCount: optionalInteger,
  lat: optionalNumber,
  lng: optionalNumber,
  name: optionalString,
  nodeType: optionalInteger,
  nodeTypeName: optionalString,
  observerId: optionalString,
  possiblyForeign: optionalBoolean,
  publicKey: optionalString,
  radio: optionalString,
  stale: optionalBoolean,
});

const nodeNeighbor = z.looseObject({
  firstSeen: epochMs,
  iata: optionalString,
  id: optionalString,
  lastSeen: epochMs,
  lat: optionalNumber,
  lng: optionalNumber,
  name: optionalString,
  nodeType: optionalInteger,
  nodeTypeName: optionalString,
  observationCount: optionalInteger,
  publicKey: optionalString,
  snr: optionalNumber,
});

const node = z.looseObject({
  defaultScope: optionalString,
  iatas: z.array(nodeIata).nullish(),
  id: optionalString,
  isObserver: optionalBoolean,
  knownNeighborCount: optionalInteger,
  lat: optionalNumber,
  lng: optionalNumber,
  name: optionalString,
  nodeType: optionalInteger,
  nodeTypeName: optionalString,
  observerId: optionalString,
  possiblyForeign: optionalBoolean,
  publicKey: optionalString,
  radio: optionalString,
  stale: optionalBoolean,
  supportsMultibytePaths: optionalBoolean,
  supportsMultibyteTraces: optionalBoolean,
  clockCheckedAt: epochMs,
  clockDriftSeconds: optionalNumber,
  clockOutOfSync: optionalBoolean,
  firstSeen: epochMs,
  lastAdvertAt: epochMs,
  lastSeen: epochMs,
  locationSource: optionalString,
  metadata: z.unknown().nullish(),
  minFirmwareVersion: optionalString,
  neighbors: z.array(nodeNeighbor).nullish(),
});

const observerSummary = z.looseObject({
  displayName: optionalString,
  iata: optionalString,
  id: optionalString,
  observerType: optionalString,
  radio: optionalString,
  scopes: optionalStrings,
  status: optionalString,
});

const observerBroker = z.looseObject({
  lastPacketAt: epochMs,
  lastSeenAt: epochMs,
  name: optionalString,
});

const observer = z.looseObject({
  ...observerSummary.shape,
  batteryLevel: optionalNumber,
  brokers: z.array(observerBroker).nullish(),
  firmwareBuild: optionalString,
  firmwareVersion: optionalString,
  firstSeen: epochMs,
  hardwareModel: optionalString,
  lastSeen: epochMs,
  lastStatusAt: epochMs,
  observationCount: optionalInteger,
  publicKey: optionalString,
  radioBwKhz: optionalNumber,
  radioCr: optionalInteger,
  radioFreqMhz: optionalNumber,
  radioSf: optionalInteger,
  softwareVersion: optionalString,
  statusMetadata: z.unknown().nullish(),
  uptimeSeconds: optionalInteger,
});

const payloadBreakdownItem = z.looseObject({
  count: optionalInteger,
  payloadType: optionalInteger,
  payloadTypeName,
});

const observerActivityPoint = z.looseObject({
  airtimeMs: optionalNumber,
  observations: optionalInteger,
  rssiAvg: optionalNumber,
  snrAvg: optionalNumber,
  snrMin: optionalNumber,
  t: epochMs,
});

const observerActivityRadio = z.looseObject({
  bwKhz: optionalNumber,
  cr: optionalInteger,
  freqMhz: optionalNumber,
  preambleSymbols: optionalInteger,
  sf: optionalInteger,
});

const observerActivitySummary = z.looseObject({
  lastCompleteHour: optionalInteger.describe(
    "Upstream field name is misleading: this value is the observation count for the latest complete hour, not an hour number or timestamp",
  ),
  lastCompleteHourEnd: epochMs,
  lastCompleteHourStart: epochMs,
  latestRecordedAt: epochMs,
  recordedPackets: optionalInteger,
});

const observerActivity = z.looseObject({
  generatedAt: epochMs,
  interval: optionalString,
  payloadTypes: z.array(payloadBreakdownItem).nullish(),
  points: z.array(observerActivityPoint).nullish(),
  radio: observerActivityRadio.nullish(),
  range: optionalString,
  rawFrom: epochMs,
  rolledUntil: epochMs,
  source: optionalString,
  summary: observerActivitySummary.nullish(),
  windowEnd: epochMs,
  windowStart: epochMs,
});

const packetSummary = z.looseObject({
  firstHeardAt: epochMs,
  lastHeardAt: epochMs,
  latestObserver: packetLatestObserver.nullish(),
  observationCount: optionalInteger,
  packetHash: optionalString,
  payloadType: optionalInteger,
  payloadTypeName,
  routeType: optionalInteger,
  routeTypeName: optionalString,
  scope: optionalString,
  summary: optionalString,
});

const packetHeader = z.looseObject({
  payloadType: optionalInteger,
  payloadTypeName,
  payloadVersion: optionalInteger,
  raw: optionalString,
  routeType: optionalInteger,
  routeTypeName: optionalString,
});

const packetRadio = z.looseObject({
  bandwidthKhz: optionalNumber,
  codingRate: optionalInteger,
  freqMhz: optionalNumber,
  spreadFactor: optionalInteger,
});

const packetObservation = z.looseObject({
  heardAt: epochMs,
  iata: optionalString,
  id: optionalInteger,
  observerId: optionalString,
  observerName: optionalString,
  pathBytes: optionalString,
  pathLength: packetPathLength.nullish(),
  propagationTimeMs: optionalNumber,
  radio: packetRadio.nullish(),
  resolvedDestination: resolvedHop.nullish(),
  resolvedPath: z.array(resolvedHop).nullish(),
  resolvedSource: resolvedHop.nullish(),
  rssi: optionalNumber,
  snr: optionalNumber,
  sourceBroker: optionalString,
});

const packet = z.looseObject({
  channelHash: optionalString,
  decrypted: optionalBoolean,
  firstHeardAt: epochMs,
  firstToLastMs: optionalNumber.describe(
    "Span between the earliest and latest observer-reported heardAt across the packet's observations, using observer device clocks. It is not the difference between the server-side firstHeardAt and lastHeardAt receive times; absent when the packet has fewer than two observations",
  ),
  header: packetHeader.nullish(),
  lastHeardAt: epochMs,
  observationCount: optionalInteger,
  observations: z.array(packetObservation).nullish(),
  originPubkey: optionalString,
  packetHash: optionalString,
  parsedPayload: z.unknown().nullish(),
  rawPayload: optionalString,
  resolvedRoute: z.array(resolvedHop).nullish(),
  scope: optionalString,
  transportCodes: z
    .looseObject({
      regionCode: optionalInteger,
      subRegionCode: optionalInteger,
    })
    .nullish(),
});

const channelMessage = z.looseObject({
  channelHash: optionalString,
  content: optionalString,
  id: optionalInteger,
  observationCount: optionalInteger,
  packetHash: optionalString,
  scope: optionalString,
  scopeStatus: z
    .enum(["matched", "unscoped", "unknown", "unavailable"])
    .nullish(),
  senderName: optionalString,
  sentAt: epochMs,
});

const channelSummary = z.looseObject({
  channelHash: optionalString,
  id: optionalInteger,
  isHashtag: optionalBoolean,
  keyKnown: optionalBoolean,
  lastSeen: epochMs,
  name: optionalString,
});

const knownRoute = z.looseObject({
  firstSeen: epochMs,
  hopCount: optionalInteger,
  hops: z.array(routeHop).nullish(),
  iata: optionalString,
  id: optionalInteger,
  lastSeen: epochMs,
  observationCount: optionalInteger,
  pathKey: optionalString,
});

const crossIataRoute = z.looseObject({
  crossHop: z
    .looseObject({
      fromIata: optionalString,
      fromNode: resolvedNode.nullish(),
      lastSeen: epochMs,
      toIata: optionalString,
      toNode: resolvedNode.nullish(),
    })
    .nullish(),
  sourceSegment: z.array(routeHop).nullish(),
  targetSegment: z.array(routeHop).nullish(),
  totalHops: optionalInteger,
});

const traceSummary = z.looseObject({
  firstHeardAt: epochMs,
  iataCount: optionalInteger,
  lastHeardAt: epochMs,
  packetCount: optionalInteger,
  pathHashes: optionalStrings,
  snrValues: z.array(z.number()).nullish(),
  traceTag: optionalString,
  traceType: z.enum(["TRACE", "PING"]).nullish(),
});

const traceDetail = z.looseObject({
  packets: z
    .array(
      z.looseObject({
        firstHeardAt: epochMs,
        lastHeardAt: epochMs,
        packetHash: optionalString,
        rawPath: z
          .array(
            z.looseObject({
              hash: optionalString,
              snr: optionalNumber,
            }),
          )
          .nullish(),
        resolvedRoute: z.array(resolvedHop).nullish(),
        routeType: optionalInteger,
        routeTypeName: optionalString,
        scope: optionalString,
      }),
    )
    .nullish(),
  traceTag: optionalString,
});

const statsOverview = z.looseObject({
  activeIatas: optionalInteger,
  activeObservers: optionalInteger,
  since: epochMs,
  totalObservations: optionalInteger,
  totalPackets: optionalInteger,
  until: epochMs,
  windowHours: optionalInteger,
});

const statsSeriesValues = z.looseObject({
  activeIatas: optionalInteger,
  activeObservers: optionalInteger,
  activeScopes: optionalInteger,
  maxPathEntries: optionalInteger,
  observations: optionalInteger,
  rssiSamples: optionalInteger,
  rssiSum: optionalNumber,
  scopedPackets: optionalInteger,
  snrSamples: optionalInteger,
  snrSum: optionalNumber,
  uniquePackets: optionalInteger,
});

const statsSeries = z.looseObject({
  completeHours: optionalInteger,
  earliestComplete: epochMs,
  hours: z
    .array(
      z.looseObject({
        hour: epochMs,
        status: z.enum(["complete", "partial", "missing"]).nullish(),
        values: statsSeriesValues.nullish(),
      }),
    )
    .nullish(),
  revision: optionalInteger,
  since: epochMs,
  summary: statsSeriesValues.nullish(),
  until: epochMs,
});

const observerComparison = z.looseObject({
  both: optionalInteger,
  observerA: optionalString,
  observerB: optionalString,
  onlyA: optionalInteger,
  onlyB: optionalInteger,
  since: epochMs,
  totalPackets: optionalInteger,
  until: epochMs,
});

const numericCursor = z
  .strictObject({
    cursor: z.number().int().nonnegative(),
  })
  .describe("Continuation page cursor; pass the value to the next call as-is");

const routeCursor = z
  .strictObject({
    cursor: z.number().int().nonnegative(),
    cursorId: z.number().int().positive(),
  })
  .describe(
    "Continuation compound cursor; pass both values to the next call as-is",
  );

const traceCursor = z
  .strictObject({
    cursor: z.number().int().nonnegative(),
    cursorTag: z.string(),
  })
  .describe(
    "Continuation compound cursor; pass both values to the next call as-is",
  );

const channelCursor = z
  .strictObject({
    pageCursor: z.string(),
  })
  .describe(
    "Continuation opaque page cursor; pass the value to the next call as-is",
  );

function finitePage<T extends z.ZodType>(item: T) {
  return z.strictObject({
    items: z.array(item),
    pagination: z.strictObject({
      hasMore: z.boolean(),
      truncated: z.boolean().optional(),
    }),
  });
}

function cursorPage<T extends z.ZodType, C extends z.ZodType>(
  item: T,
  cursor: C,
) {
  return z.strictObject({
    items: z.array(item),
    pagination: z.strictObject({
      hasMore: z.boolean(),
      nextCursor: cursor.optional(),
    }),
  });
}

export const outputSchemas = {
  beacon_get_server_info: serverInfo,
  beacon_list_iatas: finitePage(iataArea),
  beacon_list_regions: finitePage(regionSummary),
  beacon_list_scopes: cursorPage(z.string(), numericCursor),
  beacon_search_nodes: cursorPage(nodeSummary, numericCursor),
  beacon_get_node: node,
  beacon_search_observers: cursorPage(observerSummary, numericCursor),
  beacon_get_observer: observer,
  beacon_get_observer_activity: observerActivity,
  beacon_search_packets: cursorPage(packetSummary, numericCursor),
  beacon_get_packet: packet,
  beacon_search_messages: cursorPage(channelMessage, numericCursor),
  beacon_list_channels: cursorPage(channelSummary, channelCursor),
  beacon_get_channel_messages: cursorPage(channelMessage, numericCursor),
  beacon_list_routes: cursorPage(knownRoute, routeCursor),
  beacon_search_routes: finitePage(knownRoute),
  beacon_find_cross_iata_routes: finitePage(crossIataRoute),
  beacon_search_traces: cursorPage(traceSummary, traceCursor),
  beacon_get_trace: traceDetail,
  beacon_get_network_overview: statsOverview,
  beacon_get_network_series: statsSeries,
  beacon_compare_observers: observerComparison,
} as const;
