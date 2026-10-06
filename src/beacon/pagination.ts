import type { ArrayPage } from "./client.js";

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

/**
 * Build a truncated page from a bounded incremental read of a top-level
 * upstream array. truncated is reported both when more items existed than
 * the requested limit and when the read stopped before the upstream array
 * terminator was seen, so data was deliberately left unread.
 */
export function truncatedArrayPage(page: ArrayPage, limit: number): Page {
  const truncated = !page.complete || page.items.length > limit;
  return {
    items: page.items.slice(0, limit),
    pagination: {
      hasMore: false,
      ...(truncated ? { truncated: true } : {}),
    },
  };
}

export function toOffsetArrayPage(
  value: unknown,
  limit: number,
  cursor = 0,
): Page {
  const source = Array.isArray(value) ? value : [];
  const items = source.slice(cursor, cursor + limit);
  const nextOffset = cursor + items.length;
  const hasMore = nextOffset < source.length;
  return {
    items,
    pagination: {
      hasMore,
      ...(hasMore ? { nextCursor: { cursor: nextOffset } } : {}),
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
