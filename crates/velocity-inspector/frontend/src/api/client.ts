import type { DeviceInfo, Direction, Element, GenerateResponse, Selector } from "./types";

const BASE = "/api";

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
  }
  return res.json();
}

export async function listDevices(): Promise<DeviceInfo[]> {
  return json(`${BASE}/devices`);
}

export async function selectDevice(deviceId: string): Promise<DeviceInfo> {
  return json(`${BASE}/devices/select`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ device_id: deviceId }),
  });
}

export function screenshotUrl(deviceId: string): string {
  return `${BASE}/devices/${deviceId}/screenshot?t=${Date.now()}`;
}

export async function getHierarchy(deviceId: string): Promise<Element> {
  return json(`${BASE}/devices/${deviceId}/hierarchy`);
}

export async function tap(deviceId: string, opts: { selector?: Selector; coordinates?: [number, number] }): Promise<void> {
  await json(`${BASE}/devices/${deviceId}/tap`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(opts),
  });
}

export async function typeText(deviceId: string, selector: Selector, text: string): Promise<void> {
  await json(`${BASE}/devices/${deviceId}/type`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ selector, text }),
  });
}

export async function swipe(deviceId: string, direction: Direction): Promise<void> {
  await json(`${BASE}/devices/${deviceId}/swipe`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ direction }),
  });
}

export async function generateSelector(element: Element): Promise<GenerateResponse> {
  return json(`${BASE}/selector/generate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ element }),
  });
}

export async function saveFlow(
  name: string,
  appId: string,
  steps: string[],
  path?: string
): Promise<{ path: string; yaml: string }> {
  return json(`${BASE}/flow/save`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, app_id: appId, steps, path }),
  });
}
