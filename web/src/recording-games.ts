// Splits the local recording shelf into games. Frames recorded by this version carry `game` (a new
// id at every match end the end-of-game review detects: PostGame screen, new match, long gap,
// Stop). Older untagged frames are split here by match id, a clock restart or a long pause.
import type { RecordedFrame } from './types';

export interface RecordedGame {
  key: string; frames: RecordedFrame[]; start: number; end: number;
  hero: string | null; result: 'win' | 'loss' | null; startClock: number | null; endClock: number | null;
  /** Frames with a game clock (gameplay, death and shop screens); 0 = only menus between games. */
  played: number; bytes: number;
}

/** A pause this long between two recorded frames starts a new (untagged) game. */
export const UNTAGGED_GAP_MS = 10 * 60 * 1000;

export function newGameId(now = Date.now()) { return `g${now.toString(36)}${Math.random().toString(36).slice(2, 6)}`; }

const clockOf = (f: RecordedFrame) => typeof f.snapshot?.game_clock_secs === 'number' ? f.snapshot.game_clock_secs : null;

/** Games, oldest first. Each frame lands in exactly one game. */
export function groupGames(frames: RecordedFrame[]): RecordedGame[] {
  const sorted = [...frames].sort((a, b) => (a.created - b.created) || ((a.id ?? 0) - (b.id ?? 0)));
  const groups: RecordedFrame[][] = []; const byTag = new Map<string, RecordedFrame[]>();
  let untagged: RecordedFrame[] | null = null; let lastMatch: number | null = null, lastClock: number | null = null, lastAt = 0;
  for (const f of sorted) {
    if (f.game) {
      let g = byTag.get(f.game); if (!g) { g = []; byTag.set(f.game, g); groups.push(g); }
      g.push(f); untagged = null; continue;
    }
    const match = typeof f.snapshot?.match_id === 'number' ? f.snapshot.match_id : null, clock = clockOf(f);
    const split = !untagged || f.created - lastAt > UNTAGGED_GAP_MS
      || (match != null && lastMatch != null && match !== lastMatch)
      || (clock != null && lastClock != null && clock + 120 < lastClock && clock < 180);
    if (split) { untagged = []; groups.push(untagged); lastMatch = null; lastClock = null; }
    untagged!.push(f); lastAt = f.created;
    if (match != null) lastMatch = match; if (clock != null) lastClock = clock;
  }
  return groups.map((g, i) => summarize(g, g[0].game ?? `untagged-${g[0].id ?? i}`));
}

function summarize(frames: RecordedFrame[], key: string): RecordedGame {
  const heroes = new Map<string, number>(); let result: RecordedGame['result'] = null, startClock: number | null = null, endClock: number | null = null, played = 0, bytes = 0;
  for (const f of frames) {
    const s = f.snapshot ?? {}; bytes += f.image?.size ?? 0;
    if (typeof s.hero === 'string' && s.hero) heroes.set(s.hero, (heroes.get(s.hero) ?? 0) + 1);
    if (s.match_result === 'win' || s.match_result === 'loss') result = s.match_result;
    const c = clockOf(f); if (c != null) { played++; startClock ??= c; endClock = c; }
  }
  const hero = [...heroes].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
  return { key, frames, start: frames[0].created, end: frames[frames.length - 1].created, hero, result, startClock, endClock, played, bytes };
}

const pad = (n: number) => String(n).padStart(2, '0');
export const clockText = (s: number | null) => s == null ? '?' : `${Math.floor(s / 60)}:${pad(Math.floor(s % 60))}`;

/** `deadlock-recording-2026-10-30-2115-grey-talon-win.json` (local start time, hero, result). */
export function gameFileName(g: Pick<RecordedGame, 'start' | 'hero' | 'result'>, heroName?: (id: string) => string) {
  const d = new Date(g.start);
  const hero = g.hero ? (heroName?.(g.hero) || g.hero.replace(/^hero_/, '')).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') : '';
  return ['deadlock-recording', `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`, hero, g.result ?? ''].filter(Boolean).join('-') + '.json';
}

/** Spoken calls / AI exchanges belonging to a game: wall time from its first frame to a minute after its last. */
export function inWindow<T extends { t: number }>(items: T[], g: Pick<RecordedGame, 'start' | 'end'>, before = 5000, after = 60000) {
  return items.filter(x => x.t >= g.start - before && x.t <= g.end + after);
}

/** Worth downloading by itself: some gameplay was recorded (not only menus or a minute of play). */
export const downloadable = (g: RecordedGame) => g.played >= 10;
