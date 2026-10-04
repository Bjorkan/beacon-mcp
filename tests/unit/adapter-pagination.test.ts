import { describe, expect, it, vi } from "vitest";
import { BeaconAdapter } from "../../src/beacon/adapter.js";
import { BeaconClient } from "../../src/beacon/client.js";

const config = {
  beaconBaseUrl: new URL("https://beacon.example/"),
  beaconTimeoutMs: 1_000,
  beaconStatsTimeoutMs: 1_000,
  beaconMaxResponseBytes: 1_000_000,
  version: "test",
};

function adapterReturning(body: unknown): BeaconAdapter {
  const fetchMock = vi.fn<typeof fetch>(async () => Response.json(body));
  return new BeaconAdapter(new BeaconClient(config, { fetch: fetchMock }));
}

describe("Beacon adapter pagination", () => {
  it("derives the route compound cursor from the last returned route", async () => {
    const page = await adapterReturning([
      { id: 10, lastSeen: 2_000 },
      { id: 9, lastSeen: 1_000 },
    ]).listRoutes({ limit: 2 });
    expect(page.pagination).toEqual({
      hasMore: true,
      nextCursor: { cursor: 1_000, cursorId: 9 },
    });
  });

  it("derives the trace compound cursor from the last returned trace", async () => {
    const page = await adapterReturning([
      { traceTag: "aabb", lastHeardAt: 2_000 },
      { traceTag: "ccdd", lastHeardAt: 1_000 },
    ]).searchTraces({ limit: 2 });
    expect(page.pagination).toEqual({
      hasMore: true,
      nextCursor: { cursor: 1_000, cursorTag: "ccdd" },
    });
  });

  it("marks bounded non-pageable arrays as truncated without a fake cursor", async () => {
    const page = await adapterReturning(["ARN", "GOT", "MMX"]).listIatas(2);
    expect(page).toEqual({
      items: ["ARN", "GOT"],
      pagination: { hasMore: false, truncated: true },
    });
  });
});
