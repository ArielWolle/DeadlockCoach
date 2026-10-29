// Feed recorded coach frames through the player profile and print what it learned. Read-only on
// the input files.
//   node --experimental-strip-types scripts/player-profile-replay.ts <file.jsonl> [more.jsonl]
//     JSONL: one {t|ms, snapshot} per line, or bare snapshots; add --no-wall to ignore the recorded
//     wall time (as a replay in the app does).
//   node --experimental-strip-types scripts/player-profile-replay.ts --live <recording.json>...
//     Replay lab exports ({frames: [{timestamp, snapshot}]}): the live full-scan snapshots, fed as
//     main.ts playerFrame does (one profile across all files, frames in wall-clock order, now() =
//     frame time, focusCue() at each match start and respawn). Prints each match and every cue.
import { readFileSync } from 'node:fs';
import { PlayerProfile, clockText, deadOf, spanOf, type MatchRecord } from '../src/player-profile.ts';
import type { Snapshot } from '../src/types.ts';

export function framesOf(text: string) {
  return text.split(/\r?\n/).filter(l => l.trim()).map(l => {
    const v = JSON.parse(l);
    return v && typeof v === 'object' && v.snapshot ? { snapshot: v.snapshot, t: typeof v.t === 'number' ? v.t : null } : { snapshot: v, t: null };
  });
}
/** Hero display names from data/heroes.json (the app loads the same list as assets/heroes.json). */
export function heroNames(): Record<string, string> {
  try {
    const list = JSON.parse(readFileSync(new URL('../../data/heroes.json', import.meta.url), 'utf8').replace(/^\uFEFF/, '')) as Array<{ class_name: string; name: string }>;
    return Object.fromEntries(list.map(h => [h.class_name, h.name]));
  } catch { return {}; }
}
export function profileOf(files: string[], wall = true, verbose = false) {
  let clock = 1_700_000_000_000;
  const profile = new PlayerProfile({ now: () => clock });
  profile.setHeroNames(heroNames());
  for (const file of files) {
    const frames = framesOf(readFileSync(file, 'utf8'));
    const base = clock;
    for (const f of frames) {
      if (f.t != null) clock = base + f.t;
      profile.observe(f.snapshot, wall && f.t != null ? base + f.t : null);
    }
    const rec = profile.endMatch();
    if (verbose && rec) console.log(file, JSON.stringify(rec, null, 1));
    clock += 3_600_000;
  }
  return profile;
}

export interface LiveCue { file: string; wall: number; clock: number | null; event: 'start' | 'respawn'; key: string; text: string }
export interface LiveMatch { file: string; record: MatchRecord }
/** Live frames of Replay lab exports, merged in wall-clock order (exports can overlap: rec11 and
 *  rec12 share two minutes), duplicates removed. */
export function liveFramesOf(files: string[]): Array<{ file: string; wall: number; snapshot: Snapshot }> {
  const seen = new Set<number>(); const out: Array<{ file: string; wall: number; snapshot: Snapshot }> = [];
  for (const file of files) {
    const data = JSON.parse(readFileSync(file, 'utf8')) as { frames?: Array<{ timestamp?: number; snapshot?: Snapshot }> };
    for (const f of data.frames ?? []) if (typeof f.timestamp === 'number' && f.snapshot && !seen.has(f.timestamp)) { seen.add(f.timestamp); out.push({ file, wall: f.timestamp, snapshot: f.snapshot }); }
  }
  return out.sort((a, b) => a.wall - b.wall);
}
/** The browser's live path: one profile, observe(snapshot, wall), focusCue() at starts / respawns. */
export function liveProfileOf(files: string[]) {
  const frames = liveFramesOf(files);
  let now = frames[0]?.wall ?? 0;
  const p = new PlayerProfile({ now: () => now });
  p.setHeroNames(heroNames());
  const cues: LiveCue[] = [], matches: LiveMatch[] = [];
  let startFile = frames[0]?.file ?? '';
  for (const f of frames) {
    now = f.wall;
    const ev = p.observe(f.snapshot, f.wall);
    if (ev.ended) matches.push({ file: startFile, record: ev.ended });
    if (ev.matchStarted) startFile = f.file;
    if (ev.matchStarted || ev.respawned) {
      const cue = p.focusCue();
      if (cue) cues.push({ file: f.file, wall: f.wall, clock: typeof f.snapshot.game_clock_secs === 'number' ? f.snapshot.game_clock_secs : null, event: ev.matchStarted ? 'start' : 'respawn', key: cue.key, text: cue.text });
    }
  }
  const last = p.endMatch(); if (last) matches.push({ file: startFile, record: last });
  return { profile: p, cues, matches };
}

const isMain = import.meta.url === `file:///${process.argv[1]?.replaceAll('\\', '/')}` || process.argv[1]?.endsWith('player-profile-replay.ts');
if (isMain && process.argv.includes('--live')) {
  const { profile, cues, matches } = liveProfileOf(process.argv.slice(2).filter(a => !a.startsWith('--')));
  const short = (f: string) => f.replace(/\\/g, '/').split('/').slice(-2, -1)[0] || f;
  for (const c of cues) console.log(`${short(c.file)} ${c.clock == null ? '--' : clockText(c.clock)} ${c.event}: [${c.key}] ${c.text}`);
  console.log('\nMATCHES');
  for (const { file, record: m } of matches) console.log(`${short(file)} ${profile.model.heroName(m.hero)} ${m.startClock == null ? '--' : clockText(m.startClock)}-${m.endClock == null ? '--' : clockText(m.endClock)}: ${m.deaths.length} deaths, dead ${deadOf(m)} of ${spanOf(m)} s = ${Math.round(deadOf(m) * 100 / Math.max(1, spanOf(m)))}%, windows ${m.windows.used}/${m.windows.used + m.windows.unused}, urn ${m.urn?.delivered ?? 0}`);
  const s = profile.summary();
  console.log('\nSTRENGTHS'); for (const t of s.strengths) console.log(`- [${t.id}] ${t.label}: ${t.evidence}`);
  console.log('IMPROVED'); for (const t of s.improvements) console.log(`- [${t.id}] ${t.label}: ${t.evidence}`);
  console.log('WEAKNESSES'); for (const t of s.weaknesses) console.log(`- [${t.id}] ${t.label}: ${t.evidence}`);
  console.log('\nFOCUS\n' + profile.focusTips(4).join('\n'));
  console.log('\nPROMPT (' + (profile.promptText()?.length ?? 0) + ' chars)\n' + profile.promptText());
} else if (isMain) {
  const args = process.argv.slice(2);
  const profile = profileOf(args.filter(a => !a.startsWith('--')), !args.includes('--no-wall'), args.includes('--verbose'));
  const s = profile.summary();
  for (const [i, r] of s.rows.entries()) console.log(`match ${r.heroName}: ${r.minutes} min, ${r.deaths} deaths, ${r.deadPct}% dead (${profile.matches[i]?.deadSecs} of ${profile.matches[i]?.spanSecs} s; per death ${profile.matches[i]?.deaths.map(d => d.deadSecs).join('/')}), fights ${r.fightsJoined}/${r.fightsJoined + r.fightsMissed}, unspent at death ${r.unspentAtDeath}, top killer ${r.topKiller}${r.excluded ? ` (not counted: ${r.excluded})` : ''}`);
  console.log('\nWEAKNESSES');
  for (const t of s.weaknesses) console.log(`- [${t.id}] ${t.label} (sev ${t.severity}, conf ${t.confidenceLabel} ${t.confidence}, ${t.trend}): ${t.evidence}\n    tip: ${t.tip}`);
  console.log('STRENGTHS');
  for (const t of s.strengths) console.log(`- [${t.id}] ${t.label} (${t.scope}, conf ${t.confidenceLabel}): ${t.evidence}`);
  console.log('IMPROVED');
  for (const t of s.improvements) console.log(`- [${t.id}] ${t.label}: ${t.evidence}`);
  console.log('\nPROMPT (' + (profile.promptText()?.length ?? 0) + ' chars)\n' + profile.promptText());
  console.log('\nFOCUS\n' + profile.focusTips(3).join('\n'));
}
