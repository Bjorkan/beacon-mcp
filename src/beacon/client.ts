import type { Config } from "../config/config.js";
import type { paths } from "../generated/beacon-api.js";
import type { Logger } from "../logging/logger.js";
import { currentRequestContext } from "../logging/logger.js";
import {
  BeaconInputError,
  BeaconResponseTooLargeError,
  BeaconTimeoutError,
  BeaconUpstreamError,
  errorFromResponse,
} from "./errors.js";
import { buildQuery, type QueryValue } from "./query.js";
import { BEACON_API_VERSION } from "./version.js";

const BEACON_MAX_RESPONSE_BYTES = 5_242_880;
const BEACON_MAX_URL_LENGTH = 8_192;

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

/** Result of a bounded incremental read of a top-level JSON array. */
export interface ArrayPage {
  items: unknown[];
  /**
   * True when the upstream array terminator was seen, so the items are the
   * complete result set. False when reading stopped early because the item
   * budget was reached, the byte cap was hit, or the stream ended
   * prematurely with elements already parsed.
   */
  complete: boolean;
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

/**
 * Incremental scanner for top-level JSON arrays. Reads chunk by chunk,
 * parses each complete array element as it closes, and stops reading as
 * soon as maxItems elements are available, so a huge upstream array never
 * has to be transferred or buffered in full. Non-array bodies fall back to
 * whole-body buffering with the same byte cap.
 */
async function readArrayPage(
  response: Response,
  maxBytes: number,
  maxItems: number,
): Promise<ArrayPage> {
  if (response.status === 204) return { items: [], complete: true };
  if (!response.body) return { items: [], complete: true };

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const items: unknown[] = [];
  let byteCount = 0;
  let buffer = "";
  // JSON lexer state, carried across chunks.
  let started = false; // opening '[' seen
  let closed = false; // closing ']' seen
  let depth = 0; // nesting depth of the element currently being read
  let inString = false;
  let escaped = false;
  let itemStart = -1; // index of the first character of the current element
  let resume = 0; // index within buffer where scanning resumes next chunk
  let truncated = false;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      byteCount += value.byteLength;
      if (byteCount > maxBytes) {
        await reader.cancel();
        if (items.length > 0) {
          return { items, complete: false };
        }
        throw new BeaconResponseTooLargeError(
          `Beacon response exceeded the ${maxBytes} byte limit`,
          response.status,
        );
      }
      buffer += decoder.decode(value, { stream: true });
      for (let i = resume; i < buffer.length; i += 1) {
        const char = buffer.charAt(i);
        if (inString) {
          if (escaped) {
            escaped = false;
          } else if (char === "\\") {
            escaped = true;
          } else if (char === '"') {
            inString = false;
          }
          continue;
        }
        if (!started) {
          if (char === "[") {
            started = true;
          } else if (!/\s/.test(char)) {
            // Not an array response; the whole-body path below handles it.
            started = true;
            depth = -1;
            itemStart = 0;
          }
          continue;
        }
        if (depth === -1) continue; // draining a non-array body
        if (depth === 0) {
          if (char === "]") {
            if (itemStart >= 0) {
              items.push(JSON.parse(buffer.slice(itemStart, i)));
              itemStart = -1;
            }
            closed = true;
            break;
          }
          if (itemStart < 0) {
            if (char === "," || /\s/.test(char)) continue;
            itemStart = i;
          } else if (char === ",") {
            items.push(JSON.parse(buffer.slice(itemStart, i)));
            itemStart = -1;
            if (items.length >= maxItems) break;
            continue;
          }
          if (char === '"' || char === "{" || char === "[") {
            if (char === '"') inString = true;
            else depth = 1;
          }
        } else {
          if (char === '"') {
            inString = true;
          } else if (char === "{" || char === "[") {
            depth += 1;
          } else if (char === "}" || char === "]") {
            depth -= 1;
            if (depth === 0) {
              items.push(JSON.parse(buffer.slice(itemStart, i + 1)));
              itemStart = -1;
              if (items.length >= maxItems) break;
            }
          }
        }
      }
      if (closed || items.length >= maxItems) {
        if (!closed) truncated = true;
        break;
      }
      // Keep only the unconsumed tail to bound memory across chunks: either
      // an unfinished element (depth non-zero or a scalar element in
      // progress) or nothing at all. Chars inside the kept tail were
      // already scanned, so scanning resumes past them.
      const keep = itemStart >= 0 ? itemStart : buffer.length;
      resume = buffer.length - keep;
      buffer = keep < buffer.length ? buffer.slice(keep) : "";
      if (itemStart >= 0) itemStart = 0;
    }
    if (closed) {
      return { items, complete: true };
    }
    if (truncated || items.length > 0) {
      return { items, complete: false };
    }
    // The stream ended without a single complete element and without an
    // array terminator: treat it like any other body and parse it whole.
    let fallback: unknown;
    try {
      fallback = JSON.parse(`${buffer}${decoder.decode()}`);
    } catch {
      throw new BeaconUpstreamError(
        "Beacon returned an invalid JSON response",
        response.status,
      );
    }
    if (Array.isArray(fallback)) return { items: fallback, complete: true };
    if (
      fallback &&
      typeof fallback === "object" &&
      Array.isArray((fallback as Record<string, unknown>)["items"])
    ) {
      return {
        items: (fallback as Record<string, unknown>)["items"] as unknown[],
        complete: true,
      };
    }
    return { items: [], complete: true };
  } finally {
    reader.releaseLock();
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

  /**
   * Fetch one pinned public GET operation and parse the JSON body.
   */
  async request<O extends BeaconOperation>(
    operation: O,
    options: BeaconRequest<O> = {},
  ): Promise<BeaconResponse<O>> {
    return this.execute(operation, options, (response) =>
      responseJson(response, BEACON_MAX_RESPONSE_BYTES),
    ) as Promise<BeaconResponse<O>>;
  }

  /**
   * Fetch an operation whose upstream response is a top-level JSON array,
   * reading at most maxItems elements before cancelling the transfer. Use
   * this for array endpoints without server-side pagination so the MCP
   * tool's limit bounds the actual work instead of only slicing the
   * already-downloaded result.
   */
  async requestArrayPage<O extends BeaconOperation>(
    operation: O,
    options: BeaconRequest<O> & { maxItems: number },
  ): Promise<ArrayPage> {
    const { maxItems, ...rest } = options;
    if (!Number.isInteger(maxItems) || maxItems < 1) {
      throw new BeaconInputError("maxItems must be a positive integer");
    }
    return this.execute(operation, rest, (response) =>
      readArrayPage(response, BEACON_MAX_RESPONSE_BYTES, maxItems),
    );
  }

  private async execute<O extends BeaconOperation, R>(
    operation: O,
    options: BeaconRequest<O>,
    consume: (response: Response) => Promise<R>,
  ): Promise<R> {
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
    if (url.href.length > BEACON_MAX_URL_LENGTH) {
      throw new BeaconInputError("Beacon request filters are too large");
    }
    // Cross-IATA route search is an expensive upstream join over whole
    // stored routes, so it shares the analytics timeout budget.
    const expensive =
      operation === "networkOverview" ||
      operation === "networkSeries" ||
      operation === "compareObservers" ||
      operation === "crossRoutes";
    const timeoutMs = expensive
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
          redirect: "error",
          headers: {
            accept: "application/json",
            "user-agent": `beacon-mcp/${BEACON_API_VERSION}`,
          },
          signal,
        });
        upstreamStatus = response.status;
        // Non-ok bodies are only read to extract the error envelope and are
        // never returned, so the cast below is sound.
        const body: R = response.ok
          ? await consume(response)
          : ((await responseJson(response, BEACON_MAX_RESPONSE_BYTES)) as R);
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
        if (response.ok) return body;
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
