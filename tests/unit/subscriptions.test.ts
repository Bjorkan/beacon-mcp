import { afterEach, describe, expect, it, vi } from "vitest";
import { beaconResourceUri } from "../../src/mcp/resources.js";
import { ResourceNotifications } from "../../src/subscriptions/notifications.js";
import { BeaconWsBridge } from "../../src/subscriptions/beacon-ws.js";
import { loadConfig } from "../../src/config/config.js";
import { createLogger } from "../../src/logging/logger.js";

const ID = "123e4567-e89b-42d3-a456-426614174000";
const OTHER = "123e4567-e89b-42d3-a456-426614174001";
const logger = createLogger("silent");

afterEach(() => {
  vi.useRealTimers();
});

describe("Beacon resource notification mapping", () => {
  it("normalizes UUIDs and rejects untrusted identifiers", () => {
    expect(beaconResourceUri("nodes", ID.toUpperCase())).toBe(
      `beacon://nodes/${ID}`,
    );
    expect(beaconResourceUri("nodes", "../private")).toBeUndefined();
    expect(beaconResourceUri("observers", "not-a-uuid")).toBeUndefined();
  });

  it("debounces repeated updates, filters incorrect payloads, and stops cleanly", () => {
    vi.useFakeTimers();
    const published = vi.fn();
    const relay = new ResourceNotifications(
      { resourceUpdated: published },
      logger,
      250,
      1,
    );
    relay.received("nodeUpdate", { nodeId: ID });
    relay.received("nodeUpdate", { nodeId: ID });
    relay.received("nodeUpdate", { nodeId: OTHER }); // capacity bound
    relay.received("channelMessage", { nodeId: OTHER });
    relay.received("observerStatus", { observerId: "../bad" });
    vi.advanceTimersByTime(250);
    expect(published).toHaveBeenCalledExactlyOnceWith(`beacon://nodes/${ID}`);
    relay.received("observerStatus", { observerId: OTHER });
    relay.stop();
    vi.runAllTimers();
    expect(published).toHaveBeenCalledTimes(1);
  });
});

class TestSocket extends EventTarget {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
  readyState = 1;
  readonly sent: object[] = [];
  send(text: string) {
    this.sent.push(JSON.parse(text) as object);
  }
  close() {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }
  incoming(value: unknown) {
    const event = new Event("message") as Event & { data: string };
    Object.defineProperty(event, "data", { value: JSON.stringify(value) });
    this.dispatchEvent(event);
  }
  opened() {
    this.dispatchEvent(new Event("open"));
  }
}

describe("Beacon WS bridge", () => {
  it("requires hello and subscribe ack, maps only the selected events, then disconnects", () => {
    const socket = new TestSocket();
    const events: string[] = [];
    const bridge = new BeaconWsBridge(
      new URL("ws://localhost/ws"),
      logger,
      (event) => events.push(event),
      () => socket as unknown as WebSocket,
    );
    bridge.start();
    socket.opened();
    socket.incoming({
      v: 1,
      type: "event",
      event: "nodeUpdate",
      data: { nodeId: ID },
    });
    expect(events).toHaveLength(0);
    socket.incoming({ v: 1, type: "hello" });
    expect(socket.sent).toContainEqual({
      v: 1,
      type: "subscribe",
      id: "mcp-changes",
      scope: { events: ["nodeUpdate", "observerStatus"] },
    });
    socket.incoming({
      v: 1,
      type: "subscribed",
      id: "mcp-changes",
      subscriptionId: "upstream-1",
    });
    socket.incoming({
      v: 1,
      type: "event",
      event: "nodeUpdate",
      data: { nodeId: ID },
    });
    socket.incoming({
      v: 1,
      type: "event",
      event: "observerStatus",
      data: { observerId: OTHER },
    });
    socket.incoming({
      v: 1,
      type: "event",
      event: "channelMessage",
      data: { nodeId: ID },
    });
    socket.incoming({
      v: 2,
      type: "event",
      event: "nodeUpdate",
      data: { nodeId: ID },
    });
    expect(events).toEqual(["nodeUpdate", "observerStatus"]);
    bridge.stop();
    expect(socket.readyState).toBe(TestSocket.CLOSED);
    socket.incoming({
      v: 1,
      type: "event",
      event: "nodeUpdate",
      data: { nodeId: ID },
    });
    expect(events).toHaveLength(2);
  });
});

describe("subscription configuration", () => {
  it("defaults to on and derives WS URL from Beacon origin", () => {
    const cfg = loadConfig({
      BEACON_BASE_URL: "https://beacon.example/api/v1",
    });
    expect(cfg.subscriptions?.enabled).toBe(true);
    expect(cfg.subscriptions?.wsUrl.href).toBe("wss://beacon.example/ws");
    expect(
      loadConfig({
        BEACON_BASE_URL: "https://beacon.example/api/v1",
        BEACON_SUBSCRIPTIONS_ENABLED: "false",
      }).subscriptions?.enabled,
    ).toBe(false);
  });
  it("rejects unsafe upstream URLs and invalid bounds", () => {
    expect(() =>
      loadConfig({
        BEACON_BASE_URL: "https://x.test",
        BEACON_WS_URL: "https://x.test/ws",
      }),
    ).toThrow("ws/wss");
    expect(() =>
      loadConfig({
        BEACON_BASE_URL: "https://x.test",
        BEACON_WS_URL: "wss://user:secret@x.test/ws",
      }),
    ).toThrow("credentials");
    expect(() =>
      loadConfig({
        BEACON_BASE_URL: "https://x.test",
        BEACON_SUBSCRIPTIONS_MAX_STREAMS: "100000",
      }),
    ).toThrow();
  });
});
