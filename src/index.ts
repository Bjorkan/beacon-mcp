import { BeaconAdapter } from "./beacon/adapter.js";
import { BeaconClient } from "./beacon/client.js";
import { loadConfig } from "./config/config.js";
import { createLogger } from "./logging/logger.js";
import { buildHttpServer, shutdown } from "./server/http.js";

async function main(): Promise<void> {
  const config = loadConfig();
  const logger = createLogger(config.logLevel);
  const client = new BeaconClient(config, { logger });
  const adapter = new BeaconAdapter(client);
  const app = buildHttpServer(config, adapter, logger);

  if (config.mcpAuthToken === undefined)
    logger.warn("MCP_AUTH_TOKEN is unset; /mcp allows unauthenticated access");
  if (config.beaconBaseUrl.protocol !== "https:")
    logger.warn("BEACON_BASE_URL does not use HTTPS");

  let stopping = false;
  const stop = (signal: string): void => {
    if (stopping) return;
    stopping = true;
    void shutdown(app, config.shutdownGraceMs, signal, logger).then(
      () => {
        process.exitCode = 0;
      },
      (error: unknown) => {
        logger.error(
          { error: error instanceof Error ? error.message : "unknown" },
          "Graceful shutdown failed",
        );
        process.exitCode = 1;
      },
    );
  };
  process.once("SIGTERM", () => stop("SIGTERM"));
  process.once("SIGINT", () => stop("SIGINT"));

  await app.listen({ host: config.host, port: config.port });
  logger.info(
    { host: config.host, port: config.port, version: config.version },
    "beacon-mcp started",
  );
}

main().catch((error: unknown) => {
  const message =
    error instanceof Error ? error.message : "Unknown startup error";
  createLogger("info").fatal({ error: message }, "Startup failed");
  process.exitCode = 1;
});
