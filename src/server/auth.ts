import { createHash, timingSafeEqual } from "node:crypto";

function digest(value: string): Buffer {
  return createHash("sha256").update(value, "utf8").digest();
}

export function bearerAuthorized(
  header: string | undefined,
  expected: string | undefined,
): boolean {
  if (expected === undefined) return true;
  if (header === undefined || !header.startsWith("Bearer ")) return false;
  const supplied = header.slice(7);
  return (
    supplied.length > 0 && timingSafeEqual(digest(supplied), digest(expected))
  );
}
