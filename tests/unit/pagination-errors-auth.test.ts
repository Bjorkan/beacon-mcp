import { describe, expect, it } from "vitest";
import {
  errorFromResponse,
  BeaconNotFoundError,
  BeaconRateLimitError,
  BeaconValidationError,
} from "../../src/beacon/errors.js";
import { toPage } from "../../src/beacon/pagination.js";
import { bearerAuthorized } from "../../src/server/auth.js";
import { toolError } from "../../src/mcp/result.js";

describe("pagination, errors, and auth", () => {
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
        nextCursor: { nextCursor: 10, nextPageCursor: "opaque" },
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

  it("checks bearer tokens without accepting malformed values", () => {
    expect(bearerAuthorized(undefined, undefined)).toBe(true);
    expect(
      bearerAuthorized("Bearer correct-token-123", "correct-token-123"),
    ).toBe(true);
    expect(
      bearerAuthorized("bearer correct-token-123", "correct-token-123"),
    ).toBe(false);
  });

  it("does not disclose unexpected internal errors", () => {
    expect(toolError(new Error("secret implementation detail"))).toMatchObject({
      isError: true,
      content: [{ text: "Internal gateway error" }],
      structuredContent: { error: { type: "InternalError" } },
    });
  });
});
