import { describe, expect, it, vi } from "vitest";
import { BeaconClient } from "../../src/beacon/client.js";
import {
  BeaconRateLimitError,
  BeaconResponseTooLargeError,
  BeaconTimeoutError,
  BeaconUpstreamError,
} from "../../src/beacon/errors.js";

const config = {
  beaconBaseUrl: new URL("https://beacon.example/"),
  beaconTimeoutMs: 100,
  beaconStatsTimeoutMs: 200,
};

describe("Beacon HTTP contract", () => {
  it("calls only the pinned public endpoint and returns representative data", async () => {
    const fetchMock = vi.fn<typeof fetch>(async (input, init) => {
      expect(String(input)).toBe(
        "https://beacon.example/api/v1/nodes?iatas=ARN&limit=20",
      );
      expect(init?.headers).toMatchObject({
        "user-agent": "beacon-mcp/2.0.2",
      });
      expect(init?.redirect).toBe("error");
      return Response.json({
        items: [{ id: "node-1", name: "Repeater", lastSeen: 1791127800000 }],
        hasMore: false,
      });
    });
    const client = new BeaconClient(config, { fetch: fetchMock });
    await expect(
      client.request("listNodes", { query: { iatas: "ARN", limit: 20 } }),
    ).resolves.toMatchObject({ items: [{ id: "node-1" }] });
  });

  it("rejects successful invalid JSON and oversized response streams", async () => {
    const invalid = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        Promise.resolve(new Response("not-json", { status: 200 })),
      ),
    });
    await expect(invalid.request("listIatas")).rejects.toMatchObject({
      name: "BeaconUpstreamError",
      status: 200,
    });

    const oversized = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        Promise.resolve(
          new Response("[]", {
            headers: { "content-length": "5242881" },
          }),
        ),
      ),
    });
    await expect(oversized.request("listIatas")).rejects.toBeInstanceOf(
      BeaconResponseTooLargeError,
    );
  });

  it("propagates caller cancellation without converting it to a timeout", async () => {
    const controller = new AbortController();
    const cancelled = new DOMException("caller cancelled", "AbortError");
    const pending = vi.fn<typeof fetch>(
      async (_input, init) =>
        new Promise((_resolve, reject) =>
          init?.signal?.addEventListener("abort", () => reject(cancelled), {
            once: true,
          }),
        ),
    );
    const request = new BeaconClient(config, { fetch: pending }).request(
      "listIatas",
      {
        signal: controller.signal,
      },
    );
    controller.abort(cancelled);
    await expect(request).rejects.toBe(cancelled);
    expect(pending).toHaveBeenCalledTimes(1);
  });

  it("never retries rate limits and preserves Retry-After", async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      Response.json(
        { error: { code: "rate_limited", message: "slow down" } },
        { status: 429, headers: { "retry-after": "60" } },
      ),
    );
    const client = new BeaconClient(config, { fetch: fetchMock });
    await expect(client.request("listPackets")).rejects.toBeInstanceOf(
      BeaconRateLimitError,
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("retries one transient 503 and then fails", async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      Response.json(
        { error: { code: "service_unavailable", message: "down" } },
        { status: 503 },
      ),
    );
    const client = new BeaconClient(config, {
      fetch: fetchMock,
      sleep: async () => undefined,
    });
    await expect(client.request("listPackets")).rejects.toBeInstanceOf(
      BeaconUpstreamError,
    );
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("maps timeouts and structurally prevents arbitrary/admin operations", async () => {
    const pending = vi.fn<typeof fetch>(
      async (_input, init) =>
        new Promise((_resolve, reject) =>
          init?.signal?.addEventListener(
            "abort",
            () => reject(init.signal?.reason),
            {
              once: true,
            },
          ),
        ),
    );
    const client = new BeaconClient(config, { fetch: pending });
    await expect(client.request("listIatas")).rejects.toBeInstanceOf(
      BeaconTimeoutError,
    );
    await expect(client.request("admin" as never)).rejects.toThrow("Unsafe");
    expect(pending).toHaveBeenCalledTimes(1);
  });

  it("rejects oversized outbound URLs before making a request", async () => {
    const fetchMock = vi.fn<typeof fetch>();
    const client = new BeaconClient(config, { fetch: fetchMock });
    await expect(
      client.request("listNodes", { query: { name: "x".repeat(8_192) } }),
    ).rejects.toThrow("filters are too large");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

/** Build a JSON response body delivered one string chunk at a time. */
function chunkedResponse(
  chunks: string[],
  counters: { pulled: number } = { pulled: 0 },
): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      counters.pulled += 1;
      const chunk = chunks.shift();
      if (chunk === undefined) controller.close();
      else controller.enqueue(encoder.encode(chunk));
    },
  });
  return new Response(stream, {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

describe("bounded incremental array reads", () => {
  it("stops reading once the item budget is satisfied", async () => {
    const item = JSON.stringify({
      sourceSegment: [{ nodeId: "x" }],
      crossHop: { fromIata: "MMX", toIata: "HAD" },
      totalHops: 7,
    });
    const chunks = ["["];
    for (let i = 0; i < 40; i += 1) {
      chunks.push(`${i > 0 ? "," : ""}${item}`);
    }
    chunks.push("]");
    const counters = { pulled: 0 };
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () => chunkedResponse(chunks, counters)),
    });
    const expected = JSON.parse(item) as unknown;
    await expect(
      client.requestArrayPage("crossRoutes", { maxItems: 3 }),
    ).resolves.toEqual({
      complete: false,
      items: [expected, expected, expected],
    });
    expect(counters.pulled).toBeLessThan(chunks.length);
  });

  it("returns the complete array when it ends within the budget", async () => {
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        chunkedResponse(["[{", '"a"', ":1},", '{"a":2', "}]"]),
      ),
    });
    await expect(
      client.requestArrayPage("searchRoutes", { maxItems: 50 }),
    ).resolves.toEqual({ complete: true, items: [{ a: 1 }, { a: 2 }] });
  });

  it("handles strings, escapes, and scalar elements across chunk borders", async () => {
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        chunkedResponse([
          '["a,b", "c\\"d", ',
          'null, 12, true, {"x":"y,z"}',
          "]",
        ]),
      ),
    });
    await expect(
      client.requestArrayPage("listIatas", { maxItems: 10 }),
    ).resolves.toEqual({
      complete: true,
      items: ["a,b", 'c"d', null, 12, true, { x: "y,z" }],
    });
  });

  it("reports a partial read when the byte cap is hit after items", async () => {
    const item = `{"pad":"${"x".repeat(65536)}"}`;
    const chunks: string[] = ["["];
    for (let i = 0; i < 120; i += 1) {
      chunks.push(`${i > 0 ? "," : ""}${item}`);
    }
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () => chunkedResponse(chunks)),
    });
    await expect(
      client.requestArrayPage("searchRoutes", { maxItems: 200 }),
    ).resolves.toMatchObject({ complete: false });
  });

  it("still errors when the byte cap is hit before any element completes", async () => {
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        chunkedResponse([`[${"x".repeat(5_242_881)}`]),
      ),
    });
    await expect(
      client.requestArrayPage("searchRoutes", { maxItems: 10 }),
    ).rejects.toBeInstanceOf(BeaconResponseTooLargeError);
  });

  it("falls back to whole-body parsing for non-array envelopes", async () => {
    const client = new BeaconClient(config, {
      fetch: vi.fn<typeof fetch>(async () =>
        Response.json({ items: [1, 2], hasMore: false }),
      ),
    });
    await expect(
      client.requestArrayPage("listIatas", { maxItems: 10 }),
    ).resolves.toEqual({ complete: true, items: [1, 2] });
  });

  it("gives cross-IATA route search the analytics timeout budget", async () => {
    const pending = vi.fn<typeof fetch>(
      async (_input, init) =>
        new Promise((_resolve, reject) =>
          init?.signal?.addEventListener(
            "abort",
            () => reject(init.signal?.reason),
            { once: true },
          ),
        ),
    );
    const slowConfig = {
      beaconBaseUrl: new URL("https://beacon.example/"),
      beaconTimeoutMs: 50,
      beaconStatsTimeoutMs: 300,
    };
    const client = new BeaconClient(slowConfig, { fetch: pending });
    const started = Date.now();
    await expect(
      client.request("crossRoutes", {
        query: {
          fromHash: "88a9",
          fromIata: "MMX",
          toHash: "3ad1",
          toIata: "HAD",
        },
      }),
    ).rejects.toBeInstanceOf(BeaconTimeoutError);
    const crossElapsed = Date.now() - started;
    expect(crossElapsed).toBeGreaterThanOrEqual(250);
    const listStarted = Date.now();
    await expect(client.request("listIatas")).rejects.toBeInstanceOf(
      BeaconTimeoutError,
    );
    expect(Date.now() - listStarted).toBeLessThan(250);
    expect(crossElapsed).toBeGreaterThan(Date.now() - listStarted);
  });
});
