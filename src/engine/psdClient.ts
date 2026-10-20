// Talks to the PSD worker.
import type { ReadResult } from './psd';

let worker: Worker | null = null;
let next = 1;
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();

function get(): Worker {
  if (!worker) {
    worker = new Worker(new URL('./psd.worker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = (e: MessageEvent<{ id: number; ok: boolean; result?: unknown; error?: string }>) => {
      const p = pending.get(e.data.id);
      pending.delete(e.data.id);
      if (!p) return;
      if (e.data.ok) p.resolve(e.data.result);
      else p.reject(new Error(e.data.error));
    };
  }
  return worker;
}

function call<T>(op: 'read' | 'write', data: ArrayBuffer): Promise<T> {
  const id = next++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    get().postMessage({ id, op, data }, [data]);
  });
}

/** Parses a PSD into the engine's manifest message (and what couldn't be carried over). */
export const readPsd = (bytes: ArrayBuffer) => call<ReadResult>('read', bytes);
/** Writes a PSD from the engine's export message. */
export const writePsd = (message: ArrayBuffer) => call<Uint8Array>('write', message);
