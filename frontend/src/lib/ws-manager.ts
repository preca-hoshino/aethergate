import { useEffect } from "react";
import { useLiveMetricsStore, type LiveNodeMetric } from "@/stores/use-live-metrics-store";

export type NodeStateListener = (event: {
  node_id: string;
  desired_state: string;
  runtime_state: string;
  agent_online: boolean;
}) => void;

type WsServerFrame =
  | {
      type: "snapshot";
      cadence_mode: "eco" | "realtime";
      nodes: LiveNodeMetric[];
    }
  | {
      type: "live_tick";
      cadence_mode: "eco" | "realtime";
      nodes: LiveNodeMetric[];
    }
  | {
      type: "bandwidth_tick";
      ts: string;
      channel: string;
      up_bps: number;
      down_bps: number;
      active_conns: number;
    }
  | {
      type: "node_state_changed";
      node_id: string;
      desired_state: string;
      runtime_state: string;
      agent_online: boolean;
    };

class RealtimeWsManager {
  private ws: WebSocket | null = null;
  private refCounts = new Map<string, number>();
  private pausedBandwidth = new Set<string>();
  private nodeListeners = new Set<NodeStateListener>();
  private reconnectDelayMs = 1000;
  private reconnectTimer: number | null = null;
  private hiddenTimer: number | null = null;
  private started = false;

  start(): void {
    if (this.started) return;
    this.started = true;
    document.addEventListener("visibilitychange", this.handleVisibilityChange);
    this.connect();
  }

  stop(): void {
    this.started = false;
    document.removeEventListener("visibilitychange", this.handleVisibilityChange);
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.hiddenTimer !== null) {
      window.clearTimeout(this.hiddenTimer);
      this.hiddenTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  subscribe(channels: string[]): () => void {
    this.start();
    const added: string[] = [];
    for (const ch of channels) {
      const prev = this.refCounts.get(ch) ?? 0;
      this.refCounts.set(ch, prev + 1);
      if (prev === 0 && !this.pausedBandwidth.has(ch)) {
        added.push(ch);
      }
    }
    if (added.length > 0) {
      this.sendOp("subscribe", added);
    }

    return () => {
      const removed: string[] = [];
      for (const ch of channels) {
        const prev = this.refCounts.get(ch) ?? 0;
        if (prev <= 1) {
          this.refCounts.delete(ch);
          this.pausedBandwidth.delete(ch);
          removed.push(ch);
        } else {
          this.refCounts.set(ch, prev - 1);
        }
      }
      if (removed.length > 0) {
        this.sendOp("unsubscribe", removed);
      }
    };
  }

  onNodeState(listener: NodeStateListener): () => void {
    this.nodeListeners.add(listener);
    return () => {
      this.nodeListeners.delete(listener);
    };
  }

  private handleVisibilityChange = (): void => {
    if (document.visibilityState === "hidden") {
      if (this.hiddenTimer !== null) window.clearTimeout(this.hiddenTimer);
      this.hiddenTimer = window.setTimeout(() => {
        const toPause: string[] = [];
        for (const ch of this.refCounts.keys()) {
          if (ch !== "node_status" && !this.pausedBandwidth.has(ch)) {
            this.pausedBandwidth.add(ch);
            toPause.push(ch);
          }
        }
        if (toPause.length > 0) {
          this.sendOp("unsubscribe", toPause);
        }
      }, 30_000);
    } else {
      if (this.hiddenTimer !== null) {
        window.clearTimeout(this.hiddenTimer);
        this.hiddenTimer = null;
      }
      if (this.pausedBandwidth.size > 0) {
        const toResume = [...this.pausedBandwidth];
        this.pausedBandwidth.clear();
        this.sendOp("subscribe", toResume);
      }
    }
  };

  private connect(): void {
    if (!this.started) return;
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/api/v1/ws/realtime`;
    const socket = new WebSocket(url);
    this.ws = socket;

    socket.onopen = () => {
      this.reconnectDelayMs = 1000;
      useLiveMetricsStore.getState().setConnected(true);
      const active = [...this.refCounts.keys()].filter((ch) => !this.pausedBandwidth.has(ch));
      if (active.length > 0) {
        this.sendOp("subscribe", active);
      }
    };

    socket.onmessage = (ev) => {
      try {
        const msg = JSON.parse(String(ev.data)) as WsServerFrame;
        if (msg.type === "snapshot" || msg.type === "live_tick") {
          useLiveMetricsStore.getState().applyLiveNodes(msg.cadence_mode, msg.nodes);
        } else if (msg.type === "bandwidth_tick") {
          useLiveMetricsStore.getState().pushTick(msg.channel, {
            ts: msg.ts,
            upBps: msg.up_bps,
            downBps: msg.down_bps,
            activeConns: msg.active_conns,
          });
        } else {
          for (const listener of this.nodeListeners) {
            listener(msg);
          }
        }
      } catch {
        // Ignore malformed frame
      }
    };

    socket.onclose = () => {
      if (!this.started) return;
      this.ws = null;
      const delay = this.reconnectDelayMs;
      this.reconnectDelayMs = Math.min(this.reconnectDelayMs * 2, 15_000);
      this.reconnectTimer = window.setTimeout(() => {
        this.connect();
      }, delay);
    };
  }

  private sendOp(op: "subscribe" | "unsubscribe", channels: string[]): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ op, channels }));
    }
  }
}

export const wsManager = new RealtimeWsManager();

export function useRealtimeChannels(channels: string[]): void {
  const key = channels.join(",");
  useEffect(() => {
    if (!key) return;
    const list = key.split(",").filter(Boolean);
    return wsManager.subscribe(list);
  }, [key]);
}
