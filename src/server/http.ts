import { createMcpFastifyApp } from "@modelcontextprotocol/fastify";
import { toNodeHandler } from "@modelcontextprotocol/node";
import { createMcpHandler } from "@modelcontextprotocol/server";
import type { FastifyInstance } from "fastify";
import type { BeaconAdapter } from "../beacon/adapter.js";
import type { Config } from "../config/config.js";
import {
  currentRequestContext,
  type Logger,
  withRequestContext,
} from "../logging/logger.js";
import { createMcpServer } from "../mcp/create-server.js";
import { bearerAuthorized } from "./auth.js";

export function buildHttpServer(
  config: Config,
  adapter: BeaconAdapter,
  logger: Logger,
): FastifyInstance {
  const options = {
    host: config.host,
    allowedHosts: [...config.allowedHosts],
    allowedOrigins: [...config.allowedOrigins],
  };
  const app = createMcpFastifyApp(options);

  app.get("/healthz", async () => ({ status: "ok" }));
  app.get("/readyz", async () => ({ status: "ok" }));

  const handler = createMcpHandler(
    () => createMcpServer(adapter, config.version, logger),
    {
      legacy: config.mcpLegacyMode,
      onerror: (error) =>
        logger.error(
          { ...currentRequestContext(), error: error.message },
          "MCP handler error",
        ),
    },
  );
  const nodeHandler = toNodeHandler(handler, {
    onerror: (error) =>
      logger.error(
        { ...currentRequestContext(), error: error.message },
        "MCP adapter error",
      ),
    maxRequestBodySize: 1_048_576,
  });

  app.all("/mcp", async (request, reply) => {
    const started = performance.now();
    if (!bearerAuthorized(request.headers.authorization, config.mcpAuthToken)) {
      logger.warn(
        { requestId: request.id, status: 401 },
        "MCP authentication rejected",
      );
      reply.header("WWW-Authenticate", "Bearer");
      return reply.code(401).send({ error: "unauthorized" });
    }
    const body = request.body as
      { method?: unknown; params?: { name?: unknown } } | undefined;
    const method = typeof body?.method === "string" ? body.method : undefined;
    const tool =
      typeof body?.params?.name === "string" ? body.params.name : undefined;
    reply.raw.once("finish", () =>
      logger.info(
        {
          requestId: request.id,
          method,
          tool,
          status: reply.raw.statusCode,
          latencyMs: Math.round(performance.now() - started),
        },
        "MCP HTTP request",
      ),
    );
    await withRequestContext(
      { requestId: request.id, ...(tool ? { tool } : {}) },
      () =>
        nodeHandler(
          request.raw as typeof request.raw & { method: string; url: string },
          reply.raw,
          request.body,
        ),
    );
    return reply;
  });

  return app;
}

export async function shutdown(
  app: FastifyInstance,
  graceMs: number,
  signal: string,
  logger: Logger,
): Promise<void> {
  logger.info({ signal, graceMs }, "Graceful shutdown started");
  const timer = setTimeout(() => app.server.closeAllConnections(), graceMs);
  timer.unref();
  try {
    await app.close();
  } finally {
    clearTimeout(timer);
  }
  logger.info("Graceful shutdown complete");
}
