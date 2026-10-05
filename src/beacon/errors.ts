export interface BeaconErrorBody {
  error?: { code?: string; message?: string };
}

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
