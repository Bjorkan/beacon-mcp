import { afterEach, describe, expect, it, vi } from "vitest";
import type { FastifyInstance } from "fastify";
import { BeaconClient } from "../../src/beacon/client.js";
import { BeaconAdapter } from "../../src/beacon/adapter.js";
import { loadConfig } from "../../src/config/config.js";
import { createLogger } from "../../src/logging/logger.js";
import { buildHttpServer } from "../../src/server/http.js";

const nodeId = "123e4567-e89b-42d3-a456-426614174000";
const observerId = "123e4567-e89b-42d3-a456-426614174001";

class UpstreamSocket extends EventTarget {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
  readyState = 1;
  sent: unknown[] = [];
  send(value: string) {
    this.sent.push(JSON.parse(value) as unknown);
  }
  close() {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }
  push(value: object) {
    const event = new Event("message");
    Object.defineProperty(event, "data", { value: JSON.stringify(value) });
    this.dispatchEvent(event);
  }
}

const apps: FastifyInstance[] = [];
const aborts: AbortController[] = [];
afterEach(async () => {
  aborts.splice(0).forEach((a) => a.abort());
  await Promise.all(apps.splice(0).map((a) => a.close()));
  vi.unstubAllGlobals();
});

function payload(
  method: string,
  params: Record<string, unknown> = {},
  era = "2026-07-28",
) {
  return {
    jsonrpc: "2.0",
    id: "listen-1",
    method,
    params: {
      ...params,
      _meta: {
        "io.modelcontextprotocol/protocolVersion": era,
        "io.modelcontextprotocol/clientInfo": {
          name: "integration-test",
          version: "1.0",
        },
        "io.modelcontextprotocol/clientCapabilities": {},
      },
    },
  };
}
function headers(method: string, name?: string) {
  return {
    "content-type": "application/json",
    accept: "application/json, text/event-stream",
    "mcp-protocol-version": "2026-07-28",
    "mcp-method": method,
    ...(name ? { "mcp-name": name } : {}),
  };
}
async function fixture(enabled: boolean) {
  const socket = new UpstreamSocket();
  vi.stubGlobal(
    "WebSocket",
    class {
      static OPEN = 1;
      static CLOSED = 3;
      constructor() {
        return socket;
      }
    },
  );
  const cfg = loadConfig({
    BEACON_BASE_URL: "http://127.0.0.1:3001",
    BEACON_SUBSCRIPTIONS_ENABLED: enabled ? "true" : "false",
    BEACON_SUBSCRIPTIONS_DEBOUNCE_MS: "0",
  });
  const fakeFetch = vi.fn<typeof fetch>(async (url) => {
    const path = new URL(String(url)).pathname;
    return Response.json({
      id: path.split("/").pop(),
      kind: path.includes("/nodes/") ? "node" : "observer",
    });
  });
  const app = buildHttpServer(
    cfg,
    new BeaconAdapter(new BeaconClient(cfg, { fetch: fakeFetch })),
    createLogger("silent"),
  );
  apps.push(app);
  await app.listen({ host: "127.0.0.1", port: 0 });
  const addr = app.server.address();
  if (!addr || typeof addr === "string") throw new Error("No listener");
  const url = `http://127.0.0.1:${addr.port}/`;
  if (enabled) {
    socket.dispatchEvent(new Event("open"));
    socket.push({ v: 1, type: "hello" });
    socket.push({
      v: 1,
      type: "subscribed",
      id: "mcp-changes",
      subscriptionId: "ws-1",
    });
  }
  return { app, socket, url, fakeFetch };
}
async function rpc(
  url: string,
  method: string,
  params?: Record<string, unknown>,
) {
  const result = await fetch(url, {
    method: "POST",
    headers: headers(
      method,
      typeof params?.["uri"] === "string" ? params["uri"] : undefined,
    ),
    body: JSON.stringify(payload(method, params)),
  });
  return result.json() as Promise<{
    result?: Record<string, unknown>;
    error?: Record<string, unknown>;
  }>;
}

async function waitFor(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  pattern: string,
): Promise<string> {
  let value = "";
  const deadline = Date.now() + 3_000;
  while (!value.includes(pattern) && Date.now() < deadline) {
    const next = await Promise.race([
      reader.read(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Timed out waiting for ${pattern}: ${value}`)),
          3000,
        ),
      ),
    ]);
    if (next.done) break;
    value += new TextDecoder().decode(next.value);
  }
  expect(value).toContain(pattern);
  return value;
}

describe("MCP resource subscription transport", () => {
  it("lists templates, reads REST resources and advertises modern subscription capability", async () => {
    const { url, fakeFetch } = await fixture(true);
    const disc = await rpc(url, "server/discover");
    expect(JSON.stringify(disc.result)).toContain('"subscribe":true');
    const templates = await rpc(url, "resources/templates/list");
    expect(JSON.stringify(templates.result)).toContain("beacon://nodes/{id}");
    expect(JSON.stringify(templates.result)).toContain(
      "beacon://observers/{id}",
    );
    const result = await rpc(url, "resources/read", {
      uri: `beacon://nodes/${nodeId}`,
    });
    expect(JSON.stringify(result.result)).toContain(nodeId);
    expect(fakeFetch).toHaveBeenCalled();
    const bad = await rpc(url, "resources/read", {
      uri: "beacon://nodes/../../etc/passwd",
    });
    expect(bad.error).toBeDefined();
  });

  it("does not advertise subscribe when disabled", async () => {
    const { url } = await fixture(false);
    const disc = await rpc(url, "server/discover");
    expect(JSON.stringify(disc.result)).not.toContain('"subscribe":true');
    expect(
      JSON.stringify(await rpc(url, "resources/templates/list")),
    ).toContain("beacon://nodes/{id}");
  });

  it("sends ack then updates for the subscribed URI only", async () => {
    const { url, socket } = await fixture(true);
    const controller = new AbortController();
    aborts.push(controller);
    const response = await fetch(url, {
      method: "POST",
      headers: headers("subscriptions/listen"),
      signal: controller.signal,
      body: JSON.stringify(
        payload("subscriptions/listen", {
          notifications: {
            resourceSubscriptions: [`beacon://nodes/${nodeId}`],
          },
        }),
      ),
    });
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/event-stream");
    const reader = response.body!.getReader();
    const ack = await waitFor(
      reader,
      "notifications/subscriptions/acknowledged",
    );
    expect(ack).toContain("listen-1");
    socket.push({
      v: 1,
      type: "event",
      event: "observerStatus",
      data: { observerId },
    });
    socket.push({ v: 1, type: "event", event: "nodeUpdate", data: { nodeId } });
    const notification = await waitFor(
      reader,
      "notifications/resources/updated",
    );
    expect(notification).toContain(`beacon://nodes/${nodeId}`);
    expect(notification).not.toContain("beacon://observers/");
    expect(notification).toContain("io.modelcontextprotocol/subscriptionId");
    controller.abort();
  });

  it("does not advertise the old resources/subscribe protocol to legacy clients", async () => {
    const { url } = await fixture(true);
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        "mcp-protocol-version": "2025-11-25",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 3,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "legacy", version: "1" },
        },
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.text();
    expect(body).toContain("resources");
    expect(body).not.toContain('"subscribe":true');
  });

  it("delivers updates to two independent streams without cross-notifying", async () => {
    const { url, socket } = await fixture(true);
    const open = async (uri: string) => {
      const controller = new AbortController();
      aborts.push(controller);
      const response = await fetch(url, {
        method: "POST",
        headers: headers("subscriptions/listen"),
        signal: controller.signal,
        body: JSON.stringify(
          payload("subscriptions/listen", {
            notifications: { resourceSubscriptions: [uri] },
          }),
        ),
      });
      const reader = response.body!.getReader();
      await waitFor(reader, "notifications/subscriptions/acknowledged");
      return reader;
    };
    const nodeReader = await open(`beacon://nodes/${nodeId}`);
    const observerReader = await open(`beacon://observers/${observerId}`);
    socket.push({ v: 1, type: "event", event: "nodeUpdate", data: { nodeId } });
    socket.push({
      v: 1,
      type: "event",
      event: "observerStatus",
      data: { observerId },
    });
    const [nodeNote, observerNote] = await Promise.all([
      waitFor(nodeReader, "notifications/resources/updated"),
      waitFor(observerReader, "notifications/resources/updated"),
    ]);
    expect(nodeNote).toContain(`beacon://nodes/${nodeId}`);
    expect(nodeNote).not.toContain(`beacon://observers/${observerId}`);
    expect(observerNote).toContain(`beacon://observers/${observerId}`);
    expect(observerNote).not.toContain(`beacon://nodes/${nodeId}`);
  });

  it("shuts down an active listen stream without waiting for the client", async () => {
    const { url, app } = await fixture(true);
    const controller = new AbortController();
    aborts.push(controller);
    const response = await fetch(url, {
      method: "POST",
      headers: headers("subscriptions/listen"),
      signal: controller.signal,
      body: JSON.stringify(
        payload("subscriptions/listen", {
          notifications: {
            resourceSubscriptions: [`beacon://nodes/${nodeId}`],
          },
        }),
      ),
    });
    const reader = response.body!.getReader();
    await waitFor(reader, "notifications/subscriptions/acknowledged");
    await Promise.race([
      app.close(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("Active SSE stream blocked shutdown")),
          2000,
        ),
      ),
    ]);
    controller.abort();
  });
});
