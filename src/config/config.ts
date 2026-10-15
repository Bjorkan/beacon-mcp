import { z } from "zod/v4";

export type LogLevel =
  "fatal" | "error" | "warn" | "info" | "debug" | "trace" | "silent";

const envSchema = z.object({
  HOST: z.string().min(1).default("0.0.0.0"),
  PORT: z.coerce.number().int().min(1024).max(65535).default(3000),
  BEACON_BASE_URL: z.url(),
  BEACON_TIMEOUT_MS: z.coerce
    .number()
    .int()
    .min(100)
    .max(120_000)
    .default(10_000),
  BEACON_STATS_TIMEOUT_MS: z.coerce
    .number()
    .int()
    .min(100)
    .max(300_000)
    .default(20_000),
  LOG_LEVEL: z
    .enum(["fatal", "error", "warn", "info", "debug", "trace", "silent"])
    .default("info"),
  BEACON_SUBSCRIPTIONS_ENABLED: z.enum(["true", "false"]).default("false"),
  BEACON_WS_URL: z.union([z.literal(""), z.url()]).optional(),
  BEACON_SUBSCRIPTIONS_MAX_STREAMS: z.coerce
    .number()
    .int()
    .min(1)
    .max(1024)
    .default(64),
  BEACON_SUBSCRIPTIONS_DEBOUNCE_MS: z.coerce
    .number()
    .int()
    .min(0)
    .max(5000)
    .default(500),
  SHUTDOWN_GRACE_MS: z.coerce
    .number()
    .int()
    .min(100)
    .max(60_000)
    .default(10_000),
});

export type Config = Readonly<{
  host: string;
  port: number;
  beaconBaseUrl: URL;
  beaconTimeoutMs: number;
  beaconStatsTimeoutMs: number;
  logLevel: LogLevel;
  shutdownGraceMs: number;
  /** Optional for callers constructing Config directly; loadConfig always supplies it. */
  subscriptions?: Readonly<{
    enabled: boolean;
    wsUrl: URL;
    maxStreams: number;
    debounceMs: number;
  }>;
}>;

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = envSchema.parse(env);
  const beaconBaseUrl = new URL(parsed.BEACON_BASE_URL);
  if (!["http:", "https:"].includes(beaconBaseUrl.protocol)) {
    throw new Error("BEACON_BASE_URL must use http or https");
  }
  if (
    beaconBaseUrl.username ||
    beaconBaseUrl.password ||
    beaconBaseUrl.search ||
    beaconBaseUrl.hash
  ) {
    throw new Error(
      "BEACON_BASE_URL must not contain credentials, query parameters, or a fragment",
    );
  }
  beaconBaseUrl.pathname = `${beaconBaseUrl.pathname.replace(/\/+$/, "")}/`;

  // Beacon Server exposes /ws at its root, not underneath /api/v1.
  const wsUrl = parsed.BEACON_WS_URL
    ? new URL(parsed.BEACON_WS_URL)
    : new URL(`/ws`, beaconBaseUrl.origin);
  if (!parsed.BEACON_WS_URL)
    wsUrl.protocol = beaconBaseUrl.protocol === "https:" ? "wss:" : "ws:";
  if (
    !["ws:", "wss:"].includes(wsUrl.protocol) ||
    wsUrl.username ||
    wsUrl.password ||
    wsUrl.search ||
    wsUrl.hash ||
    !wsUrl.pathname.endsWith("/ws")
  ) {
    throw new Error(
      "BEACON_WS_URL must be a ws/wss URL ending in /ws without credentials, query or fragment",
    );
  }

  return Object.freeze({
    host: parsed.HOST,
    port: parsed.PORT,
    beaconBaseUrl,
    beaconTimeoutMs: parsed.BEACON_TIMEOUT_MS,
    beaconStatsTimeoutMs: parsed.BEACON_STATS_TIMEOUT_MS,
    logLevel: parsed.LOG_LEVEL,
    shutdownGraceMs: parsed.SHUTDOWN_GRACE_MS,
    subscriptions: Object.freeze({
      enabled: parsed.BEACON_SUBSCRIPTIONS_ENABLED === "true",
      wsUrl,
      maxStreams: parsed.BEACON_SUBSCRIPTIONS_MAX_STREAMS,
      debounceMs: parsed.BEACON_SUBSCRIPTIONS_DEBOUNCE_MS,
    }),
  });
}
