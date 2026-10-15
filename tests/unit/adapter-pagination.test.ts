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

  it("marks reads that stopped before the array terminator as truncated", async () => {
    // A dropped connection mid-array: one complete element, no closing ']'.
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        const encoder = new TextEncoder();
        controller.enqueue(encoder.encode('[{"iata":"ARN"}'));
        controller.close();
      },
    });
    const fetchMock = vi.fn<typeof fetch>(
      async () =>
        new Response(stream, {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    );
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    await expect(adapter.listIatas(5)).resolves.toEqual({
      items: [{ iata: "ARN" }],
      pagination: { hasMore: false, truncated: true },
    });
  });

  it("applies the cross-IATA limit while streaming, not after download", async () => {
    const route = {
      sourceSegment: [{ hashBytes: "88a9" }],
      crossHop: { fromIata: "MMX", toIata: "HAD" },
      totalHops: 5,
    };
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json([route, route, route]);
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    const page = await adapter.findCrossIataRoutes({
      fromHash: "88a9",
      fromIata: "mmx",
      toHash: "3ad1",
      toIata: "HAD",
      limit: 1,
    });
    expect(page).toEqual({
      items: [route],
      pagination: { hasMore: false, truncated: true },
    });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/routes/cross?fromHash=88a9&fromIata=MMX&toHash=3ad1&toIata=HAD",
    ]);
  });

  it("reports a full bounded array without truncation", async () => {
    const page = await adapterReturning([{ id: 1 }, { id: 2 }]).searchRoutes({
      iata: "ARN",
      from: "88a9",
      to: "3ad1",
      limit: 2,
    });
    expect(page).toEqual({
      items: [{ id: 1 }, { id: 2 }],
      pagination: { hasMore: false },
    });
  });

  it("supports all-IATA, multi-IATA, and directed route searches", async () => {
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json([]);
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    await adapter.searchRoutes({ from: "aabbccdd", to: "ff" });
    await adapter.searchRoutes({ from: "aa", to: "bb", iatas: ["arn", "mmx"] });
    await adapter.findCrossIataRoutes({ fromHash: "aa", toHash: "bb" });
    await adapter.findCrossIataRoutes({
      fromHash: "aabbccdd",
      toHash: "cc",
      iatas: ["arn", "mmx"],
    });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/routes/search?from=aabbccdd&to=ff",
      "https://beacon.example/api/v1/routes/search?iatas=ARN%2CMMX&from=aa&to=bb",
      "https://beacon.example/api/v1/routes/cross?fromHash=aa&toHash=bb",
      "https://beacon.example/api/v1/routes/cross?fromHash=aabbccdd&toHash=cc&iatas=ARN%2CMMX",
    ]);
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

  it("requires the advertiser sort and forwards flood/direct rankings", async () => {
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json([{ publicKey: "ab" }]);
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    const page = await adapter.getTopAdvertisers({
      iatas: ["arn"],
      sort: "direct",
      since: "2026-10-01T00:00:00Z",
      limit: 2,
    });
    expect(page).toEqual({
      items: [{ publicKey: "ab" }],
      pagination: { hasMore: false },
    });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/stats/top-advertisers?iatas=ARN&sort=direct&since=1790812800000&limit=2",
    ]);
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

  it("forwards packet hash prefixes, including odd-length and mixed-case hex", async () => {
    const urls: string[] = [];
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      urls.push(String(input));
      return Response.json({ items: [], hasMore: false });
    });
    const adapter = new BeaconAdapter(
      new BeaconClient(config, { fetch: fetchMock }),
    );
    await adapter.searchPackets({ hashPrefix: "aBcDe", iatas: ["arn"] });
    expect(urls).toEqual([
      "https://beacon.example/api/v1/packets?iatas=ARN&limit=20&hashPrefix=aBcDe",
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
