// Feed recorded coach frames (JSONL: one {t|ms, snapshot} per line, or bare snapshots) through the
// player profile and print what it learned. Read-only on the input files.
//   node --experimental-strip-types scripts/player-profile-replay.ts <file.jsonl> [more.jsonl]
//   add --no-wall to ignore the recorded wall time (as a replay in the app does).
import { readFileSync } from 'node:fs';
import { PlayerProfile } from '../src/player-profile.ts';

export function framesOf(text: string) {
  return text.split(/\r?\n/).filter(l => l.trim()).map(l => {
    const v = JSON.parse(l);
    return v && typeof v === 'object' && v.snapshot ? { snapshot: v.snapshot, t: typeof v.t === 'number' ? v.t : null } : { snapshot: v, t: null };
  });
}
export function profileOf(files: string[], wall = true, verbose = false) {
  let clock = 1_700_000_000_000;
  const profile = new PlayerProfile({ now: () => clock });
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

if (import.meta.url === `file:///${process.argv[1]?.replaceAll('\\', '/')}` || process.argv[1]?.endsWith('player-profile-replay.ts')) {
  const args = process.argv.slice(2);
  const profile = profileOf(args.filter(a => !a.startsWith('--')), !args.includes('--no-wall'), args.includes('--verbose'));
  const s = profile.summary();
  for (const r of s.rows) console.log(`match ${r.heroName}: ${r.minutes} min, ${r.deaths} deaths, ${r.deadPct}% dead, fights ${r.fightsJoined}/${r.fightsJoined + r.fightsMissed}, unspent at death ${r.unspentAtDeath}, top killer ${r.topKiller}`);
  console.log('\nWEAKNESSES');
  for (const t of s.weaknesses) console.log(`- [${t.id}] ${t.label} (sev ${t.severity}, conf ${t.confidenceLabel} ${t.confidence}, ${t.trend}): ${t.evidence}\n    tip: ${t.tip}`);
  console.log('STRENGTHS');
  for (const t of s.strengths) console.log(`- [${t.id}] ${t.label} (conf ${t.confidenceLabel}): ${t.evidence}`);
  console.log('\nPROMPT (' + (profile.promptText()?.length ?? 0) + ' chars)\n' + profile.promptText());
  console.log('\nFOCUS\n' + profile.focusTips(3).join('\n'));
}
