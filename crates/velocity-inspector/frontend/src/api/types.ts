export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Element {
  platform_id: string;
  label: string | null;
  text: string | null;
  element_type: string;
  bounds: Rect;
  enabled: boolean;
  visible: boolean;
  children: Element[];
}

export type Selector =
  | { Id: string }
  | { Text: string }
  | { TextContains: string }
  | { AccessibilityId: string }
  | { ClassName: string }
  | { Index: { selector: Selector; index: number } }
  | { Compound: Selector[] };

export type Direction = "up" | "down" | "left" | "right";

export type Platform = "ios" | "android";

export type DeviceState = "booted" | "shutdown" | "unknown";

export type DeviceType = "physical" | "simulator" | "emulator" | "unknown";

export interface DeviceInfo {
  id: string;
  name: string;
  platform: Platform;
  state: DeviceState;
  os_version: string | null;
  device_type: DeviceType;
}

export interface GenerateResponse {
  selector: Selector;
  yaml_tap: string;
  yaml_assert: string;
}

export interface WsMessage {
  type: "screenshot" | "hierarchy" | "error" | "performance";
  url?: string;
  root?: Element;
  message?: string;
  // Performance fields (only when type === "performance")
  javaHeapKb?: number;
  nativeHeapKb?: number;
  totalPssKb?: number;
  cpuPercent?: number;
}

/** A single performance sample from the WebSocket stream. */
export interface PerfSample {
  timestamp: number;
  javaHeapKb: number;
  nativeHeapKb: number;
  totalPssKb: number;
  cpuPercent: number;
}

/** A recorded test step with its YAML representation. */
export interface RecordedStep {
  /** Action type for display (tap, inputText, swipe, assertVisible) */
  action: string;
  /** Human-readable label (e.g., element label or swipe direction) */
  label: string;
  /** YAML snippet for this step */
  yaml: string;
  /** Timestamp when recorded */
  timestamp: number;
}

/** Request body for saving a recorded flow. */
export interface SaveFlowRequest {
  name: string;
  app_id: string;
  steps: string[];
  path?: string;
}

export interface SaveFlowResponse {
  path: string;
  yaml: string;
}
