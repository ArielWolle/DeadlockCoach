/// <reference lib="webworker" />
// One member of the parallel line-recognition pool. It owns a recognition-only model and
// answers batches through shared memory, so the (blocked) analysis worker can wait on them.
import init, { LineRecognizer } from '../wasm/coach_web';
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let recognizer: LineRecognizer | undefined;
ctx.onmessage = async event => {
  const m = event.data;
  if (m.type === 'init') {
    try { await init(); recognizer = new LineRecognizer(new Uint8Array(m.model)); ctx.postMessage({ type: 'ready' }); }
    catch (e) { ctx.postMessage({ type: 'failed', message: String(e) }); }
    return;
  }
  if (m.type !== 'lines') return;
  const ctrl = new Int32Array(m.ctrl), out = new Uint8Array(m.out);
  let encoded: Uint8Array;
  try { encoded = new TextEncoder().encode(JSON.stringify(recognizer ? recognizer.read(new Uint8Array(m.bytes), new Uint32Array(m.dims)) : null)); }
  catch { encoded = new TextEncoder().encode('null'); }
  if (encoded.length > out.length) encoded = new TextEncoder().encode('null');
  out.set(encoded); Atomics.store(ctrl, 1, encoded.length);
  Atomics.store(ctrl, 0, m.seq); Atomics.notify(ctrl, 0);
};
