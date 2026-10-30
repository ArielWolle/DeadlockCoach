import type { RecordedFrame } from './types';
const DB = 'deadlock-coach-recordings-v1';
// Only full scans are recorded (one every ~3-4 s), so a 40-minute game is ~600-700 frames of
// ~250-320 KB. Keep about three games; the oldest frames go first.
export const MAX_FRAMES = 2400, MAX_BYTES = 750 * 1024 * 1024;
let opening: Promise<IDBDatabase> | undefined;
function open() {
  return opening ||= new Promise<IDBDatabase>((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore('frames', { keyPath: 'id', autoIncrement: true });
    req.onsuccess = () => resolve(req.result); req.onerror = () => { opening = undefined; reject(req.error); };
  });
}
function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error || new Error('Storage transaction aborted')); });
}
export async function listFrames(): Promise<RecordedFrame[]> {
  const db = await open();
  return new Promise((resolve, reject) => { const req = db.transaction('frames').objectStore('frames').getAll(); req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error); });
}
/** Sizes of the stored frames, oldest first (loaded once; the main thread serializes writes). */
let known: Array<{ id: number; size: number }> | null = null;
export async function recordFrame(frame: RecordedFrame) {
  if (frame.image.size > MAX_BYTES) throw new Error('Frame exceeds recording limit');
  known ??= (await listFrames()).map(f => ({ id: f.id!, size: f.image.size }));
  const db = await open(); const tx = db.transaction('frames', 'readwrite'); const req = tx.objectStore('frames').add(frame);
  await done(tx); known.push({ id: Number(req.result), size: frame.image.size });
  // Cap both count and total image bytes, removing the oldest first.
  let bytes = known.reduce((n, f) => n + f.size, 0); const drop: number[] = [];
  while (known.length > MAX_FRAMES || bytes > MAX_BYTES) { const f = known.shift()!; bytes -= f.size; drop.push(f.id); }
  if (drop.length) { const trim = db.transaction('frames', 'readwrite'); for (const id of drop) trim.objectStore('frames').delete(id); await done(trim); }
}
export async function deleteFrames(ids: number[]) {
  if (!ids.length) return; known = null;
  const db = await open(); const tx = db.transaction('frames', 'readwrite'); for (const id of ids) tx.objectStore('frames').delete(id); await done(tx);
}
export async function clearFrames() { known = null; const db = await open(); const tx = db.transaction('frames', 'readwrite'); tx.objectStore('frames').clear(); await done(tx); }
export function download(name: string, data: Blob) {
  const url = URL.createObjectURL(data), link = document.createElement('a'); link.href = url; link.download = name; link.click(); setTimeout(() => URL.revokeObjectURL(url), 30000);
}
export function snapshotLines(text: string) {
  return text.replace(/^\uFEFF/, '').split(/\r?\n/).filter(s => s.trim()).map((line, i) => {
    const v = JSON.parse(line);
    if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error(`Line ${i + 1} is not a snapshot object`);
    if (v.game_clock_secs != null && (!Number.isInteger(v.game_clock_secs) || v.game_clock_secs < 0 || v.game_clock_secs > 14400)) throw new Error(`Line ${i + 1} has an invalid game clock`);
    if (v.souls != null && (!Number.isInteger(v.souls) || v.souls < 0 || v.souls > 1000000)) throw new Error(`Line ${i + 1} has invalid souls`);
    return v;
  });
}
