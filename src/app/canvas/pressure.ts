// Pen pressure from the native side. WebKit on macOS reports tablet pens as a plain mouse without
// pressure, so the app forwards the pressure of tablet mouse events (NSEvent) as `pen-pressure`
// events. Samples older than 60 ms are ignored, so a mouse never picks up a stale value.
import { listen } from '@tauri-apps/api/event';

let last: { pressure: number; at: number } | null = null;

export function installPressureBridge() {
  void listen<number>('pen-pressure', (e) => {
    last = { pressure: e.payload, at: performance.now() };
  }).catch(() => {});
}

export function nativePressure(): number | null {
  if (!last || performance.now() - last.at > 60) return null;
  return Math.max(0.0001, Math.min(1, last.pressure));
}

/** For tests: inject a native pressure sample. */
export function simulateNativePressure(pressure: number) {
  last = { pressure, at: performance.now() };
}
