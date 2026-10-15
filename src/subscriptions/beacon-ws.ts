import type { Logger } from "../logging/logger.js";

interface WsEnvelope {
  v?: unknown;
  type?: unknown;
  event?: unknown;
  data?: unknown;
  id?: unknown;
  droppedCount?: unknown;
}

/** One upstream connection per MCP process. Beacon WS v1 uses JSON application pings. */
export class BeaconWsBridge {
  private socket: WebSocket | undefined;
  private running = false;
  private ready = false;
  private attempt = 0;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private pingTimer: ReturnType<typeof setInterval> | undefined;
  private handshakeTimer: ReturnType<typeof setTimeout> | undefined;
  private awaitingPong = false;
  private pingId = 0;

  constructor(
    private readonly url: URL,
    private readonly logger: Logger,
    private readonly onUpdate: (event: string, data: unknown) => void,
    private readonly socketFactory: (url: string) => WebSocket = (url) =>
      new WebSocket(url),
  ) {}

  start(): void {
    if (this.running) return;
    this.running = true;
    this.connect();
  }

  stop(): void {
    if (!this.running) return;
    this.running = false;
    this.clearTimers();
    this.ready = false;
    const socket = this.socket;
    this.socket = undefined;
    if (socket) this.closeSocket(socket);
  }

  private closeSocket(socket: WebSocket): void {
    try {
      if (socket.readyState !== WebSocket.CLOSED) socket.close();
    } catch {
      // Some runtimes reject close() during CONNECTING. The open handler
      // closes stale sockets once the handshake completes.
    }
  }

  private clearTimers(): void {
    if (this.retryTimer) clearTimeout(this.retryTimer);
    if (this.pingTimer) clearInterval(this.pingTimer);
    if (this.handshakeTimer) clearTimeout(this.handshakeTimer);
    this.retryTimer = undefined;
    this.pingTimer = undefined;
    this.handshakeTimer = undefined;
    this.awaitingPong = false;
  }

  private connect(): void {
    if (!this.running) return;
    let socket: WebSocket;
    try {
      socket = this.socketFactory(this.url.href);
    } catch (error) {
      this.logger.warn(
        { error: error instanceof Error ? error.message : "unknown" },
        "Beacon WS connection failed",
      );
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;
    this.ready = false;
    this.handshakeTimer = setTimeout(() => {
      if (this.socket === socket && !this.ready) {
        this.socket = undefined;
        this.clearTimers();
        this.closeSocket(socket);
        this.scheduleReconnect();
      }
    }, 10_000);
    this.handshakeTimer.unref();

    socket.addEventListener("open", () => {
      if (!this.running || this.socket !== socket) {
        this.closeSocket(socket);
        return;
      }
      this.pingTimer = setInterval(() => {
        if (this.socket !== socket || socket.readyState !== WebSocket.OPEN)
          return;
        if (this.awaitingPong) {
          this.logger.warn("Beacon WS pong timeout");
          this.closeSocket(socket);
          return;
        }
        this.awaitingPong = true;
        socket.send(
          JSON.stringify({
            v: 1,
            type: "ping",
            id: `mcp-ping-${++this.pingId}`,
          }),
        );
      }, 30_000);
      this.pingTimer.unref();
    });

    socket.addEventListener("message", (message) => {
      if (
        !this.running ||
        this.socket !== socket ||
        typeof message.data !== "string"
      )
        return;
      // Cap the data we parse, not merely the objects we retain.
      if (message.data.length > 131_072) {
        this.logger.warn("Oversized Beacon WS message ignored");
        return;
      }
      let msg: WsEnvelope;
      try {
        msg = JSON.parse(message.data) as WsEnvelope;
      } catch {
        return;
      }
      if (!msg || typeof msg !== "object" || msg.v !== 1) return;
      if (
        msg.type === "hello" &&
        !this.ready &&
        socket.readyState === WebSocket.OPEN
      ) {
        try {
          socket.send(
            JSON.stringify({
              v: 1,
              type: "subscribe",
              id: "mcp-changes",
              scope: { events: ["nodeUpdate", "observerStatus"] },
            }),
          );
        } catch {
          this.closeSocket(socket);
        }
      } else if (msg.type === "subscribed" && msg.id === "mcp-changes") {
        if (this.handshakeTimer) clearTimeout(this.handshakeTimer);
        this.handshakeTimer = undefined;
        this.ready = true;
        this.attempt = 0;
        this.logger.info("Beacon WS subscribed to node and observer updates");
      } else if (msg.type === "pong" && msg.id === `mcp-ping-${this.pingId}`) {
        this.awaitingPong = false;
      } else if (msg.type === "lagged") {
        this.logger.warn(
          { droppedCount: msg.droppedCount },
          "Beacon WS dropped events; no replay available",
        );
      } else if (
        this.ready &&
        msg.type === "event" &&
        (msg.event === "nodeUpdate" || msg.event === "observerStatus")
      ) {
        this.onUpdate(msg.event, msg.data);
      }
    });

    socket.addEventListener("error", () => {
      if (this.socket === socket) this.closeSocket(socket);
    });
    socket.addEventListener("close", () => {
      if (this.socket !== socket) return;
      this.clearTimers();
      this.ready = false;
      this.socket = undefined;
      if (this.running) {
        this.logger.warn("Beacon WS closed; updates may have been missed");
        this.scheduleReconnect();
      }
    });
  }

  private scheduleReconnect(): void {
    if (!this.running || this.retryTimer) return;
    const base = Math.min(30_000, 1000 * 2 ** Math.min(this.attempt++, 5));
    const delay = Math.round(base * (0.75 + Math.random() * 0.25));
    this.retryTimer = setTimeout(() => {
      this.retryTimer = undefined;
      this.connect();
    }, delay);
    this.retryTimer.unref();
  }
}
