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
    logLevel: parsed.LOG_LEVEL,
    shutdownGraceMs: parsed.SHUTDOWN_GRACE_MS,
  });
}
