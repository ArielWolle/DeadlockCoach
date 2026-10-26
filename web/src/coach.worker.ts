/// <reference lib="webworker" />
import init, { Coach } from '../wasm/coach_web';
import type { Manifest, WorkerReply } from './types';
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let coach: Coach | undefined;
const send = (r: WorkerReply) => ctx.postMessage(r);

async function initialize(base: string) {
  send({ type: 'progress', message: 'Loading local analysis engine…', fraction: 0 });
  await init();
  const mr = await fetch(`${base}assets/manifest.json`, { cache: 'no-cache' });
  if (!mr.ok) throw new Error(`Asset manifest: HTTP ${mr.status}`);
  const manifest: Manifest = await mr.json();
  if (manifest.schema !== 1) throw new Error('Unsupported asset manifest version');
  const names = ['models/text-detection.rten', 'models/text-rec.rten', 'templates.bin', 'heroes.json', 'timings.json', 'counters.json', 'rules.json'];
  let cache: Cache | undefined;
  try { cache = await caches.open(`deadlock-assets-${manifest.version}`); } catch { /* storage can be unavailable; network still works */ }
  const buffers: ArrayBuffer[] = [];
  let loaded = 0; const total = names.reduce((n, name) => n + manifest.files[name].bytes, 0);
  for (const name of names) {
    const url = `${base}assets/${name}`;
    const expected = manifest.files[name];
    let response = await cache?.match(url);
    if (!response) { response = await fetch(url); }
    if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
    const buffer = await response.arrayBuffer();
    const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', buffer))].map(b => b.toString(16).padStart(2, '0')).join('');
    if (hash !== expected.sha256 || buffer.byteLength !== expected.bytes) {
      await cache?.delete(url); throw new Error(`Asset integrity check failed for ${name}. Reload to retry.`);
    }
    try { await cache?.put(url, new Response(buffer.slice(0))); } catch { /* quota is not fatal */ }
    buffers.push(buffer); loaded += buffer.byteLength;
    send({ type: 'progress', message: `Loading ${name} (${Math.round(loaded / 1048576)} MB)…`, fraction: loaded / total * .9 });
  }
  const text = (i: number) => new TextDecoder().decode(buffers[i]);
  coach?.free();
  coach = new Coach(new Uint8Array(buffers[0]), new Uint8Array(buffers[1]), new Uint8Array(buffers[2]), text(3), text(4), text(5), text(6));
  try {
    for (const key of await caches.keys()) if (key.startsWith('deadlock-assets-') && key !== `deadlock-assets-${manifest.version}`) await caches.delete(key);
  } catch { /* no persistent cache */ }
  send({ type: 'ready', version: manifest.version });
}

// Single serialized mailbox. The main thread never sends a second frame until completion.
let tasks = Promise.resolve();
ctx.onmessage = event => {
  const m = event.data;
  tasks = tasks.then(async () => {
    try {
      if (m.type === 'init') { await initialize(m.base); return; }
      if (!coach) throw new Error('Analysis engine is not initialized');
      if (m.type === 'reset') { coach.reset(); return; }
      if (m.type === 'configure') { coach.configure(JSON.stringify(m.options)); return; }
      const start = performance.now();
      if (m.type === 'frame' || m.type === 'replay') {
        const analysis = m.type === 'frame'
          ? coach.analyze(new Uint8Array(m.rgba), m.width, m.height, new Uint8Array(m.map), m.mapWidth, m.mapHeight, m.timestamp)
          : coach.replay(JSON.stringify(m.snapshot));
        send({ type: 'result', id: m.id, generation: m.generation, analysis, elapsed: performance.now() - start });
      }
    } catch (e) { send({ type: 'error', message: e instanceof Error ? e.message : String(e), id: m.id }); }
  });
};
