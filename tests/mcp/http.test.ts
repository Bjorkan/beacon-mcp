import { afterEach, describe, expect, it, vi } from "vitest";
import type { FastifyInstance } from "fastify";
import { BeaconAdapter } from "../../src/beacon/adapter.js";
import { BeaconClient } from "../../src/beacon/client.js";
import type { Config } from "../../src/config/config.js";
import { createLogger } from "../../src/logging/logger.js";
import { buildHttpServer } from "../../src/server/http.js";

const apps: FastifyInstance[] = [];
const config: Config = Object.freeze({
  host: "127.0.0.1",
  port: 3000,
  beaconBaseUrl: new URL("https://beacon.example/"),
  beaconTimeoutMs: 1000,
  beaconStatsTimeoutMs: 1000,
  logLevel: "silent",
  shutdownGraceMs: 1000,
});

interface StartOptions {
  config?: Partial<Config>;
  fetchImpl?: typeof fetch;
}

interface ListedTool {
  name: string;
  description?: string;
  inputSchema: Record<string, unknown>;
  outputSchema?: Record<string, unknown>;
}

async function start(options: StartOptions = {}): Promise<FastifyInstance> {
  const defaultFetch = vi.fn<typeof fetch>(async (input) => {
    const path = new URL(String(input)).pathname;
    if (path.endsWith("/nodes"))
      return Response.json({
        items: [{ id: "n1", name: "Node" }],
        hasMore: false,
      });
    if (path.includes("/packets/"))
      return Response.json({ packetHash: "abcd", observationCount: 1 });
    if (path.endsWith("/messages"))
      return Response.json({
        items: [{ id: 1, content: "hello" }],
        hasMore: false,
      });
    if (path.endsWith("/routes"))
      return Response.json([{ id: 1, lastSeen: 1_000 }]);
    if (path.endsWith("/stats/overview"))
      return Response.json({ packetCount: 42 });
    return Response.json([]);
  });
  const effectiveConfig = {
    ...config,
    ...options.config,
  };
  const app = buildHttpServer(
    effectiveConfig,
    new BeaconAdapter(
      new BeaconClient(effectiveConfig, {
        fetch: options.fetchImpl ?? defaultFetch,
      }),
    ),
    createLogger("silent"),
  );
  apps.push(app);
  await app.ready();
  return app;
}

async function rpc(
  app: FastifyInstance,
  method: string,
  params?: Record<string, unknown>,
) {
  return app.inject({
    url: "/",
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      "mcp-protocol-version": "2026-07-28",
      "mcp-method": method,
      ...(typeof params?.["name"] === "string"
        ? { "mcp-name": params["name"] }
        : {}),
    },
    payload: {
      jsonrpc: "2.0",
      id: 1,
      method,
      params: {
        ...params,
        _meta: {
          "io.modelcontextprotocol/protocolVersion": "2026-07-28",
          "io.modelcontextprotocol/clientInfo": {
            name: "vitest",
            version: "1.0.0",
          },
          "io.modelcontextprotocol/clientCapabilities": {},
        },
      },
    },
  });
}

afterEach(async () => Promise.all(apps.splice(0).map((app) => app.close())));

describe("MCP HTTP integration", () => {
  it("serves health/readiness and discovers only read-only tools", async () => {
    const app = await start();
    const health = await app.inject({ url: "/healthz" });
    expect(health.json()).toEqual({
      status: "ok",
    });
    expect(health.headers["cache-control"]).toBe("no-store");
    expect(health.headers["x-content-type-options"]).toBe("nosniff");
    expect((await app.inject({ url: "/readyz" })).json()).toEqual({
      status: "ok",
    });
    const response = await rpc(app, "tools/list");
    expect(response.statusCode).toBe(200);
    expect(response.headers["cache-control"]).toBe("no-store");
    expect(response.headers["x-content-type-options"]).toBe("nosniff");
    const text = response.body;
    expect(text).toContain("beacon_search_nodes");
    expect(text).toContain("beacon_compare_observers");
    expect(text).not.toContain("admin");
    expect(text.match(/"readOnlyHint":true/g)).toHaveLength(21);
    expect(text.match(/"destructiveHint":false/g)).toHaveLength(21);
    expect(text.match(/"idempotentHint":true/g)).toHaveLength(21);
    expect(text.match(/"openWorldHint":true/g)).toHaveLength(21);
    const listedTools = (response.json() as { result: { tools: ListedTool[] } })
      .result.tools;
    expect(listedTools).toHaveLength(21);
    expect(listedTools.every((tool) => tool.outputSchema !== undefined)).toBe(
      true,
    );
    const listed = (name: string) => {
      const tool = listedTools.find((candidate) => candidate.name === name);
      expect(tool).toBeDefined();
      return tool as ListedTool;
    };
    for (const name of [
      "beacon_search_nodes",
      "beacon_search_observers",
      "beacon_search_packets",
      "beacon_search_messages",
      "beacon_search_traces",
      "beacon_get_network_overview",
      "beacon_get_network_series",
      "beacon_compare_observers",
    ]) {
      const serialized = JSON.stringify(listed(name).inputSchema);
      expect(serialized).not.toContain('"not"');
      expect(serialized).not.toContain('"allOf"');
      expect(serialized).not.toContain('"if"');
      expect(serialized).not.toContain('"dependentRequired"');
    }
    expect(JSON.stringify(listed("beacon_search_nodes").description)).toContain(
      "region and regionId are mutually exclusive",
    );
    expect(JSON.stringify(listed("beacon_search_nodes").description)).toContain(
      "type and typeName",
    );
    expect(
      JSON.stringify(listed("beacon_get_observer_activity").inputSchema),
    ).toContain("43200m");
    expect(
      JSON.stringify(listed("beacon_get_observer_activity").description),
    ).toContain("48h or less");

    const channelsInput = JSON.stringify(
      listed("beacon_list_channels").inputSchema,
    );
    expect(channelsInput).toContain('"pageCursor"');
    expect(channelsInput).not.toContain('"cursor"');

    const listIatasOutput = JSON.stringify(
      listed("beacon_list_iatas").outputSchema,
    );
    expect(listIatasOutput).toContain('"hasMore"');
    expect(listIatasOutput).not.toContain('"nextCursor"');

    const nodesOutput = JSON.stringify(
      listed("beacon_search_nodes").outputSchema,
    );
    expect(nodesOutput).toContain('"pagination"');
    expect(nodesOutput).toContain('"cursor"');
    expect(nodesOutput).not.toContain('"cursorTag"');
    expect(nodesOutput).not.toContain('"pageCursor"');

    const channelsOutput = JSON.stringify(
      listed("beacon_list_channels").outputSchema,
    );
    expect(channelsOutput).toContain('"pageCursor"');
    expect(channelsOutput).not.toContain('"cursorTag"');

    const tracesOutput = JSON.stringify(
      listed("beacon_search_traces").outputSchema,
    );
    expect(tracesOutput).toContain('"cursorTag"');
    expect(tracesOutput).toContain('"cursor"');

    const routesOutput = JSON.stringify(
      listed("beacon_list_routes").outputSchema,
    );
    expect(routesOutput).toContain('"cursorId"');
    expect(routesOutput).toContain('"cursor"');

    expect(JSON.stringify(listed("beacon_get_packet").outputSchema)).toContain(
      '"packetHash"',
    );
    expect(
      JSON.stringify(listed("beacon_search_packets").outputSchema),
    ).toContain('"text_message"');
    expect(listed("beacon_list_iatas").description).toContain(
      "network partitions",
    );
    expect(listed("beacon_search_packets").description).toContain(
      "does not persist or log",
    );
    const discovery = await rpc(app, "server/discover");
    expect(discovery.statusCode, discovery.body).toBe(200);
    expect(discovery.body).toContain('"version":"2.0.2"');
    const modern = await app.inject({
      url: "/",
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        "mcp-protocol-version": "2026-07-28",
        "mcp-method": "tools/list",
      },
      payload: {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/list",
        params: {
          _meta: {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {
              name: "vitest",
              version: "1.0.0",
            },
            "io.modelcontextprotocol/clientCapabilities": {},
          },
        },
      },
    });
    expect(modern.statusCode, modern.body).toBe(200);
    expect(modern.body).toContain("beacon_search_nodes");
  });

  it("calls node and packet tools through the actual handler", async () => {
    const app = await start();
    const nodes = (
      await rpc(app, "tools/call", {
        name: "beacon_search_nodes",
        arguments: { iatas: ["arn"], limit: 20 },
      })
    ).body;
    expect(nodes).toContain("Node");
    const packet = (
      await rpc(app, "tools/call", {
        name: "beacon_get_packet",
        arguments: { hash: "0123456789abcdef" },
      })
    ).body;
    expect(packet).toContain("packetHash");
    const invalidRoute = (
      await rpc(app, "tools/call", {
        name: "beacon_search_routes",
        arguments: { limit: 2 },
      })
    ).body;
    expect(invalidRoute).toContain("Invalid");
  });

  it("calls messages, routes, and analytics through the actual handler", async () => {
    const app = await start();
    const messages = await rpc(app, "tools/call", {
      name: "beacon_search_messages",
      arguments: { limit: 20 },
    });
    expect(messages.body).toContain("hello");

    const routes = await rpc(app, "tools/call", {
      name: "beacon_list_routes",
      arguments: { limit: 1 },
    });
    expect(routes.body).toContain('"cursorId":1');

    const overview = await rpc(app, "tools/call", {
      name: "beacon_get_network_overview",
      arguments: {},
    });
    expect(overview.body).toContain('"packetCount":42');
  });

  it("validates mutually exclusive filters and bounded observer activity", async () => {
    const app = await start();
    const packets = await rpc(app, "tools/call", {
      name: "beacon_search_packets",
      arguments: { payloadType: 1, payloadTypes: [1] },
    });
    expect(packets.body).toContain("mutually exclusive");

    const namedPackets = await rpc(app, "tools/call", {
      name: "beacon_search_packets",
      arguments: { payloadType: 5, payloadTypeName: "group_text" },
    });
    expect(namedPackets.body).toContain("payloadTypeName cannot be combined");

    const channels = await rpc(app, "tools/call", {
      name: "beacon_list_channels",
      arguments: { cursor: 1, pageCursor: "opaque" },
    });
    expect(channels.body).toContain('Unrecognized key: \\"cursor\\"');

    const activity = await rpc(app, "tools/call", {
      name: "beacon_get_observer_activity",
      arguments: {
        id: "00000000-0000-4000-8000-000000000000",
        range: "721h",
        interval: "2h",
      },
    });
    expect(activity.body).toContain("Invalid");

    const expensiveActivity = await rpc(app, "tools/call", {
      name: "beacon_get_observer_activity",
      arguments: {
        id: "00000000-0000-4000-8000-000000000000",
        range: "49h",
        interval: "5m",
      },
    });
    expect(expensiveActivity.body).toContain("48h or less");

    const routeCursor = await rpc(app, "tools/call", {
      name: "beacon_list_routes",
      arguments: { cursorId: 1 },
    });
    expect(routeCursor.body).toContain("cursorId requires cursor");

    const traceCursor = await rpc(app, "tools/call", {
      name: "beacon_search_traces",
      arguments: { cursorTag: "0123abcd" },
    });
    expect(traceCursor.body).toContain("cursorTag requires cursor");
  });

  it("publishes exact identifier constraints and keeps prefix search separate", async () => {
    const app = await start();
    for (const [name, args] of [
      ["beacon_get_packet", { hash: "f" }],
      ["beacon_get_trace", { tag: "e" }],
      ["beacon_search_nodes", { pubkey: "8e" }],
    ] as const) {
      const response = await rpc(app, "tools/call", {
        name,
        arguments: args,
      });
      expect(response.body).toContain("Invalid");
    }

    const prefix = await rpc(app, "tools/call", {
      name: "beacon_search_nodes",
      arguments: { pubkeyPrefix: "8e" },
    });
    expect(prefix.body).toContain("Node");

    for (const payloadTypeName of [
      "group_data",
      "anonymous_request",
      "reserved",
      "grp_txt",
    ]) {
      const response = await rpc(app, "tools/call", {
        name: "beacon_search_packets",
        arguments: { payloadTypeName },
      });
      expect(response.body).not.toContain("Invalid");
    }

    for (const [name, args] of [
      [
        "beacon_search_routes",
        { iata: "ARN", from: "e7", to: "e72b", limit: 1 },
      ],
      [
        "beacon_find_cross_iata_routes",
        {
          fromHash: "e72b",
          fromIata: "ARN",
          toHash: "abcd",
          toIata: "LHR",
          limit: 1,
        },
      ],
    ] as const) {
      const response = await rpc(app, "tools/call", {
        name,
        arguments: args,
      });
      expect(response.body).not.toContain("Invalid");
    }

    for (const [name, args] of [
      [
        "beacon_search_routes",
        { iata: "ARN", from: "e72ba", to: "e72b", limit: 1 },
      ],
      [
        "beacon_find_cross_iata_routes",
        {
          fromHash: "dbg",
          fromIata: "ARN",
          toHash: "e72b",
          toIata: "LHR",
          limit: 1,
        },
      ],
    ] as const) {
      const response = await rpc(app, "tools/call", {
        name,
        arguments: args,
      });
      expect(response.body).toContain("Invalid");
    }
  });

  it("rejects unknown and ambiguous filters instead of broadening queries", async () => {
    const app = await start();
    const unsupportedRegion = await rpc(app, "tools/call", {
      name: "beacon_search_messages",
      arguments: { region: "north" },
    });
    expect(unsupportedRegion.body).toContain("region");
    expect(unsupportedRegion.body).toContain("Invalid");

    const ambiguousRegion = await rpc(app, "tools/call", {
      name: "beacon_search_packets",
      arguments: { region: "north", regionId: 1 },
    });
    expect(ambiguousRegion.body).toContain(
      "region and regionId are mutually exclusive",
    );

    const ambiguousType = await rpc(app, "tools/call", {
      name: "beacon_search_nodes",
      arguments: { type: 2, typeName: "repeater" },
    });
    expect(ambiguousType.body).toContain("type and typeName");
  });

  it("maps upstream 404, 429, 503, and timeout failures", async () => {
    for (const [status, expected] of [
      [404, "BeaconNotFoundError"],
      [429, "BeaconRateLimitError"],
      [503, "BeaconUpstreamError"],
    ] as const) {
      const app = await start({
        fetchImpl: vi.fn<typeof fetch>(async () =>
          Response.json(
            { error: { code: "failure", message: `status ${status}` } },
            { status },
          ),
        ),
      });
      const response = await rpc(app, "tools/call", {
        name: "beacon_get_packet",
        arguments: { hash: "0123456789abcdef" },
      });
      expect(response.body).toContain(expected);
    }

    const timeoutApp = await start({
      config: { beaconTimeoutMs: 100 },
      fetchImpl: vi.fn<typeof fetch>(
        async (_input, init) =>
          new Promise((_resolve, reject) =>
            init?.signal?.addEventListener(
              "abort",
              () => reject(init.signal?.reason),
              { once: true },
            ),
          ),
      ),
    });
    const timeout = await rpc(timeoutApp, "tools/call", {
      name: "beacon_get_packet",
      arguments: { hash: "0123456789abcdef" },
    });
    expect(timeout.body).toContain("BeaconTimeoutError");
  });

  it("does not filter public hosts or origins and serves legacy MCP", async () => {
    const app = await start();
    expect(
      (
        await app.inject({
          url: "/healthz",
          method: "GET",
          headers: { host: "evil.example" },
        })
      ).statusCode,
    ).toBe(200);
    expect(
      (
        await app.inject({
          url: "/healthz",
          method: "GET",
          headers: { origin: "https://evil.example" },
        })
      ).statusCode,
    ).toBe(200);

    const legacy = await app.inject({
      url: "/",
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
      },
      payload: { jsonrpc: "2.0", id: 1, method: "tools/list" },
    });
    expect(legacy.statusCode).toBe(200);
    expect(legacy.body).toContain("beacon_list_iatas");
  });

  it("negotiates legacy and pre-2025 initialize requests statelessly", async () => {
    const app = await start();
    for (const requested of [
      "2025-11-25",
      "2025-06-18",
      "2025-03-26",
      "2024-11-05",
      "2024-10-07",
    ]) {
      const response = await app.inject({
        url: "/",
        method: "POST",
        headers: {
          "content-type": "application/json",
          accept: "application/json, text/event-stream",
          "mcp-protocol-version": requested,
        },
        payload: {
          jsonrpc: "2.0",
          id: 1,
          method: "initialize",
          params: {
            protocolVersion: requested,
            capabilities: {},
            clientInfo: { name: "vitest", version: "1.0.0" },
          },
        },
      });
      expect(response.statusCode).toBe(200);
      expect(response.body).toContain(`"protocolVersion":"${requested}"`);
      expect(response.body).toContain('"name":"beacon-mcp"');
      expect(response.body).toContain('"version":"2.0.2"');
    }
  });

  it("downgrades unknown legacy revisions to the newest served one", async () => {
    const app = await start();
    const response = await app.inject({
      url: "/",
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
      },
      payload: {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "1999-01-01",
          capabilities: {},
          clientInfo: { name: "vitest", version: "1.0.0" },
        },
      },
    });
    expect(response.statusCode).toBe(200);
    expect(response.body).toContain('"protocolVersion":"2025-11-25"');
  });
});
