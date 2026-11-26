import { create } from "zustand";

export interface BandwidthPoint {
  ts: string;
  upBps: number;
  downBps: number;
  activeConns: number;
}

export interface LiveNodeMetric {
  id: string;
  name: string;
  status: string;
  cpu_pct: number;
  mem_pct: number;
  active_connections: number;
  active_users: number;
  uptime_secs: number;
}

const MAX_RING_POINTS = 60;

interface LiveMetricsState {
  connected: boolean;
  cadenceMode: "eco" | "realtime";
  nodes: Record<string, LiveNodeMetric>;
  channels: Record<string, BandwidthPoint[]>;
  latest: Record<string, BandwidthPoint>;
  setConnected: (connected: boolean) => void;
  applyLiveNodes: (cadenceMode: "eco" | "realtime", nodeList: LiveNodeMetric[]) => void;
  pushTick: (channel: string, point: BandwidthPoint) => void;
  clearAll: () => void;
}

export const useLiveMetricsStore = create<LiveMetricsState>((set) => ({
  connected: false,
  cadenceMode: "eco",
  nodes: {},
  channels: {},
  latest: {},
  setConnected: (connected) => {
    set({ connected });
  },
  applyLiveNodes: (cadenceMode, nodeList) => {
    const map: Record<string, LiveNodeMetric> = {};
    for (const n of nodeList) {
      map[n.id] = n;
    }
    set({ cadenceMode, nodes: map });
  },
  pushTick: (channel, point) => {
    set((state) => {
      const existing = state.channels[channel] ?? [];
      const next = [...existing, point];
      if (next.length > MAX_RING_POINTS) {
        next.splice(0, next.length - MAX_RING_POINTS);
      }
      return {
        channels: { ...state.channels, [channel]: next },
        latest: { ...state.latest, [channel]: point },
      };
    });
  },
  clearAll: () => {
    set({ connected: false, nodes: {}, channels: {}, latest: {} });
  },
}));
