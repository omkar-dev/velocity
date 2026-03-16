import { create } from "zustand";
import type { DeviceInfo, Element, GenerateResponse } from "../api/types";

interface InspectorState {
  // Device
  devices: DeviceInfo[];
  currentDeviceId: string | null;
  setDevices: (devices: DeviceInfo[]) => void;
  setCurrentDeviceId: (id: string | null) => void;

  // Screenshot
  screenshotUrl: string | null;
  setScreenshotUrl: (url: string | null) => void;

  // Hierarchy
  hierarchy: Element | null;
  setHierarchy: (hierarchy: Element | null) => void;

  // Selection
  selectedElement: Element | null;
  setSelectedElement: (element: Element | null) => void;

  // Selector generation
  generated: GenerateResponse | null;
  setGenerated: (gen: GenerateResponse | null) => void;

  // Loading / errors
  loading: boolean;
  setLoading: (loading: boolean) => void;
  error: string | null;
  setError: (error: string | null) => void;
}

export const useInspectorStore = create<InspectorState>((set) => ({
  devices: [],
  currentDeviceId: null,
  setDevices: (devices) => set({ devices }),
  setCurrentDeviceId: (id) => set({ currentDeviceId: id, selectedElement: null, generated: null }),

  screenshotUrl: null,
  setScreenshotUrl: (url) => set({ screenshotUrl: url }),

  hierarchy: null,
  setHierarchy: (hierarchy) => set({ hierarchy }),

  selectedElement: null,
  setSelectedElement: (element) => set({ selectedElement: element }),

  generated: null,
  setGenerated: (gen) => set({ generated: gen }),

  loading: false,
  setLoading: (loading) => set({ loading }),

  error: null,
  setError: (error) => set({ error }),
}));
