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
  if (!Number.isSafeInteger(result))
    throw new BeaconInputError(`${field} is outside the supported range`);
  const parsed = new Date(result);
  const [, year, month, day, hour, minute, second] = match;
  if (
    parsed.getUTCFullYear() !== Number(year) ||
    parsed.getUTCMonth() + 1 !== Number(month) ||
    parsed.getUTCDate() !== Number(day) ||
    parsed.getUTCHours() !== Number(hour) ||
    parsed.getUTCMinutes() !== Number(minute) ||
    parsed.getUTCSeconds() !== Number(second)
  ) {
    throw new BeaconInputError(`${field} must be a valid calendar timestamp`);
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
