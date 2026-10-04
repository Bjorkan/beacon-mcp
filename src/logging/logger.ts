import { AsyncLocalStorage } from "node:async_hooks";
import pino, { type Logger } from "pino";
import type { LogLevel } from "../config/config.js";

interface RequestLogContext {
  requestId: string;
  tool?: string;
}

const requestContext = new AsyncLocalStorage<RequestLogContext>();

export function createLogger(level: LogLevel): Logger {
  return pino({
    level,
    base: null,
    redact: {
      paths: ["authorization", "token", "headers.authorization"],
      censor: "[Redacted]",
    },
  });
}

export function withRequestContext<T>(
  context: RequestLogContext,
  callback: () => T,
): T {
  return requestContext.run(context, callback);
}

export function currentRequestContext(): RequestLogContext | undefined {
  return requestContext.getStore();
}

export type { Logger };
