import {
  McpServer,
  type CallToolResult,
  type ServerContext,
  type ToolCallback,
} from "@modelcontextprotocol/server";
import { z } from "zod/v4";
import type { BeaconAdapter } from "../beacon/adapter.js";
import { BeaconError, BeaconInputError } from "../beacon/errors.js";
import { PAYLOAD_TYPE_FILTER_NAMES } from "../beacon/payload.js";
import { BEACON_API_VERSION } from "../beacon/version.js";
import type { Logger } from "../logging/logger.js";
import { toolError, toolResult } from "./result.js";

const MAX_INT32 = 2_147_483_647;
const shortText = z.string().trim().min(1).max(128);
const nameText = z.string().trim().min(1).max(256);
const hexString = z
  .string()
  .max(256)
  .regex(/^[0-9a-fA-F]+$/);
const packetHash = z
  .string()
  .regex(/^[0-9a-fA-F]{16}$/)
  .describe("Exact 8-byte Beacon packet hash as 16 hexadecimal characters");
const traceTag = z
  .string()
  .regex(/^[0-9a-fA-F]{8}$/)
  .describe("Exact 4-byte trace tag as 8 hexadecimal characters");
const nodePublicKey = z
  .string()
  .regex(/^[0-9a-fA-F]{64}$/)
  .describe("Exact 32-byte MeshCore public key as 64 hexadecimal characters");
const nodePublicKeyPrefix = z
  .string()
  .max(64)
  .regex(/^[0-9a-fA-F]+$/)
  .describe("Hexadecimal prefix of a MeshCore public key");
const limit = z
  .number()
  .int()
  .min(1)
  .max(50)
  .default(20)
  .describe("Maximum results, from 1 through 50");
const iata = z
  .string()
  .regex(/^[A-Za-z0-9]{3}$/)
  .describe("Three-character IATA identifier; normalized to uppercase");
const iatas = z
  .array(iata)
  .min(1)
  .max(20)
  .optional()
  .describe("IATA identifiers; normalized to uppercase");
const timestamp = z
  .string()
  .max(24)
  .regex(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/)
  .describe("RFC3339 UTC timestamp, for example 2026-10-04T15:30:00Z");
const observerRangePattern =
  /^(?:(?:[1-9]\d{0,3}|[1-3]\d{4}|4[0-2]\d{3}|43[01]\d{2}|43200)m|(?:[1-9]\d?|[1-6]\d{2}|7[01]\d|720)h)$/;
const shortIntervalRangePattern =
  /^(?:(?:[1-9]\d{0,2}|1\d{3}|2[0-7]\d{2}|28[0-7]\d|2880)m|(?:[1-9]|[1-3]\d|4[0-8])h)$/;
const observerRange = z
  .string()
  .regex(observerRangePattern)
  .describe(
    "Positive whole minutes or hours, at most 720h (43200m); combined values such as 1h30m are not supported",
  );
const observerInterval = z.enum(["5m", "15m", "1h", "6h", "24h"]);
const location = {
  iatas,
  region: shortText.optional(),
  regionId: z.number().int().positive().max(MAX_INT32).optional(),
};
const scopedLocation = {
  ...location,
  scope: shortText.optional(),
};
const pagination = {
  cursor: z.number().int().nonnegative().optional(),
  limit,
};
const timeWindow = {
  since: timestamp.optional(),
  until: timestamp.optional(),
};
const readOnlyToolAnnotations = {
  readOnlyHint: true,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: true,
} as const;

const mutuallyExclusive = (left: string, right: string) => ({
  not: { required: [left, right] },
});

const regionLocationRule = mutuallyExclusive("region", "regionId");

function register<S extends z.ZodType<Record<string, unknown>>>(
  server: McpServer,
  logger: Logger,
  name: string,
  description: string,
  schema: S,
  run: (args: z.output<S>, signal: AbortSignal) => Promise<unknown>,
): void {
  const callback = (async (
    rawArgs: z.output<S>,
    ctx: ServerContext,
  ): Promise<CallToolResult> => {
    try {
      return toolResult(await run(rawArgs, ctx.mcpReq.signal));
    } catch (error) {
      if (ctx.mcpReq.signal.aborted) throw error;
      if (
        !(error instanceof BeaconError) &&
        !(error instanceof BeaconInputError)
      ) {
        logger.error(
          { error: error instanceof Error ? error.message : "unknown" },
          "Unexpected MCP tool failure",
        );
      }
      return toolError(error);
    }
  }) as unknown as ToolCallback<S>;
  server.registerTool(
    name,
    {
      description,
      inputSchema: schema,
      annotations: readOnlyToolAnnotations,
    },
    callback,
  );
}

export function createMcpServer(
  adapter: BeaconAdapter,
  logger: Logger,
): McpServer {
  const server = new McpServer(
    { name: "beacon-mcp", version: BEACON_API_VERSION },
    { capabilities: { tools: {} } },
  );

  register(
    server,
    logger,
    "beacon_list_iatas",
    "List Beacon geographic areas identified by IATA-style codes. Codes represent network partitions and do not necessarily refer to the corresponding airport.",
    z.strictObject({ limit }),
    (a, s) => adapter.listIatas(a.limit, s),
  );
  register(
    server,
    logger,
    "beacon_list_regions",
    "List configured Beacon geographic regions.",
    z.strictObject({ limit }),
    (a, s) => adapter.listRegions(a.limit, s),
  );
  register(
    server,
    logger,
    "beacon_list_scopes",
    "List public MeshCore transport scopes.",
    z
      .strictObject({ ...location, ...pagination })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .meta(regionLocationRule),
    (a, s) => adapter.listScopes(a, s),
  );

  register(
    server,
    logger,
    "beacon_search_nodes",
    "Search known MeshCore nodes using bounded filters.",
    z
      .strictObject({
        ...scopedLocation,
        ...pagination,
        name: nameText.optional(),
        type: z.number().int().min(1).max(4).optional(),
        typeName: z
          .enum(["companion", "repeater", "room_server", "sensor"])
          .optional(),
        pubkey: nodePublicKey.optional(),
        pubkeyPrefix: nodePublicKeyPrefix.optional(),
        supportsMultibytePaths: z.boolean().optional(),
        supportsMultibyteTraces: z.boolean().optional(),
      })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .refine((a) => !(a.type && a.typeName), {
        message: "type and typeName are mutually exclusive",
      })
      .meta({
        allOf: [regionLocationRule, mutuallyExclusive("type", "typeName")],
      }),
    (a, s) => adapter.searchNodes(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_node",
    "Get one node by its Beacon node UUID.",
    z.strictObject({ id: z.uuid() }),
    (a, s) => adapter.getNode(a.id, s),
  );

  register(
    server,
    logger,
    "beacon_search_observers",
    "Search Beacon observers/gateways.",
    z
      .strictObject({
        ...scopedLocation,
        ...pagination,
        type: shortText.optional(),
        broker: shortText.optional(),
        status: z.enum(["online", "offline"]).optional(),
        name: nameText.optional(),
      })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .meta(regionLocationRule),
    (a, s) => adapter.searchObservers(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_observer",
    "Get one observer by UUID.",
    z.strictObject({ id: z.uuid() }),
    (a, s) => adapter.getObserver(a.id, s),
  );
  register(
    server,
    logger,
    "beacon_get_observer_activity",
    "Get heard-activity history for one observer.",
    z
      .strictObject({
        id: z.uuid(),
        range: observerRange.optional(),
        interval: observerInterval.optional(),
        until: timestamp.optional(),
      })
      .refine(
        (a) => {
          if (!a.range || !["5m", "15m"].includes(a.interval ?? "15m"))
            return true;
          const amount = Number.parseInt(a.range, 10);
          const minutes = a.range.endsWith("h") ? amount * 60 : amount;
          return minutes <= 48 * 60;
        },
        { message: "range must be 48h or less for intervals under 1h" },
      )
      .meta({
        allOf: [
          {
            if: {
              properties: {
                interval: { enum: ["1h", "6h", "24h"] },
              },
              required: ["interval"],
            },
            else: {
              properties: {
                range: { pattern: shortIntervalRangePattern.source },
              },
            },
          },
        ],
      }),
    (a, s) => adapter.getObserverActivity(a.id, a, s),
  );

  register(
    server,
    logger,
    "beacon_search_packets",
    "Search packet summaries. Beacon MCP Server does not persist or log packet payloads; upstream Beacon may store and return them through beacon_get_packet.",
    z
      .strictObject({
        ...scopedLocation,
        ...timeWindow,
        ...pagination,
        payloadType: z.number().int().min(0).max(32_767).optional(),
        payloadTypes: z
          .array(z.number().int().min(0).max(32_767))
          .min(1)
          .max(20)
          .optional(),
        payloadTypeName: z
          .enum(PAYLOAD_TYPE_FILTER_NAMES)
          .optional()
          .describe(
            "Canonical payload type name; txt_msg, grp_txt, and anon_req remain supported as legacy aliases",
          ),
        routeType: z.number().int().min(0).max(3).optional(),
        routeTypes: z
          .array(z.number().int().min(0).max(3))
          .min(1)
          .max(20)
          .optional(),
        scopes: z.array(shortText).min(1).max(20).optional(),
      })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .refine((a) => !(a.payloadType !== undefined && a.payloadTypes), {
        message: "payloadType and payloadTypes are mutually exclusive",
      })
      .refine(
        (a) =>
          !(
            a.payloadTypeName !== undefined &&
            (a.payloadType !== undefined || a.payloadTypes !== undefined)
          ),
        {
          message:
            "payloadTypeName cannot be combined with payloadType or payloadTypes",
        },
      )
      .refine((a) => !(a.routeType !== undefined && a.routeTypes), {
        message: "routeType and routeTypes are mutually exclusive",
      })
      .refine((a) => !(a.scope !== undefined && a.scopes), {
        message: "scope and scopes are mutually exclusive",
      })
      .meta({
        allOf: [
          regionLocationRule,
          mutuallyExclusive("payloadType", "payloadTypes"),
          mutuallyExclusive("payloadTypeName", "payloadType"),
          mutuallyExclusive("payloadTypeName", "payloadTypes"),
          mutuallyExclusive("routeType", "routeTypes"),
          mutuallyExclusive("scope", "scopes"),
        ],
      }),
    (a, s) => adapter.searchPackets(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_packet",
    "Get full public packet detail by hex packet hash. Packet firstHeardAt/lastHeardAt are Beacon server receive times; observations[].heardAt is observer-reported time and may differ.",
    z.strictObject({ hash: packetHash }),
    (a, s) => adapter.getPacket(a.hash, s),
  );

  register(
    server,
    logger,
    "beacon_search_messages",
    "Search decrypted public channel messages.",
    z
      .strictObject({
        iatas,
        scope: shortText.optional(),
        since: timestamp.optional(),
        ...pagination,
        channelId: z.number().int().positive().max(MAX_INT32).optional(),
        channelHash: z
          .string()
          .regex(/^[0-9a-fA-F]{2}$/)
          .optional(),
      })
      .refine(
        (a) => !(a.channelId && a.channelHash),
        "channelId and channelHash are mutually exclusive",
      )
      .meta(mutuallyExclusive("channelId", "channelHash")),
    (a, s) => adapter.searchMessages(a, s),
  );
  register(
    server,
    logger,
    "beacon_list_channels",
    "List public channels. Continue with the opaque pageCursor returned as pagination.nextCursor.pageCursor.",
    z.strictObject({
      iatas,
      limit,
      hash: z
        .string()
        .regex(/^[0-9a-fA-F]{2}$/)
        .optional(),
      keyKnown: z.boolean().optional(),
      pageCursor: z
        .string()
        .min(1)
        .max(64)
        .optional()
        .describe(
          "Opaque continuation token returned as pagination.nextCursor.pageCursor",
        ),
    }),
    (a, s) => adapter.listChannels(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_channel_messages",
    "List messages for a channel ID.",
    z.strictObject({
      channelId: z.number().int().positive().max(MAX_INT32),
      iatas,
      scope: shortText.optional(),
      since: timestamp.optional(),
      ...pagination,
    }),
    (a, s) => adapter.getChannelMessages(a.channelId, a, s),
  );

  register(
    server,
    logger,
    "beacon_list_routes",
    "List known routes with the upstream compound cursor preserved.",
    z
      .strictObject({
        iata: iata.optional(),
        hopCount: z.number().int().positive().max(MAX_INT32).optional(),
        cursor: z
          .number()
          .int()
          .nonnegative()
          .optional()
          .describe(
            "Continuation timestamp returned as pagination.nextCursor.cursor",
          ),
        cursorId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Continuation route ID returned as pagination.nextCursor.cursorId; requires cursor",
          ),
        limit,
      })
      .refine((a) => a.cursorId === undefined || a.cursor !== undefined, {
        message: "cursorId requires cursor",
      })
      .meta({ dependentRequired: { cursorId: ["cursor"] } }),
    (a, s) => adapter.listRoutes(a, s),
  );
  register(
    server,
    logger,
    "beacon_search_routes",
    "Search routes by source and destination hash.",
    z.strictObject({
      iata,
      from: hexString,
      to: hexString,
      limit,
    }),
    (a, s) => adapter.searchRoutes(a, s),
  );
  register(
    server,
    logger,
    "beacon_find_cross_iata_routes",
    "Find routes crossing IATA boundaries.",
    z.strictObject({
      fromHash: hexString,
      fromIata: iata,
      toHash: hexString,
      toIata: iata,
      limit,
    }),
    (a, s) => adapter.findCrossIataRoutes(a, s),
  );

  register(
    server,
    logger,
    "beacon_search_traces",
    "Search trace or ping tags with the timestamp/tag cursor preserved.",
    z
      .strictObject({
        ...scopedLocation,
        ...timeWindow,
        ...pagination,
        type: z.enum(["TRACE", "PING"]).optional(),
        cursor: z
          .number()
          .int()
          .nonnegative()
          .optional()
          .describe(
            "Continuation timestamp returned as pagination.nextCursor.cursor",
          ),
        cursorTag: traceTag
          .optional()
          .describe(
            "Continuation trace tag returned as pagination.nextCursor.cursorTag; requires cursor",
          ),
      })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .refine((a) => a.cursorTag === undefined || a.cursor !== undefined, {
        message: "cursorTag requires cursor",
      })
      .meta({
        allOf: [regionLocationRule],
        dependentRequired: { cursorTag: ["cursor"] },
      }),
    (a, s) => adapter.searchTraces(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_trace",
    "Get trace detail by hex tag.",
    z.strictObject({ tag: traceTag }),
    (a, s) => adapter.getTrace(a.tag, s),
  );

  register(
    server,
    logger,
    "beacon_get_network_overview",
    "Get the Beacon network overview for the most recent rolled 24 hours.",
    z
      .strictObject(location)
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .meta(regionLocationRule),
    (a, s) => adapter.getNetworkOverview(a, s),
  );
  register(
    server,
    logger,
    "beacon_get_network_series",
    "Get hourly network analytics for an explicit time window.",
    z
      .strictObject({ ...location, since: timestamp, until: timestamp })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .meta(regionLocationRule),
    (a, s) => adapter.getNetworkSeries(a, s),
  );
  register(
    server,
    logger,
    "beacon_compare_observers",
    "Compare distinct flood packets reported by two observers.",
    z
      .strictObject({
        ...location,
        observerA: z.uuid(),
        observerB: z.uuid(),
        since: timestamp,
        until: timestamp,
      })
      .refine((a) => !(a.region && a.regionId), {
        message: "region and regionId are mutually exclusive",
      })
      .refine((a) => a.observerA !== a.observerB, {
        message: "observerA and observerB must be different",
      })
      .meta(regionLocationRule),
    (a, s) => adapter.compareObservers(a, s),
  );

  return server;
}
