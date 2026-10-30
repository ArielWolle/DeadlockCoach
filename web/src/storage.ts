import type { RecordedFrame } from './types';
const DB = 'deadlock-coach-recordings-v1';
// Only full scans are recorded (one every ~3-4 s), so a 40-minute game is ~600-700 frames of
// ~250-320 KB. Keep about three games; the oldest frames of earlier games go first, and the game
// being recorded is never trimmed (a long game may go over the cap until the next one starts).
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
let known: Array<{ id: number; size: number; game?: string }> | null = null;
let persistAsked = false;
/** Which stored frames to delete (oldest first) so the rest fits `fits(count, bytes)`. Frames of
 *  `game` (the one being recorded) are never chosen, so the current game is kept whole. */
export function planTrim(frames: Array<{ id: number; size: number; game?: string }>, game: string | undefined, fits: (count: number, bytes: number) => boolean): number[] {
  let count = frames.length, bytes = frames.reduce((n, f) => n + f.size, 0); const drop: number[] = [];
  for (const f of frames) {
    if (game != null && f.game === game) continue;
    if (fits(count, bytes)) break;
    drop.push(f.id); count--; bytes -= f.size;
  }
  return drop;
}
async function trimOthers(game: string | undefined, fits: (count: number, bytes: number) => boolean) {
  if (!known) return 0;
  const drop = planTrim(known, game, fits); if (!drop.length) return 0;
  const gone = new Set(drop); known = known.filter(f => !gone.has(f.id));
  const db = await open(); const trim = db.transaction('frames', 'readwrite'); for (const id of drop) trim.objectStore('frames').delete(id); await done(trim);
  return drop.length;
}
/** Store one frame. The whole current game is always kept: when the shelf is over its cap (or the
 *  browser's storage quota) the oldest frames of earlier games go first. Throws only when even the
 *  current game alone no longer fits in the browser's storage. */
export async function recordFrame(frame: RecordedFrame) {
  if (frame.image.size > MAX_BYTES) throw new Error('Frame exceeds recording limit');
  // Ask once that the browser not evict recordings under storage pressure (silently ignored if refused).
  if (!persistAsked) { persistAsked = true; void navigator.storage?.persist?.().catch(() => false); }
  known ??= (await listFrames()).map(f => ({ id: f.id!, size: f.image.size, game: f.game }));
  const add = async () => { const db = await open(); const tx = db.transaction('frames', 'readwrite'); const req = tx.objectStore('frames').add(frame); await done(tx); return Number(req.result); };
  let id: number;
  try { id = await add(); }
  catch (e) {
    // Quota: make room from earlier games (a quarter of them, at least 50 frames), then retry once.
    const others = known.filter(f => f.game !== frame.game).length;
    if (!others || (e as DOMException)?.name !== 'QuotaExceededError') throw e;
    const target = known.length - Math.max(50, Math.ceil(others / 4));
    await trimOthers(frame.game, count => count <= target);
    id = await add();
  }
  known.push({ id, size: frame.image.size, game: frame.game });
  await trimOthers(frame.game, (count, bytes) => count <= MAX_FRAMES && bytes <= MAX_BYTES);
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
