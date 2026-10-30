// Replays live recordings through the skill mastery (web/src/mastery.ts) as main.ts does: each
// recorded full-scan frame is observed (with the cues the engine gave it), and each spoken cue of
// the recording (spoken[]: both analysis paths and the AI) goes through the suppression gate in
// wall-clock order. One mastery store across all files, oldest first. Read-only on the inputs.
//   node --experimental-strip-types scripts/mastery-validate.ts [--no-ai] <recording.json>...
//     --no-ai: leave out the recorded AI advice (a coach without the AI).
import { readFileSync } from 'node:fs';
import { SkillMastery, metricsOf, skillOfCue, SKILLS, LEVEL_LABEL, type SkillEvent } from '../src/mastery.ts';
import { PlayerProfile, clockText } from '../src/player-profile.ts';
import type { Cue, Snapshot } from '../src/types.ts';
import { heroNames } from './player-profile-replay.ts';

interface Spoken { t: number; clock: number | null; source: string; key: string; text: string; priority: Cue['priority'] }
interface Frame { timestamp: number; snapshot: Snapshot; cues?: Cue[] }
export interface FileReport { file: string; spoken: number; suppressed: number; bySkill: Record<string, number>; byReason: Record<string, number>; events: SkillEvent[]; mastered: string[]; level: string;
  allowed: Array<{ clock: number | null; key: string; reason: string; text: string }>; suppressedCues: Array<{ clock: number | null; key: string; reason: string }> }

export function validate(files: string[], log = false, ai = true): FileReport[] {
  let now = 0;
  const profile = new PlayerProfile({ now: () => now }); profile.setHeroNames(heroNames());
  const mastery = new SkillMastery({ now: () => now, metrics: () => metricsOf(profile) });
  const out: FileReport[] = [];
  for (const file of files) {
    const rec = JSON.parse(readFileSync(file, 'utf8')) as { frames: Frame[]; spoken?: Spoken[] };
    const frames = rec.frames.filter(f => f && f.snapshot && Number.isFinite(f.timestamp)).sort((a, b) => a.timestamp - b.timestamp);
    const t0 = frames[0].timestamp - 60_000, t1 = frames.at(-1)!.timestamp + 60_000;
    // The cue log keeps earlier sessions (rec12's includes rec11's): only this recording's span.
    const spoken = (rec.spoken || []).filter(s => s.t >= t0 && s.t <= t1 && (ai || s.source !== 'ai')).sort((a, b) => a.t - b.t);
    const report: FileReport = { file, spoken: spoken.length, suppressed: 0, bySkill: {}, byReason: {}, events: [], mastered: [], level: '', allowed: [], suppressedCues: [] };
    let i = 0;
    const speakUntil = (t: number) => {
      for (; i < spoken.length && spoken[i].t <= t; i++) {
        const s = spoken[i]; now = s.t;
        const cue: Cue = { key: s.key, text: s.text, priority: s.priority, cooldown_secs: 0 };
        // As main.ts deliver(): every offered cue is evidence, then the gate decides.
        if (s.source !== 'ai') mastery.offered(cue, s.clock);
        const g = mastery.gate(cue, s.clock);
        if (g.speak) { mastery.noteSpoken({ ...cue, text: g.text ?? cue.text }, s.clock); if (g.skill) report.allowed.push({ clock: s.clock, key: s.key, reason: g.reason, text: s.text }); }
        else {
          report.suppressed++; report.bySkill[g.skill!] = (report.bySkill[g.skill!] || 0) + 1; report.byReason[g.reason] = (report.byReason[g.reason] || 0) + 1;
          report.suppressedCues.push({ clock: s.clock, key: s.key, reason: g.reason });
        }
      }
    };
    const before = mastery.events.length;
    const events: SkillEvent[] = [];
    for (const f of frames) {
      speakUntil(f.timestamp - 1);
      now = f.timestamp;
      profile.observe(f.snapshot, f.timestamp);
      const cues = (f.cues || []).map(c => ({ ...c, skill: c.skill ?? skillOfCue(c) }));
      events.push(...mastery.observe(f.snapshot, f.timestamp, cues));
    }
    speakUntil(Infinity);
    void before;
    now += 60_000; profile.finishSession(0); mastery.finishSession();
    report.events = events;
    const lv = mastery.level(); report.level = `${LEVEL_LABEL[lv.level]} (${lv.basis})`;
    report.mastered = SKILLS.filter(s => mastery.data.skills[s.id]?.mastered).map(s => s.id);
    if (log) print(report, mastery);
    out.push(report);
    now += 3_600_000;
  }
  return out;
}
function print(r: FileReport, m: SkillMastery) {
  console.log(`\n== ${r.file}`);
  const per: Record<string, { ok: number; pr: number; fail: number }> = {};
  for (const e of r.events) {
    if (e.kind === 'mastered' || e.kind === 'unmastered') { console.log(`  ${e.kind.toUpperCase()} ${e.skill} at ${e.clock != null ? clockText(e.clock) : '?'}: ${e.detail}`); continue; }
    const p = per[e.skill] ??= { ok: 0, pr: 0, fail: 0 };
    if (e.kind === 'failure') p.fail++; else if (e.prompted) p.pr++; else p.ok++;
  }
  for (const [s, p] of Object.entries(per)) console.log(`  ${s}: ${p.ok} unprompted, ${p.pr} prompted, ${p.fail} failed`);
  for (const e of r.events.filter(e => e.kind === 'failure' && /souls/.test(e.detail))) console.log(`  failure ${e.skill} at ${e.clock != null ? clockText(e.clock) : '?'}: ${e.detail}`);
  console.log(`  level: ${r.level}`);
  console.log(`  mastered: ${r.mastered.join(', ') || '-'}  | state of basics: ${SKILLS.filter(s => s.level === 'basic').map(s => `${s.id}=${m.state(s.id)}`).join(', ')}`);
  console.log(`  spoken ${r.spoken}, suppressed ${r.suppressed} (${Math.round(r.suppressed * 100 / Math.max(1, r.spoken))}%): ${JSON.stringify(r.bySkill)} ${JSON.stringify(r.byReason)}`);
  const shop = r.allowed.filter(a => /^unspent|^player:focus:unspent/.test(a.key) || (/^ai:/.test(a.key) && /\bspend\b[^.]*souls/i.test(a.text)));
  const rule = (k: string) => /^unspent|^player:focus(:goal)?:unspent_souls/.test(k);
  console.log(`  rule shop reminders: ${r.allowed.filter(a => rule(a.key)).length + r.suppressedCues.filter(a => rule(a.key)).length} recorded -> ${r.allowed.filter(a => rule(a.key)).length} spoken`);
  console.log(`  shop reminders still spoken: ${shop.map(a => `${a.clock != null ? clockText(a.clock) : '?'} ${a.key} (${a.reason})`).join('; ') || 'none'}`);
}
if (process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/g, '/').split('/').pop()!)) validate(process.argv.slice(2).filter(a => a !== '--no-ai'), true, !process.argv.includes('--no-ai'));
