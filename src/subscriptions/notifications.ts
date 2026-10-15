import type { ServerNotifier } from "@modelcontextprotocol/server";
import type { Logger } from "../logging/logger.js";
import { beaconResourceUri } from "../mcp/resources.js";

/** Bounded, process-local coalescer; the SDK filters actual listen streams by URI. */
export class ResourceNotifications {
  private readonly pending = new Map<string, ReturnType<typeof setTimeout>>();
  private closed = false;

  constructor(
    private readonly notify: Pick<ServerNotifier, "resourceUpdated">,
    private readonly logger: Logger,
    private readonly debounceMs: number,
    private readonly maxPending = 1024,
  ) {}

  received(event: string, data: unknown): void {
    if (
      this.closed ||
      typeof data !== "object" ||
      data === null ||
      Array.isArray(data)
    )
      return;
    const item = data as Record<string, unknown>;
    const kind =
      event === "nodeUpdate"
        ? "nodes"
        : event === "observerStatus"
          ? "observers"
          : undefined;
    const id = kind === "nodes" ? item["nodeId"] : item["observerId"];
    if (!kind || typeof id !== "string") return;
    const uri = beaconResourceUri(kind, id);
    if (!uri || this.pending.has(uri)) return;
    if (this.pending.size >= this.maxPending) {
      this.logger.warn(
        { pending: this.pending.size },
        "Beacon notification queue full; update dropped",
      );
      return;
    }
    const timer = setTimeout(() => {
      this.pending.delete(uri);
      if (!this.closed) this.notify.resourceUpdated(uri);
    }, this.debounceMs);
    timer.unref();
    this.pending.set(uri, timer);
  }

  stop(): void {
    this.closed = true;
    for (const timer of this.pending.values()) clearTimeout(timer);
    this.pending.clear();
  }
}
