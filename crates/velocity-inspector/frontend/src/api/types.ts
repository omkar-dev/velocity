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
  type: "screenshot" | "hierarchy" | "error";
  url?: string;
  root?: Element;
  message?: string;
}
