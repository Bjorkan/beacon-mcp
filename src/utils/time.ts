import { BeaconInputError } from "../beacon/errors.js";

export function toEpochMilliseconds(value: string, field = "time"): number {
  const match =
    /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,3}))?Z$/.exec(
      value,
    );
  if (!match) {
    throw new BeaconInputError(`${field} must be an RFC3339 UTC timestamp`);
  }
  const result = Date.parse(value);
  // Impossible calendar dates (for example month 13 or February 30) roll
  // over or become NaN; both are calendar errors, not range errors.
  const parsed = new Date(result);
  if (
    Number.isNaN(result) ||
    parsed.getUTCFullYear() !== Number(match[1]) ||
    parsed.getUTCMonth() + 1 !== Number(match[2]) ||
    parsed.getUTCDate() !== Number(match[3]) ||
    parsed.getUTCHours() !== Number(match[4]) ||
    parsed.getUTCMinutes() !== Number(match[5]) ||
    parsed.getUTCSeconds() !== Number(match[6])
  ) {
    throw new BeaconInputError(`${field} must be a valid calendar timestamp`);
  }
  if (!Number.isSafeInteger(result)) {
    throw new BeaconInputError(`${field} is outside the supported range`);
  }
  return result;
}

export function timeRange(
  since: string,
  until: string,
): { since: number; until: number };
export function timeRange(
  since?: string,
  until?: string,
): { since?: number; until?: number };
export function timeRange(
  since?: string,
  until?: string,
): { since?: number; until?: number } {
  const result = {
    ...(since === undefined
      ? {}
      : { since: toEpochMilliseconds(since, "since") }),
    ...(until === undefined
      ? {}
      : { until: toEpochMilliseconds(until, "until") }),
  };
  if (
    result.since !== undefined &&
    result.until !== undefined &&
    result.since >= result.until
  ) {
    throw new BeaconInputError("since must be earlier than until");
  }
  return result;
}
