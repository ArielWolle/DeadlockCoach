/// <reference lib="webworker" />
import init, { Coach } from '../wasm/coach_web';
import type { Manifest, WorkerReply, ProfileReply } from './types';
import type { GameProfile } from './profiles';
import { loadVerifiedAsset } from './assets';
import { LinePool } from './line-pool';
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let coach: Coach | undefined;
// Parallel line recognition (cross-origin isolated sessions only). Started once per worker;
// the synchronous bridge below is what the Rust scanner calls for each frame's line batch.
let pool: LinePool | undefined, poolState = 'off';
(globalThis as unknown as { coachRecognizeLines: (b: Uint8Array, d: Uint32Array) => string[] | null }).coachRecognizeLines = (b, d) => pool?.recognize(b, d) ?? null;
function startPool(model: ArrayBuffer) {
  if (pool || !LinePool.supported()) { if (!LinePool.supported()) poolState = 'unavailable · not cross-origin isolated'; return; }
  const n = LinePool.size();
  if (n < 1) { poolState = 'off · too few CPU cores'; return; }
  pool = new LinePool(); poolState = `starting ${n}`;
  void pool.start(model, n).then(ready => {
    poolState = ready ? `${ready} parallel recognizers` : 'failed to start';
    coach?.set_parallel_lines(ready > 0);
  });
}
const send = (r: WorkerReply | ProfileReply) => ctx.postMessage(r);

async function initialize(base: string, profile: GameProfile) {
  send({ type: 'progress', message: 'Loading local analysis engine…', fraction: 0 });
  await init();
  const mr = await fetch(`${base}assets/manifest.json`, { cache: 'no-cache' });
  if (!mr.ok) throw new Error(`Asset manifest: HTTP ${mr.status}`);
  const manifest: Manifest = await mr.json();
  if (manifest.schema !== 1) throw new Error('Unsupported asset manifest version');
  const legacy = profile.adapter === 'deadlock' || profile.nodes.some(n => n.op.type === 'custom' && n.op.name === 'deadlock.scan'
    || n.op.type === 'classify' && String(n.op.library).startsWith('deadlock.'));
  const ocr = legacy || profile.nodes.some(n => n.op.type === 'ocr');
  const names = [...(ocr ? ['models/text-detection.rten', 'models/text-rec.rten'] : []),
    ...(legacy ? ['templates.bin', 'heroes.json', 'timings.json', 'counters.json', 'rules.json'] : [])];
  let cache: Cache | undefined;
  try { cache = await caches.open(`deadlock-assets-${manifest.version}`); } catch { /* storage can be unavailable; network still works */ }
  const buffers: Record<string, ArrayBuffer> = {};
  let loaded = 0; const total = names.reduce((n, name) => n + manifest.files[name].bytes, 0);
  for (const name of names) {
    const expected = manifest.files[name];
    const buffer = await loadVerifiedAsset(base, name, expected, cache);
    buffers[name] = buffer; loaded += buffer.byteLength;
    send({ type: 'progress', message: `Loading ${name} (${Math.round(loaded / 1048576)} MB)…`, fraction: loaded / total * .9 });
  }
  const bytes = (name: string) => new Uint8Array(buffers[name] || new ArrayBuffer(0));
  const text = (name: string, fallback = '') => buffers[name] ? new TextDecoder().decode(buffers[name]) : fallback;
  // Build the candidate before freeing the previous working adapter/profile.
  const candidate = Coach.create(JSON.stringify(profile), bytes('models/text-detection.rten'), bytes('models/text-rec.rten'), bytes('templates.bin'),
    text('heroes.json', '[]'), text('timings.json', '[]'), text('counters.json', '[]'), text('rules.json', '{}'));
  coach?.free(); coach = candidate;
  if (buffers['models/text-rec.rten']) startPool(buffers['models/text-rec.rten']);
  coach.set_parallel_lines((pool?.size ?? 0) > 0);
  try {
    for (const key of await caches.keys()) if (key.startsWith('deadlock-assets-') && key !== `deadlock-assets-${manifest.version}`) await caches.delete(key);
  } catch { /* no persistent cache */ }
  return manifest.version;
}

// Single serialized mailbox. The main thread never sends a second frame until completion.
let tasks = Promise.resolve();
ctx.onmessage = event => {
  const m = event.data;
  tasks = tasks.then(async () => {
    try {
      if (m.type === 'init') { const version = await initialize(m.base, m.profile); send({ type: 'ready', version }); return; }
      if (m.type === 'profile') {
        try { await initialize(m.base, m.profile); send({ type: 'profile_ready', requestId: m.requestId }); }
        catch (e) { send({ type: 'profile_error', requestId: m.requestId, message: e instanceof Error ? e.message : String(e) }); }
        return;
      }
      if (!coach) throw new Error('Analysis engine is not initialized');
      if (m.type === 'reset') { coach.reset(); return; }
      if (m.type === 'configure') { coach.configure(JSON.stringify(m.options)); return; }
      const start = performance.now();
      if (m.type === 'frame' || m.type === 'replay') {
        const analysis = m.type === 'frame'
          ? coach.analyze(new Uint8Array(m.rgba), m.width, m.height, new Uint8Array(m.map), m.mapWidth, m.mapHeight, m.timestamp)
          : coach.replay(JSON.stringify(m.snapshot));
        const s = pool?.stats;
        send({ type: 'result', id: m.id, generation: m.generation, analysis, elapsed: performance.now() - start,
          pool: { state: poolState, active: (pool?.size ?? 0) > 0,
            avg_ms: s && s.batches ? Math.round(s.ms / s.batches) : null, fallbacks: s?.fallbacks ?? 0 } });
      }
    } catch (e) { send({ type: 'error', message: e instanceof Error ? e.message : String(e), id: m.id }); }
  });
};
