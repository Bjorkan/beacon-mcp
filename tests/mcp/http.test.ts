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
  beaconMaxResponseBytes: 5_242_880,
  mcpLegacyMode: "stateless",
  allowedHosts: ["127.0.0.1", "localhost"],
  allowedOrigins: ["127.0.0.1", "localhost"],
  logLevel: "silent",
  shutdownGraceMs: 1000,
  version: "test",
});

interface StartOptions {
  auth?: string;
  config?: Partial<Config>;
  fetchImpl?: typeof fetch;
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
    ...(options.auth ? { mcpAuthToken: options.auth } : {}),
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
  token?: string,
) {
  return app.inject({
    url: "/mcp",
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    payload: { jsonrpc: "2.0", id: 1, method, ...(params ? { params } : {}) },
  });
}

afterEach(async () => Promise.all(apps.splice(0).map((app) => app.close())));

describe("MCP HTTP integration", () => {
  it("serves health/readiness and discovers only read-only tools", async () => {
    const app = await start();
    expect((await app.inject({ url: "/healthz" })).json()).toEqual({
      status: "ok",
    });
    expect((await app.inject({ url: "/readyz" })).json()).toEqual({
      status: "ok",
    });
    const response = await rpc(app, "tools/list");
    expect(response.statusCode).toBe(200);
    const text = response.body;
    expect(text).toContain("beacon_search_nodes");
    expect(text).toContain("beacon_compare_observers");
    expect(text).not.toContain("admin");
    const modern = await app.inject({
      url: "/mcp",
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
        arguments: { hash: "abcd" },
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

    const channels = await rpc(app, "tools/call", {
      name: "beacon_list_channels",
      arguments: { cursor: 1, pageCursor: "opaque" },
    });
    expect(channels.body).toContain("pageCursor cannot be combined");

    const activity = await rpc(app, "tools/call", {
      name: "beacon_get_observer_activity",
      arguments: {
        id: "00000000-0000-4000-8000-000000000000",
        range: "721h",
        interval: "2h",
      },
    });
    expect(activity.body).toContain("Invalid");
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
        arguments: { hash: "abcd" },
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
      arguments: { hash: "abcd" },
    });
    expect(timeout.body).toContain("BeaconTimeoutError");
  });

  it("rejects disallowed hosts, browser origins, and legacy requests", async () => {
    const app = await start();
    expect(
      (
        await app.inject({
          url: "/mcp",
          method: "POST",
          headers: { host: "evil.example" },
        })
      ).statusCode,
    ).toBe(403);
    expect(
      (
        await app.inject({
          url: "/mcp",
          method: "POST",
          headers: { origin: "https://evil.example" },
        })
      ).statusCode,
    ).toBe(403);

    const modernOnly = await start({ config: { mcpLegacyMode: "reject" } });
    expect((await rpc(modernOnly, "tools/list")).statusCode).toBe(400);
  });

  it("enforces bearer authentication", async () => {
    const token = "0123456789abcdef";
    const app = await start({ auth: token });
    expect((await rpc(app, "tools/list")).statusCode).toBe(401);
    expect((await rpc(app, "tools/list", undefined, token)).statusCode).toBe(
      200,
    );
  });
});
