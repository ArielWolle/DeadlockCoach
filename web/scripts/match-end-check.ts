// Offline check of the match-end detection that starts the end-of-game review (MATCH_END in
// src/player-profile.ts, ReviewTracker in src/ai-review.ts). Feeds a live recording export's
// snapshots (frames[] {timestamp, snapshot}, as main.ts reviews.live does) or a replay_dir JSONL
// ({t, snapshot} per line, e.g. the frames re-scanned with the current vision) through a live
// ReviewTracker; prints where the match ended, why, whether it is reviewed automatically, and the
// start of its digest. Read-only; no network.
//   node --experimental-strip-types scripts/match-end-check.ts <recording.json | frames.jsonl> [--chars 1500]
import { readFileSync } from 'node:fs';
import { ReviewTracker, autoReviewable, buildDigest, type EndedMatch } from '../src/ai-review.ts';
import { clockText, spanOf } from '../src/player-profile.ts';
import type { Cue, Snapshot } from '../src/types.ts';
import { heroNames } from './player-profile-replay.ts';

const [file, ...rest] = process.argv.slice(2);
if (!file) { console.error('usage: match-end-check.ts <recording.json | frames.jsonl> [--chars N]'); process.exit(2); }
const chars = Number(rest[rest.indexOf('--chars') + 1]) || 1500;
const text = readFileSync(file, 'utf8');
type Frame = { wall: number; snapshot: Snapshot; cues: Cue[] };
const frames: Frame[] = file.endsWith('.json')
  ? (JSON.parse(text).frames as Array<{ timestamp: number; snapshot: Snapshot; cues?: Cue[] }>).map(f => ({ wall: f.timestamp, snapshot: f.snapshot, cues: f.cues || [] }))
  : text.split(/\r?\n/).filter(l => l.trim()).map(l => JSON.parse(l)).map((v, i) => ({ wall: 1_800_000_000_000 + (typeof v.t === 'number' ? v.t : i * 4000), snapshot: v.snapshot ?? v, cues: [] }));
const names = heroNames(); const name = (id: string) => names[id] || id.replace(/^(hero_|upgrade_|item_)/, '').replaceAll('_', ' ');

const tr = new ReviewTracker('live', () => frames.at(-1)?.wall ?? 0);
const ends: Array<{ i: number; m: EndedMatch }> = [];
frames.forEach((f, i) => { const m = tr.add(f.snapshot, f.wall, f.cues); if (m) ends.push({ i, m }); });
const t0 = frames[0]?.wall ?? 0;
const at = (i: number) => { const c = frames[i].snapshot.game_clock_secs; return `frame ${i} (${frames[i].snapshot.screen ?? '-'}, clock ${c == null ? 'none' : clockText(c)}, +${Math.round((frames[i].wall - t0) / 1000)} s)`; };
console.log(`${file}: ${frames.length} frames`);
for (const { i, m } of ends) console.log(`match ended at ${at(i)} by ${m.endedBy}: ${name(m.record.hero || '?')}, ${clockText(m.record.startClock)}-${clockText(m.record.endClock)}, ${Math.round(spanOf(m.record) / 60)} min observed, ${m.record.deaths.length} deaths -> ${autoReviewable(m) ? 'REVIEW REQUESTED' : 'no automatic review'}`);
// Stop after the last frame closes a match still open after 10+ minutes.
const stopped = tr.stop();
if (stopped) { ends.push({ i: frames.length - 1, m: stopped }); console.log(`match still open at the last frame: Stop closes it (${Math.round(spanOf(stopped.record) / 60)} min) -> ${autoReviewable(stopped) ? 'REVIEW REQUESTED' : 'no automatic review'}`); }
const reviewed = ends.filter(e => autoReviewable(e.m));
if (!reviewed.length) { console.log('no review would be requested'); process.exit(1); }
const digest = buildDigest({ match: reviewed.at(-1)!.m, previous: [], name });
console.log(`\ndigest: ${digest.chars} characters, key ${digest.key}; first ${chars}:\n${digest.text.slice(0, chars)}`);
