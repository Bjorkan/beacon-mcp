import type { CallToolResult } from "@modelcontextprotocol/server";
import {
  BeaconError,
  BeaconInputError,
  BeaconRateLimitError,
  errorCode,
} from "../beacon/errors.js";

function structured(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : { data: value };
}

export function toolResult(value: unknown): CallToolResult {
  const payload = structured(value);
  const count = Array.isArray(payload["items"])
    ? payload["items"].length
    : undefined;
  return {
    content: [
      {
        type: "text",
        text:
          count === undefined
            ? "Beacon request succeeded."
            : `Beacon returned ${count} item(s).`,
      },
    ],
    structuredContent: payload,
  };
}

export function toolError(error: unknown): CallToolResult {
  const known =
    error instanceof BeaconError || error instanceof BeaconInputError;
  const message = known ? error.message : "Internal gateway error";
  const details =
    error instanceof BeaconError
      ? {
          code: errorCode(error),
          type: error.name,
          ...(error.status === undefined ? {} : { status: error.status }),
          ...(error.code === undefined ? {} : { upstreamCode: error.code }),
          ...(error instanceof BeaconRateLimitError && error.retryAfter
            ? { retryAfter: error.retryAfter }
            : {}),
        }
      : error instanceof BeaconInputError
        ? { code: "INVALID_ARGUMENT", type: "ValidationError" }
        : { code: "INTERNAL", type: "InternalError" };
  return {
    isError: true,
    content: [{ type: "text", text: message }],
    structuredContent: { error: details },
  };
}
