import { describe, expect, it } from "vitest";
import {
  boundedLimit,
  buildQuery,
  normalizeIatas,
} from "../../src/beacon/query.js";
import { timeRange, toEpochMilliseconds } from "../../src/utils/time.js";

describe("input normalization", () => {
  it("converts strict UTC RFC3339 timestamps", () => {
    expect(toEpochMilliseconds("2026-10-04T15:30:00Z")).toBe(1791127800000);
    expect(() => toEpochMilliseconds("2026-10-04 15:30")).toThrow("RFC3339");
    expect(() => toEpochMilliseconds("2026-02-30T15:30:00Z")).toThrow(
      "calendar",
    );
    expect(() => toEpochMilliseconds("2026-13-01T00:00:00Z")).toThrow(
      "calendar",
    );
    expect(() => toEpochMilliseconds("2026-02-29T00:00:00Z")).toThrow(
      "calendar",
    );
    expect(toEpochMilliseconds("2024-02-29T00:00:00Z")).toBe(1709164800000);
    expect(() =>
      timeRange("2026-10-05T00:00:00Z", "2026-10-04T00:00:00Z"),
    ).toThrow("earlier");
  });

  it("normalizes IATAs and builds encoded queries", () => {
    expect(normalizeIatas([" arn ", "got"])).toEqual(["ARN", "GOT"]);
    expect(
      buildQuery({
        iatas: "ARN,GOT",
        scope: "#se",
        absent: undefined,
      }).toString(),
    ).toBe("iatas=ARN%2CGOT&scope=%23se");
    expect(() => normalizeIatas(["bad value"])).toThrow("Invalid IATA");
  });

  it("enforces MCP result bounds", () => {
    expect(boundedLimit()).toBe(20);
    expect(boundedLimit(50)).toBe(50);
    expect(() => boundedLimit(51)).toThrow();
  });
});
