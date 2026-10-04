export type QueryValue = string | number | boolean | undefined;

export function normalizeIatas(
  iatas?: readonly string[],
): string[] | undefined {
  if (iatas === undefined) return undefined;
  return iatas.map((iata) => {
    const normalized = iata.trim().toUpperCase();
    if (!/^[A-Z0-9]{3}$/.test(normalized))
      throw new BeaconInputError(`Invalid IATA identifier: ${iata}`);
    return normalized;
  });
}

export function buildQuery(
  values: Readonly<Record<string, QueryValue>>,
): URLSearchParams {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(values)) {
    if (value !== undefined) query.set(key, String(value));
  }
  return query;
}

export function boundedLimit(limit = 20): number {
  if (!Number.isInteger(limit) || limit < 1 || limit > 50)
    throw new BeaconInputError("limit must be an integer from 1 to 50");
  return limit;
}
import { BeaconInputError } from "./errors.js";
