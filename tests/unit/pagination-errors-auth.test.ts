import { describe, expect, it } from "vitest";
import {
  BeaconInputError,
  BeaconNotFoundError,
  BeaconRateLimitError,
  BeaconResponseTooLargeError,
  BeaconTimeoutError,
  BeaconUpstreamError,
  BeaconValidationError,
  errorFromResponse,
  errorCode,
} from "../../src/beacon/errors.js";
import { toPage, truncatedArrayPage } from "../../src/beacon/pagination.js";
import { toolError } from "../../src/mcp/result.js";

describe("pagination and errors", () => {
  it("preserves pagination state and bounds items", () => {
    expect(
      toPage(
        {
          items: [1, 2, 3],
          hasMore: true,
          nextCursor: 10,
          nextPageCursor: "opaque",
        },
        2,
      ),
    ).toEqual({
      items: [1, 2],
      pagination: {
        hasMore: true,
        nextCursor: { pageCursor: "opaque" },
      },
    });
  });

  it("maps Beacon error envelopes and Retry-After", () => {
    expect(
      errorFromResponse(400, {
        error: { code: "bad_request", message: "bad" },
      }),
    ).toBeInstanceOf(BeaconValidationError);
    expect(
      errorFromResponse(404, { error: { message: "missing" } }),
    ).toBeInstanceOf(BeaconNotFoundError);
    const rate = errorFromResponse(
      429,
      { error: { code: "rate_limited", message: "slow" } },
      "60",
    );
    expect(rate).toBeInstanceOf(BeaconRateLimitError);
    expect(rate.retryAfter).toBe("60");
  });

  it("does not disclose unexpected internal errors", () => {
    expect(toolError(new Error("secret implementation detail"))).toMatchObject({
      isError: true,
      content: [{ text: "Internal gateway error" }],
      structuredContent: { error: { code: "INTERNAL", type: "InternalError" } },
    });
  });

  it("classifies every gateway error with a canonical code", () => {
    expect(errorCode(new BeaconInputError("bad input"))).toBe(
      "INVALID_ARGUMENT",
    );
    expect(
      errorCode(errorFromResponse(400, { error: { message: "bad" } })),
    ).toBe("INVALID_ARGUMENT");
    expect(
      errorCode(errorFromResponse(404, { error: { message: "missing" } })),
    ).toBe("NOT_FOUND");
    expect(errorCode(new BeaconTimeoutError("slow"))).toBe("DEADLINE_EXCEEDED");
    expect(errorCode(new BeaconResponseTooLargeError("huge", 200))).toBe(
      "RESPONSE_TOO_LARGE",
    );
    expect(errorCode(new BeaconRateLimitError("slow down", 429))).toBe(
      "RESOURCE_EXHAUSTED",
    );
    expect(errorCode(new BeaconUpstreamError("down", 503))).toBe("UNAVAILABLE");
    expect(errorCode(new Error("unexpected"))).toBe("INTERNAL");
  });

  it("publishes canonical codes and upstream details in error results", () => {
    expect(
      toolError(
        errorFromResponse(404, {
          error: { code: "not_found", message: "packet not found" },
        }),
      ),
    ).toMatchObject({
      isError: true,
      structuredContent: {
        error: {
          code: "NOT_FOUND",
          type: "BeaconNotFoundError",
          status: 404,
          upstreamCode: "not_found",
        },
      },
    });
    expect(toolError(new BeaconInputError("bad"))).toMatchObject({
      structuredContent: {
        error: { code: "INVALID_ARGUMENT", type: "ValidationError" },
      },
    });
    expect(toolError(new BeaconTimeoutError("timed out"))).toMatchObject({
      structuredContent: {
        error: { code: "DEADLINE_EXCEEDED", type: "BeaconTimeoutError" },
      },
    });
  });

  it("bounds streamed truncated pages", () => {
    expect(truncatedArrayPage({ items: [1, 2, 3], complete: true }, 2)).toEqual(
      {
        items: [1, 2],
        pagination: { hasMore: false, truncated: true },
      },
    );
    expect(truncatedArrayPage({ items: [1, 2], complete: false }, 2)).toEqual({
      items: [1, 2],
      pagination: { hasMore: false, truncated: true },
    });
    expect(truncatedArrayPage({ items: [1], complete: true }, 2)).toEqual({
      items: [1],
      pagination: { hasMore: false },
    });
  });

  it("does not disclose upstream 5xx response messages", () => {
    const error = errorFromResponse(500, {
      error: { code: "internal", message: "database secret" },
    });
    expect(error.message).toBe("Beacon request failed with HTTP 500");
  });
});
