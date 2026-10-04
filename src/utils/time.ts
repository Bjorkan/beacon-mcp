import { BeaconInputError } from "../beacon/errors.js";

export function toEpochMilliseconds(value: string, field = "time"): number {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/.test(value)) {
    throw new BeaconInputError(`${field} must be an RFC3339 UTC timestamp`);
  }
  const result = Date.parse(value);
  if (!Number.isSafeInteger(result))
    throw new BeaconInputError(`${field} is outside the supported range`);
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
