import { describe, expect, it, vi } from "vitest";
import { BeaconAdapter } from "../../src/beacon/adapter.js";
import { BeaconClient } from "../../src/beacon/client.js";

const config = {
  beaconBaseUrl: new URL("https://beacon.example/"),
  beaconTimeoutMs: 1_000,
  beaconStatsTimeoutMs: 1_000,
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

  it("paginates the complete upstream scope array locally", async () => {
    const adapter = adapterReturning(["#a", "#b", "#c"]);
    await expect(adapter.listScopes({ limit: 2 })).resolves.toEqual({
      items: ["#a", "#b"],
      pagination: { hasMore: true, nextCursor: { cursor: 2 } },
    });
    await expect(adapter.listScopes({ cursor: 2, limit: 2 })).resolves.toEqual({
      items: ["#c"],
      pagination: { hasMore: false },
    });
  });

  it("continues channel pages only with the opaque page cursor", async () => {
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json({ items: [], hasMore: false });
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    await adapter.listChannels({ limit: 2, pageCursor: "v1:1000:7" });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/channels?limit=2&pageCursor=v1%3A1000%3A7",
    ]);
  });

  it("normalizes canonical and legacy payload names to numeric filters", async () => {
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json({ items: [], hasMore: false });
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    await adapter.searchPackets({ payloadTypeName: "group_text" });
    await adapter.searchPackets({ payloadTypeName: "grp_txt" });
    await adapter.searchPackets({ payloadTypeName: "reserved" });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/packets?limit=20&payloadTypes=5",
      "https://beacon.example/api/v1/packets?limit=20&payloadTypes=5",
      "https://beacon.example/api/v1/packets?limit=20&payloadTypes=12%2C13%2C14",
    ]);
  });
});
