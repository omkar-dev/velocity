import { create } from "zustand";
import type { DeviceInfo, Element, GenerateResponse, PerfSample, RecordedStep } from "../api/types";

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

  // Recording
  isRecording: boolean;
  recordedSteps: RecordedStep[];
  startRecording: () => void;
  stopRecording: () => void;
  addRecordedStep: (step: RecordedStep) => void;
  removeRecordedStep: (index: number) => void;
  clearRecordedSteps: () => void;

  // Performance
  perfHistory: PerfSample[];
  addPerfSample: (sample: PerfSample) => void;
  clearPerfHistory: () => void;

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

  // Recording
  isRecording: false,
  recordedSteps: [],
  startRecording: () => set({ isRecording: true, recordedSteps: [] }),
  stopRecording: () => set({ isRecording: false }),
  addRecordedStep: (step) =>
    set((state) => ({ recordedSteps: [...state.recordedSteps, step] })),
  removeRecordedStep: (index) =>
    set((state) => ({
      recordedSteps: state.recordedSteps.filter((_, i) => i !== index),
    })),
  clearRecordedSteps: () => set({ recordedSteps: [] }),

  // Performance (rolling 60-sample window)
  perfHistory: [],
  addPerfSample: (sample) =>
    set((state) => ({
      perfHistory: [...state.perfHistory, sample].slice(-60),
    })),
  clearPerfHistory: () => set({ perfHistory: [] }),

  loading: false,
  setLoading: (loading) => set({ loading }),

  error: null,
  setError: (error) => set({ error }),
}));
