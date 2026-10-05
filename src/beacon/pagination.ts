export interface Page {
  items: unknown[];
  pagination: {
    hasMore: boolean;
    nextCursor?: Record<string, unknown>;
    truncated?: boolean;
  };
}

export function toPage(value: unknown, limit: number): Page {
  const record =
    value && typeof value === "object"
      ? (value as Record<string, unknown>)
      : {};
  const source = Array.isArray(value)
    ? value
    : Array.isArray(record["items"])
      ? record["items"]
      : [];
  const items = source.slice(0, limit);
  const nextCursor =
    record["nextPageCursor"] !== undefined
      ? { pageCursor: record["nextPageCursor"] }
      : record["nextCursor"] !== undefined
        ? { cursor: record["nextCursor"] }
        : {};
  const hasMore = Boolean(record["hasMore"]) || source.length > limit;
  return {
    items,
    pagination: {
      hasMore,
      ...(Object.keys(nextCursor).length ? { nextCursor } : {}),
    },
  };
}

export function toTruncatedPage(value: unknown, limit: number): Page {
  const source = Array.isArray(value) ? value : [];
  return {
    items: source.slice(0, limit),
    pagination: {
      hasMore: false,
      ...(source.length > limit ? { truncated: true } : {}),
    },
  };
}

export function toCursorPage(
  value: unknown,
  limit: number,
  cursor: (last: Record<string, unknown>) => Record<string, unknown>,
): Page {
  const source = Array.isArray(value) ? value : [];
  const items = source.slice(0, limit);
  const last = items.at(-1);
  if (items.length < limit || !last || typeof last !== "object") {
    return { items, pagination: { hasMore: false } };
  }
  const nextCursor = Object.fromEntries(
    Object.entries(cursor(last as Record<string, unknown>)).filter(
      ([, item]) => item !== undefined,
    ),
  );
  return {
    items,
    pagination: {
      hasMore: Object.keys(nextCursor).length > 0,
      ...(Object.keys(nextCursor).length > 0 ? { nextCursor } : {}),
    },
  };
}
