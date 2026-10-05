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
