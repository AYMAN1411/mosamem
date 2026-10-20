// Reads and writes PSD files off the main thread (ag-psd can take seconds on large files).
import { readPsdFile, writePsdFile } from './psd';

self.onmessage = (e: MessageEvent<{ id: number; op: 'read' | 'write'; data: ArrayBuffer }>) => {
  const { id, op, data } = e.data;
  try {
    if (op === 'read') {
      const r = readPsdFile(data);
      (self as unknown as Worker).postMessage({ id, ok: true, result: r }, [r.message.buffer]);
    } else {
      const out = writePsdFile(new Uint8Array(data));
      (self as unknown as Worker).postMessage({ id, ok: true, result: out }, [out.buffer]);
    }
  } catch (err) {
    (self as unknown as Worker).postMessage({ id, ok: false, error: err instanceof Error ? err.message : String(err) });
  }
};
