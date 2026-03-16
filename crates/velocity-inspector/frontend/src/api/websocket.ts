import type { WsMessage } from "./types";

export type WsListener = (msg: WsMessage) => void;

export class InspectorSocket {
  private ws: WebSocket | null = null;
  private listeners: WsListener[] = [];
  private deviceId: string;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(deviceId: string) {
    this.deviceId = deviceId;
  }

  connect() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${location.host}/api/ws/${this.deviceId}`;

    this.ws = new WebSocket(url);

    this.ws.onmessage = (event) => {
      try {
        const msg: WsMessage = JSON.parse(event.data);
        this.listeners.forEach((fn) => fn(msg));
      } catch {
        // Ignore malformed messages
      }
    };

    this.ws.onclose = () => {
      this.reconnectTimer = setTimeout(() => this.connect(), 2000);
    };

    this.ws.onerror = () => {
      this.ws?.close();
    };
  }

  onMessage(fn: WsListener) {
    this.listeners.push(fn);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== fn);
    };
  }

  sendRefresh() {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "refresh" }));
    }
  }

  disconnect() {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.ws?.close();
    this.ws = null;
    this.listeners = [];
  }
}
