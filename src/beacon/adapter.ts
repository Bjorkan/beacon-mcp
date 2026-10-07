import { BeaconInputError } from "./errors.js";
import type { BeaconClient, BeaconResponse } from "./client.js";
import {
  toCursorPage,
  toOffsetArrayPage,
  toPage,
  truncatedArrayPage,
  type Page,
} from "./pagination.js";
import { payloadTypesForName } from "./payload.js";
import { boundedLimit, normalizeIatas } from "./query.js";
import { timeRange } from "../utils/time.js";

interface LocationFilters {
  iatas?: string[];
  region?: string;
  regionId?: number;
}
interface ScopedLocationFilters extends LocationFilters {
  scope?: string;
}
interface OffsetPage {
  cursor?: number;
  limit?: number;
}
interface TimeWindow {
  since?: string;
  until?: string;
}
type Input<T> = {
  [K in keyof T]: T[K] | (undefined extends T[K] ? undefined : never);
};

function location(input: Input<LocationFilters>) {
  const iatas = normalizeIatas(input.iatas);
  return {
    iatas: iatas?.join(","),
    region: input.region,
    regionId: input.regionId,
  };
}

function scopedLocation(input: Input<ScopedLocationFilters>) {
  return { ...location(input), scope: input.scope };
}

function offsetPage(input: Input<OffsetPage>) {
  return { cursor: input.cursor, limit: boundedLimit(input.limit) };
}

export class BeaconAdapter {
  constructor(private readonly client: BeaconClient) {}

  getServerInfo(signal?: AbortSignal): Promise<BeaconResponse<"serverInfo">> {
    return this.client.request("serverInfo", { signal });
  }

  async listIatas(limit = 20, signal?: AbortSignal): Promise<Page> {
    const bounded = boundedLimit(limit);
    return truncatedArrayPage(
      await this.client.requestArrayPage("listIatas", {
        signal,
        maxItems: bounded + 1,
      }),
      bounded,
    );
  }

  async listRegions(limit = 20, signal?: AbortSignal): Promise<Page> {
    const bounded = boundedLimit(limit);
    return truncatedArrayPage(
      await this.client.requestArrayPage("listRegions", {
        signal,
        maxItems: bounded + 1,
      }),
      bounded,
    );
  }

  async listScopes(
    input: Input<LocationFilters & OffsetPage>,
    signal?: AbortSignal,
  ): Promise<Page> {
    const limit = boundedLimit(input.limit);
    return toOffsetArrayPage(
      await this.client.request("listScopes", {
        query: location(input),
        signal,
      }),
      limit,
      input.cursor,
    );
  }

  async searchNodes(
    input: Input<
      ScopedLocationFilters &
        OffsetPage & {
          name?: string;
          type?: number;
          typeName?: string;
          pubkey?: string;
          pubkeyPrefix?: string;
          supportsMultibytePaths?: boolean;
          supportsMultibyteTraces?: boolean;
        }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    return toPage(
      await this.client.request("listNodes", {
        query: {
          ...scopedLocation(input),
          ...page,
          name: input.name,
          type: input.type,
          typeName: input.typeName,
          pubkey: input.pubkey,
          pubkeyPrefix: input.pubkeyPrefix,
          supportsMultibytePaths: input.supportsMultibytePaths,
          supportsMultibyteTraces: input.supportsMultibyteTraces,
        },
        signal,
      }),
      page.limit,
    );
  }

  getNode(
    id: string,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"getNode">> {
    return this.client.request("getNode", { path: { nodeId: id }, signal });
  }

  async searchObservers(
    input: Input<
      ScopedLocationFilters &
        OffsetPage & {
          type?: string;
          broker?: string;
          status?: "online" | "offline";
          name?: string;
        }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    return toPage(
      await this.client.request("listObservers", {
        query: {
          ...scopedLocation(input),
          ...page,
          type: input.type,
          broker: input.broker,
          status: input.status,
          name: input.name,
        },
        signal,
      }),
      page.limit,
    );
  }

  getObserver(
    id: string,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"getObserver">> {
    return this.client.request("getObserver", {
      path: { observerId: id },
      signal,
    });
  }

  getObserverActivity(
    id: string,
    input: Input<{ range?: string; interval?: string; until?: string }>,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"getObserverActivity">> {
    const until = input.until
      ? timeRange(undefined, input.until).until
      : undefined;
    return this.client.request("getObserverActivity", {
      path: { observerId: id },
      query: { range: input.range, interval: input.interval, until },
      signal,
    });
  }

  async searchPackets(
    input: Input<
      ScopedLocationFilters &
        OffsetPage &
        TimeWindow & {
          payloadType?: number;
          payloadTypes?: number[];
          payloadTypeName?: string;
          routeType?: number;
          routeTypes?: number[];
          scopes?: string[];
        }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    const namedPayloadTypes = input.payloadTypeName
      ? payloadTypesForName(input.payloadTypeName)
      : undefined;
    if (input.payloadTypeName && !namedPayloadTypes) {
      throw new BeaconInputError("Unknown payload type name");
    }
    return toPage(
      await this.client.request("listPackets", {
        query: {
          ...scopedLocation(input),
          ...timeRange(input.since, input.until),
          ...page,
          payloadType: input.payloadType,
          payloadTypes:
            input.payloadTypes?.join(",") ?? namedPayloadTypes?.join(","),
          payloadTypeName: undefined,
          routeType: input.routeType,
          routeTypes: input.routeTypes?.join(","),
          scopes: input.scopes?.join(","),
        },
        signal,
      }),
      page.limit,
    );
  }

  getPacket(
    hash: string,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"getPacket">> {
    return this.client.request("getPacket", {
      path: { packetHash: hash },
      signal,
    });
  }

  async searchMessages(
    input: Input<
      Pick<ScopedLocationFilters, "iatas" | "scope"> &
        OffsetPage & {
          since?: string;
          channelId?: number;
          channelHash?: string;
        }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    return toPage(
      await this.client.request("listMessages", {
        query: {
          iatas: normalizeIatas(input.iatas)?.join(","),
          scope: input.scope,
          since: timeRange(input.since).since,
          ...page,
          channelID: input.channelId,
          channelHash: input.channelHash,
        },
        signal,
      }),
      page.limit,
    );
  }

  async listChannels(
    input: Input<{
      iatas?: string[];
      limit?: number;
      hash?: string;
      keyKnown?: boolean;
      pageCursor?: string;
    }>,
    signal?: AbortSignal,
  ): Promise<Page> {
    const limit = boundedLimit(input.limit);
    return toPage(
      await this.client.request("listChannels", {
        query: {
          iatas: normalizeIatas(input.iatas)?.join(","),
          limit,
          hash: input.hash,
          keyKnown: input.keyKnown,
          pageCursor: input.pageCursor,
        },
        signal,
      }),
      limit,
    );
  }

  async getChannelMessages(
    id: number,
    input: Input<
      Pick<ScopedLocationFilters, "iatas" | "scope"> &
        OffsetPage & { since?: string }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    return toPage(
      await this.client.request("getChannelMessages", {
        path: { channelID: id },
        query: {
          iatas: normalizeIatas(input.iatas)?.join(","),
          scope: input.scope,
          since: timeRange(input.since).since,
          ...page,
        },
        signal,
      }),
      page.limit,
    );
  }

  async listRoutes(
    input: Input<{
      iata?: string;
      hopCount?: number;
      cursor?: number;
      cursorId?: number;
      limit?: number;
    }>,
    signal?: AbortSignal,
  ): Promise<Page> {
    const limit = boundedLimit(input.limit);
    const value = await this.client.request("listRoutes", {
      query: {
        iata: input.iata ? normalizeIatas([input.iata])?.[0] : undefined,
        hopCount: input.hopCount,
        cursor: input.cursor,
        cursorId: input.cursorId,
        limit,
      },
      signal,
    });
    return toCursorPage(value, limit, (last) => ({
      cursor: last["lastSeen"],
      cursorId: last["id"],
    }));
  }

  async searchRoutes(
    input: Input<{
      iata: string;
      from: string;
      to: string;
      limit?: number;
    }>,
    signal?: AbortSignal,
  ): Promise<Page> {
    const bounded = boundedLimit(input.limit);
    return truncatedArrayPage(
      await this.client.requestArrayPage("searchRoutes", {
        query: {
          iata: normalizeIatas([input.iata])?.[0] ?? input.iata,
          from: input.from,
          to: input.to,
        },
        signal,
        maxItems: bounded + 1,
      }),
      bounded,
    );
  }

  async findCrossIataRoutes(
    input: Input<{
      fromHash: string;
      fromIata: string;
      toHash: string;
      toIata: string;
      limit?: number;
    }>,
    signal?: AbortSignal,
  ): Promise<Page> {
    const bounded = boundedLimit(input.limit);
    return truncatedArrayPage(
      await this.client.requestArrayPage("crossRoutes", {
        query: {
          fromHash: input.fromHash,
          fromIata: normalizeIatas([input.fromIata])?.[0] ?? input.fromIata,
          toHash: input.toHash,
          toIata: normalizeIatas([input.toIata])?.[0] ?? input.toIata,
        },
        signal,
        maxItems: bounded + 1,
      }),
      bounded,
    );
  }

  async searchTraces(
    input: Input<
      ScopedLocationFilters &
        OffsetPage &
        TimeWindow & { type?: "TRACE" | "PING"; cursorTag?: string }
    >,
    signal?: AbortSignal,
  ): Promise<Page> {
    const page = offsetPage(input);
    const value = await this.client.request("listTraces", {
      query: {
        ...scopedLocation(input),
        ...timeRange(input.since, input.until),
        ...page,
        type: input.type,
        cursorTag: input.cursorTag,
      },
      signal,
    });
    return toCursorPage(value, page.limit, (last) => ({
      cursor: last["lastHeardAt"],
      cursorTag: last["traceTag"],
    }));
  }

  getTrace(
    tag: string,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"getTrace">> {
    return this.client.request("getTrace", { path: { tag }, signal });
  }

  getNetworkOverview(
    input: Input<LocationFilters>,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"networkOverview">> {
    return this.client.request("networkOverview", {
      query: location(input),
      signal,
    });
  }

  getNetworkSeries(
    input: Input<LocationFilters & { since: string; until: string }>,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"networkSeries">> {
    return this.client.request("networkSeries", {
      query: {
        ...location(input),
        ...timeRange(input.since, input.until),
      },
      signal,
    });
  }

  compareObservers(
    input: Input<
      LocationFilters & {
        observerA: string;
        observerB: string;
        since: string;
        until: string;
      }
    >,
    signal?: AbortSignal,
  ): Promise<BeaconResponse<"compareObservers">> {
    if (input.observerA === input.observerB) {
      throw new BeaconInputError("observerA and observerB must be different");
    }
    return this.client.request("compareObservers", {
      query: {
        observerA: input.observerA,
        observerB: input.observerB,
        ...location(input),
        ...timeRange(input.since, input.until),
      },
      signal,
    });
  }
}
