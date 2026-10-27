/// <reference lib="webworker" />
/**
 * Parallel line recognition for the (single-threaded) WASM analysis worker.
 *
 * The Rust scanner calls `globalThis.coachRecognizeLines(bytes, dims)` synchronously with every
 * line crop of a frame. This splits the crops across pool workers by estimated cost (the
 * recognizer normalizes height, so cost ~ width/height), waits on shared memory, and returns
 * the lines in the original order. Any failure returns `null`, and Rust recognizes locally.
 * Requires cross-origin isolation (SharedArrayBuffer + Atomics.wait in a worker).
 */
interface Member { worker: Worker; ctrl: SharedArrayBuffer; out: SharedArrayBuffer; ready: boolean }
const RESULT_BYTES = 256 * 1024;

export class LinePool {
  private members: Member[] = [];
  private seq = 0;
  private broken = false;
  stats = { batches: 0, lines: 0, ms: 0, fallbacks: 0 };

  static supported() {
    return typeof SharedArrayBuffer !== 'undefined' && (self as unknown as { crossOriginIsolated?: boolean }).crossOriginIsolated === true;
  }
  static size() { const cores = navigator.hardwareConcurrency || 4; return Math.max(0, Math.min(4, Math.floor(cores / 2) - 1)); }

  /** Start `n` members with the recognition model. Resolves with the number that became ready. */
  start(model: ArrayBuffer, n: number): Promise<number> {
    const starting = Array.from({ length: n }, () => new Promise<Member | undefined>(resolve => {
      const worker = new Worker(new URL('./lines.worker.ts', import.meta.url), { type: 'module' });
      const member: Member = { worker, ctrl: new SharedArrayBuffer(8), out: new SharedArrayBuffer(RESULT_BYTES), ready: false };
      const timer = setTimeout(() => { worker.terminate(); resolve(undefined); }, 60000);
      worker.onmessage = e => {
        clearTimeout(timer);
        if (e.data.type === 'ready') { member.ready = true; resolve(member); } else { worker.terminate(); resolve(undefined); }
      };
      worker.onerror = () => { clearTimeout(timer); worker.terminate(); resolve(undefined); };
      worker.postMessage({ type: 'init', model: model.slice(0) });
    }));
    return Promise.all(starting).then(list => { this.members = list.filter((m): m is Member => !!m); return this.members.length; });
  }

  get size() { return this.broken ? 0 : this.members.length; }

  /** Synchronous: called from WASM. Returns string[] in input order, or null. */
  recognize(bytes: Uint8Array, dims: Uint32Array): string[] | null {
    if (this.broken || this.members.length < 1) return null;
    const started = performance.now();
    const count = dims.length / 2;
    const offsets: number[] = []; let at = 0;
    for (let i = 0; i < count; i++) { offsets.push(at); at += dims[2 * i] * dims[2 * i + 1] * 4; }
    // Greedy longest-processing-time assignment by width/height cost.
    const order = [...Array(count).keys()].sort((a, b) => dims[2 * b] / dims[2 * b + 1] - dims[2 * a] / dims[2 * a + 1]);
    const lanes = this.members.map(() => ({ items: [] as number[], cost: 0 }));
    for (const i of order) { const lane = lanes.reduce((m, l) => l.cost < m.cost ? l : m); lane.items.push(i); lane.cost += dims[2 * i] / Math.max(1, dims[2 * i + 1]); }
    const seq = ++this.seq;
    const jobs = lanes.map((lane, k) => ({ lane, member: this.members[k] })).filter(j => j.lane.items.length);
    for (const { lane, member } of jobs) {
      let size = 0; for (const i of lane.items) size += dims[2 * i] * dims[2 * i + 1] * 4;
      const part = new Uint8Array(size); const partDims = new Uint32Array(lane.items.length * 2); let p = 0;
      lane.items.forEach((i, j) => { const n = dims[2 * i] * dims[2 * i + 1] * 4; part.set(bytes.subarray(offsets[i], offsets[i] + n), p); p += n; partDims[2 * j] = dims[2 * i]; partDims[2 * j + 1] = dims[2 * i + 1]; });
      Atomics.store(new Int32Array(member.ctrl), 0, 0);
      member.worker.postMessage({ type: 'lines', seq, bytes: part.buffer, dims: partDims.buffer, ctrl: member.ctrl, out: member.out }, [part.buffer, partDims.buffer]);
    }
    const result: string[] = new Array(count);
    for (const { lane, member } of jobs) {
      const ctrl = new Int32Array(member.ctrl);
      // Bounded wait: a stuck member disables the pool and Rust falls back to local recognition.
      if (Atomics.wait(ctrl, 0, 0, 5000) === 'timed-out' && Atomics.load(ctrl, 0) !== seq) { this.broken = true; this.stats.fallbacks++; return null; }
      const length = Atomics.load(ctrl, 1);
      const lines = JSON.parse(new TextDecoder().decode(new Uint8Array(member.out).slice(0, length))) as string[] | null;
      if (!Array.isArray(lines) || lines.length !== lane.items.length) { this.stats.fallbacks++; return null; }
      lane.items.forEach((i, j) => { result[i] = lines[j]; });
    }
    this.stats.batches++; this.stats.lines += count; this.stats.ms += performance.now() - started;
    return result;
  }

  stop() { for (const m of this.members) m.worker.terminate(); this.members = []; }
}
