import { z } from "zod/v4";

const csv = z
  .string()
  .default("localhost,127.0.0.1,[::1]")
  .transform((value) =>
    value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  );

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
  BEACON_MAX_RESPONSE_BYTES: z.coerce
    .number()
    .int()
    .min(65_536)
    .max(52_428_800)
    .default(5_242_880),
  MCP_AUTH_TOKEN: z.preprocess(
    (value) => (value === "" ? undefined : value),
    z.string().min(16).optional(),
  ),
  MCP_LEGACY_MODE: z.enum(["stateless", "reject"]).default("stateless"),
  MCP_ALLOWED_HOSTS: csv,
  MCP_ALLOWED_ORIGINS: csv,
  LOG_LEVEL: z
    .enum(["fatal", "error", "warn", "info", "debug", "trace", "silent"])
    .default("info"),
  SHUTDOWN_GRACE_MS: z.coerce
    .number()
    .int()
    .min(100)
    .max(60_000)
    .default(10_000),
  APP_VERSION: z.string().min(1).default("0.1.0"),
});

export type Config = Readonly<{
  host: string;
  port: number;
  beaconBaseUrl: URL;
  beaconTimeoutMs: number;
  beaconStatsTimeoutMs: number;
  beaconMaxResponseBytes: number;
  mcpAuthToken?: string;
  mcpLegacyMode: "stateless" | "reject";
  allowedHosts: readonly string[];
  allowedOrigins: readonly string[];
  logLevel: LogLevel;
  shutdownGraceMs: number;
  version: string;
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

  return Object.freeze({
    host: parsed.HOST,
    port: parsed.PORT,
    beaconBaseUrl,
    beaconTimeoutMs: parsed.BEACON_TIMEOUT_MS,
    beaconStatsTimeoutMs: parsed.BEACON_STATS_TIMEOUT_MS,
    beaconMaxResponseBytes: parsed.BEACON_MAX_RESPONSE_BYTES,
    ...(parsed.MCP_AUTH_TOKEN === undefined
      ? {}
      : { mcpAuthToken: parsed.MCP_AUTH_TOKEN }),
    mcpLegacyMode: parsed.MCP_LEGACY_MODE,
    allowedHosts: Object.freeze(parsed.MCP_ALLOWED_HOSTS),
    allowedOrigins: Object.freeze(parsed.MCP_ALLOWED_ORIGINS),
    logLevel: parsed.LOG_LEVEL,
    shutdownGraceMs: parsed.SHUTDOWN_GRACE_MS,
    version: parsed.APP_VERSION,
  });
}
