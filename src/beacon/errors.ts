export interface BeaconErrorBody {
  error?: { code?: string; message?: string };
}

/**
 * Canonical gateway error classification published in tool error results.
 * The vocabulary follows the gRPC code names that clients commonly map
 * against, plus a gateway-specific RESPONSE_TOO_LARGE for the bounded
 * upstream response cap.
 */
export type BeaconErrorCode =
  | "INVALID_ARGUMENT"
  | "NOT_FOUND"
  | "DEADLINE_EXCEEDED"
  | "RESPONSE_TOO_LARGE"
  | "RESOURCE_EXHAUSTED"
  | "UNAVAILABLE"
  | "INTERNAL";

export class BeaconError extends Error {
  constructor(
    message: string,
    readonly status?: number,
    readonly code?: string,
    readonly retryAfter?: string,
  ) {
    super(message);
    this.name = new.target.name;
  }
}
export class BeaconInputError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BeaconInputError";
  }
}
export class BeaconValidationError extends BeaconError {}
export class BeaconNotFoundError extends BeaconError {}
export class BeaconRateLimitError extends BeaconError {}
export class BeaconTimeoutError extends BeaconError {}
export class BeaconUpstreamError extends BeaconError {}
export class BeaconResponseTooLargeError extends BeaconUpstreamError {}

export function errorCode(error: unknown): BeaconErrorCode {
  if (error instanceof BeaconInputError) return "INVALID_ARGUMENT";
  if (error instanceof BeaconValidationError) return "INVALID_ARGUMENT";
  if (error instanceof BeaconNotFoundError) return "NOT_FOUND";
  if (error instanceof BeaconTimeoutError) return "DEADLINE_EXCEEDED";
  // Subclass of BeaconUpstreamError, so it must be tested first.
  if (error instanceof BeaconResponseTooLargeError) return "RESPONSE_TOO_LARGE";
  if (error instanceof BeaconRateLimitError) return "RESOURCE_EXHAUSTED";
  if (error instanceof BeaconUpstreamError) return "UNAVAILABLE";
  if (error instanceof BeaconError) return "UNAVAILABLE";
  return "INTERNAL";
}

export function errorFromResponse(
  status: number,
  body: unknown,
  retryAfter?: string,
): BeaconError {
  const envelope = body as BeaconErrorBody;
  const message =
    status >= 500
      ? `Beacon request failed with HTTP ${status}`
      : (envelope?.error?.message ??
        `Beacon request failed with HTTP ${status}`);
  const code = envelope?.error?.code;
  if (status === 400) return new BeaconValidationError(message, status, code);
  if (status === 404) return new BeaconNotFoundError(message, status, code);
  if (status === 429)
    return new BeaconRateLimitError(message, status, code, retryAfter);
  return new BeaconUpstreamError(message, status, code, retryAfter);
}
