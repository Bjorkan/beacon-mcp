import type { Config } from "../config/config.js";
import type { paths } from "../generated/beacon-api.js";
import type { Logger } from "../logging/logger.js";
import { currentRequestContext } from "../logging/logger.js";
import {
  BeaconResponseTooLargeError,
  BeaconTimeoutError,
  BeaconUpstreamError,
  errorFromResponse,
} from "./errors.js";
import { buildQuery, type QueryValue } from "./query.js";
import { BEACON_API_VERSION } from "./version.js";

const BEACON_MAX_RESPONSE_BYTES = 5_242_880;

type PublicApiPath = Exclude<keyof paths, `/admin/${string}`>;

const endpoints = {
  listIatas: "/iatas",
  listRegions: "/regions",
  listScopes: "/scopes",
  listNodes: "/nodes",
  getNode: "/nodes/{nodeId}",
  listObservers: "/observers",
  getObserver: "/observers/{observerId}",
  getObserverActivity: "/observers/{observerId}/activity",
  listPackets: "/packets",
  getPacket: "/packets/{packetHash}",
  listMessages: "/messages",
  listChannels: "/channels",
  getChannelMessages: "/channels/{channelID}/messages",
  listRoutes: "/routes",
  searchRoutes: "/routes/search",
  crossRoutes: "/routes/cross",
  listTraces: "/traces",
  getTrace: "/traces/{tag}",
  networkOverview: "/stats/overview",
  networkSeries: "/stats/series",
  compareObservers: "/stats/observer-comparison",
} as const satisfies Record<string, PublicApiPath>;

export type BeaconOperation = keyof typeof endpoints;
type GetOperation<O extends BeaconOperation> = NonNullable<
  paths[(typeof endpoints)[O]]["get"]
>;
type Parameters<O extends BeaconOperation> =
  GetOperation<O> extends {
    parameters: infer P;
  }
    ? P
    : never;
type AllowExplicitUndefined<T> = {
  [K in keyof T]: T[K] | (undefined extends T[K] ? undefined : never);
};
export type BeaconQuery<O extends BeaconOperation> =
  Parameters<O> extends {
    query?: infer Q;
  }
    ? [Q] extends [never]
      ? Record<never, never>
      : AllowExplicitUndefined<Q>
    : Record<never, never>;
export type BeaconPath<O extends BeaconOperation> =
  Parameters<O> extends {
    path?: infer P;
  }
    ? [P] extends [never]
      ? Record<never, never>
      : P
    : Record<never, never>;
export type BeaconResponse<O extends BeaconOperation> =
  GetOperation<O> extends { responses: infer R }
    ? R extends Record<200, { content: { "application/json": infer B } }>
      ? B
      : never
    : never;

export interface BeaconRequest<O extends BeaconOperation> {
  path?: BeaconPath<O>;
  query?: BeaconQuery<O>;
  signal?: AbortSignal | undefined;
}

export interface BeaconClientOptions {
  fetch?: typeof fetch;
  sleep?: (ms: number) => Promise<void>;
  logger?: Logger;
}

function resultCount(body: unknown): number | undefined {
  if (Array.isArray(body)) return body.length;
  if (body && typeof body === "object") {
    const items = (body as Record<string, unknown>)["items"];
    if (Array.isArray(items)) return items.length;
  }
  return undefined;
}

async function readBoundedBody(
  response: Response,
  maxBytes: number,
): Promise<string> {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    await response.body?.cancel();
    throw new BeaconResponseTooLargeError(
      `Beacon response exceeded the ${maxBytes} byte limit`,
      response.status,
    );
  }
  if (!response.body) return "";

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let byteCount = 0;
  let text = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      byteCount += value.byteLength;
      if (byteCount > maxBytes) {
        await reader.cancel();
        throw new BeaconResponseTooLargeError(
          `Beacon response exceeded the ${maxBytes} byte limit`,
          response.status,
        );
      }
      text += decoder.decode(value, { stream: true });
    }
    return text + decoder.decode();
  } finally {
    reader.releaseLock();
  }
}

async function responseJson(
  response: Response,
  maxBytes: number,
): Promise<unknown> {
  if (response.status === 204) return null;
  const text = await readBoundedBody(response, maxBytes);
  try {
    return JSON.parse(text) as unknown;
  } catch {
    if (response.ok) {
      throw new BeaconUpstreamError(
        "Beacon returned an invalid JSON response",
        response.status,
      );
    }
    return undefined;
  }
}

export class BeaconClient {
  private readonly fetchImpl: typeof fetch;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly logger: Logger | undefined;

  constructor(
    private readonly config: Pick<
      Config,
      "beaconBaseUrl" | "beaconTimeoutMs" | "beaconStatsTimeoutMs"
    >,
    options: BeaconClientOptions = {},
  ) {
    this.fetchImpl = options.fetch ?? fetch;
    this.sleep =
      options.sleep ??
      ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.logger = options.logger;
  }

  async request<O extends BeaconOperation>(
    operation: O,
    options: BeaconRequest<O> = {},
  ): Promise<BeaconResponse<O>> {
    const selected: string | undefined = endpoints[operation];
    if (!selected) throw new Error("Unsafe or unknown Beacon operation");
    let pathname = selected;
    for (const [name, value] of Object.entries(options.path ?? {})) {
      pathname = pathname.replace(
        `{${name}}`,
        encodeURIComponent(String(value)),
      );
    }
    if (
      pathname.includes("{") ||
      pathname.includes("/admin/") ||
      !pathname.startsWith("/")
    ) {
      throw new Error("Unsafe or incomplete Beacon operation path");
    }
    const url = new URL(`api/v1${pathname}`, this.config.beaconBaseUrl);
    url.search = buildQuery(
      (options.query ?? {}) as Readonly<Record<string, QueryValue>>,
    ).toString();
    const analytics =
      operation === "networkOverview" ||
      operation === "networkSeries" ||
      operation === "compareObservers";
    const timeoutMs = analytics
      ? this.config.beaconStatsTimeoutMs
      : this.config.beaconTimeoutMs;

    for (let attempt = 1; attempt <= 2; attempt += 1) {
      const started = performance.now();
      let logged = false;
      let upstreamStatus: number | undefined;
      const timeoutSignal = AbortSignal.timeout(timeoutMs);
      const signal = options.signal
        ? AbortSignal.any([options.signal, timeoutSignal])
        : timeoutSignal;
      try {
        const response = await this.fetchImpl(url, {
          method: "GET",
          headers: {
            accept: "application/json",
            "user-agent": `beacon-mcp/${BEACON_API_VERSION}`,
          },
          signal,
        });
        upstreamStatus = response.status;
        const body = await responseJson(response, BEACON_MAX_RESPONSE_BYTES);
        this.logger?.info(
          {
            ...currentRequestContext(),
            upstreamOperation: operation,
            upstreamStatus: response.status,
            latencyMs: Math.round(performance.now() - started),
            attempt,
            resultCount: resultCount(body),
            outcome: response.ok
              ? "success"
              : attempt === 1 && [502, 503, 504].includes(response.status)
                ? "retry"
                : "error",
          },
          "Beacon upstream request",
        );
        logged = true;
        if (response.ok) return body as BeaconResponse<O>;
        if (attempt === 1 && [502, 503, 504].includes(response.status)) {
          await this.sleep(25 + Math.floor(Math.random() * 50));
          if (options.signal?.aborted) throw options.signal.reason;
          continue;
        }
        throw errorFromResponse(
          response.status,
          body,
          response.headers.get("retry-after") ?? undefined,
        );
      } catch (error) {
        if (timeoutSignal.aborted) {
          this.logger?.warn(
            {
              ...currentRequestContext(),
              upstreamOperation: operation,
              upstreamStatus,
              latencyMs: Math.round(performance.now() - started),
              attempt,
              outcome: "timeout",
            },
            "Beacon upstream request",
          );
          throw new BeaconTimeoutError(
            `Beacon request timed out after ${timeoutMs}ms`,
          );
        }
        if (options.signal?.aborted) {
          this.logger?.info(
            {
              ...currentRequestContext(),
              upstreamOperation: operation,
              upstreamStatus,
              latencyMs: Math.round(performance.now() - started),
              attempt,
              outcome: "cancelled",
            },
            "Beacon upstream request",
          );
          throw error;
        }
        if (
          error instanceof BeaconUpstreamError ||
          (error instanceof Error && error.name !== "TypeError")
        ) {
          if (!logged) {
            this.logger?.warn(
              {
                ...currentRequestContext(),
                upstreamOperation: operation,
                upstreamStatus,
                latencyMs: Math.round(performance.now() - started),
                attempt,
                outcome: "error",
                errorType: error instanceof Error ? error.name : "UnknownError",
              },
              "Beacon upstream request",
            );
          }
          throw error;
        }
        this.logger?.warn(
          {
            ...currentRequestContext(),
            upstreamOperation: operation,
            latencyMs: Math.round(performance.now() - started),
            attempt,
            outcome: attempt === 1 ? "retry" : "error",
          },
          "Beacon upstream network failure",
        );
        if (attempt === 1) {
          await this.sleep(25 + Math.floor(Math.random() * 50));
          if (options.signal?.aborted) throw options.signal.reason;
          continue;
        }
        throw new BeaconUpstreamError("Beacon is temporarily unavailable");
      }
    }
    throw new BeaconUpstreamError("Beacon is temporarily unavailable");
  }
}
