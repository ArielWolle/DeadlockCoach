// Offline evaluation of the AI coach's triggers, souls gating and repeat suppression on a recorded
// match. Feeds the recording's snapshots and rule cues through the real AiCoach controller with a
// FAKE model that answers "spend your souls" every time, then prints what would have been spoken
// and three full sample prompts (mid-game periodic, death review, just after a respawn).
// Read-only on its inputs; no network.
//   node --experimental-strip-types scripts/ai-coach-eval.ts [replay.jsonl] [--profile a.jsonl,b.jsonl] [--spoken spoken.jsonl] [--quiet]
// Defaults: frames/rec8/replay1.jsonl of the main checkout, a profile learned from rec4-rec6, and
// rec8/review/spoken.jsonl for the live AI calls of that match.
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { AiCoach, SYSTEM_PROMPT, setProfileProvider, clockText, type Prompt } from '../src/ai-coach.ts';
import { defaults, normalize } from '../src/settings.ts';
import type { Cue, Knowledge } from '../src/types.ts';
import { profileOf } from './player-profile-replay.ts';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
// Recordings live in the main checkout (git-ignored); worktrees sit next to it.
const framesRoot = [path.join(repo, 'frames'), path.resolve(repo, '..', '..', 'DeadlockCoach', 'frames')].find(p => existsSync(p)) || path.join(repo, 'frames');
const args = process.argv.slice(2);
const opt = (flag: string) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : undefined; };
const replayPath = args.find((a, i) => !a.startsWith('--') && !args[i - 1]?.startsWith('--')) || path.join(framesRoot, 'rec8', 'replay1.jsonl');
const profileFiles = (opt('--profile')?.split(',') || ['rec4/replay9.jsonl', 'rec5/replay7.jsonl', 'rec6/replay5.jsonl'].map(f => path.join(framesRoot, f))).filter(f => existsSync(f));
const spokenPath = opt('--spoken') || path.join(path.dirname(replayPath), 'review', 'spoken.jsonl');
const quiet = args.includes('--quiet');

// Knowledge and hero names from docs/ (as the unit tests do).
const docs: Knowledge[] = [];
for (const folder of ['timings', 'jungle', 'strategy', 'heroes']) for (const file of readdirSync(path.join(repo, 'docs', folder)).filter(n => n.endsWith('.md')).sort()) {
  const body = readFileSync(path.join(repo, 'docs', folder, file), 'utf8').replace(/^\uFEFF/, '');
  docs.push({ id: `${folder}/${file}`, category: folder, title: body.match(/^#\s+(.+)$/m)?.[1] || file, body });
}
const heroNames: Record<string, string> = {};
for (const d of docs.filter(d => d.category === 'heroes')) { const id = d.body.match(/`(hero_[a-z0-9_]+)`/)?.[1]; if (id && !heroNames[id]) heroNames[id] = d.title.replace(/\s*[(:—-].*$/, '').trim(); }
const name = (id: string) => heroNames[id] || id.replace(/^(hero_|upgrade_|item_)/, '').replaceAll('_', ' ');

// Player profile from earlier matches.
const profile = profileFiles.length ? profileOf(profileFiles, false).promptText() : null;
setProfileProvider(() => profile);

// The fake model: always souls advice, worded like the real rec8 calls.
let now = 0;
const calls: Array<{ clock: string; trigger: string; retry: boolean; body: string }> = [];
const fetchImpl = (async (_url: string, init: RequestInit) => {
  const body = JSON.parse(String(init.body)); const user: string = body.messages.at(-1).content;
  const trigger = user.match(/^Trigger: (\w+)/m)?.[1] || '?', clock = user.match(/clock (\d+:\d\d)/)?.[1] || '?:??';
  calls.push({ clock, trigger, retry: user.includes('WAS NOT USED'), body: user });
  const souls = user.match(/(?:Unspent souls|Souls) ([\d,]+)/)?.[1] || '2,000';
  const speak = trigger === 'death'
    ? `You died at close range again. Spend your ${souls} souls on a 1,600 item now and stay behind your front line.`
    : `Spend your ${souls} souls now on a 1,600 item - you're holding too long.`;
  const focus = calls.length % 2 ? 'souls' : 'items';
  return new Response(JSON.stringify({ model: 'fake-souls-model', choices: [{ message: { content: JSON.stringify({ speak, priority: 'Normal', why: 'souls', focus }) } }] }), { status: 200 });
}) as unknown as typeof fetch;

const settings = normalize({ ...defaults, aiEnabled: true, aiModel: 'fake-souls-model', aiInterval: 40 });
const delivered: Array<{ clock: string; trigger: string; text: string; topic: string }> = [];
const skipped: Array<{ clock: string; reason: string }> = [];
let lastClock: number | null = null, seenSuppressed = 0;
const coach = new AiCoach({ settings: () => settings, isLive: () => true, generation: () => 1, knowledge: async () => docs, now: () => now, fetchImpl, name,
  onAdvice: (cue, advice, meta) => delivered.push({ clock: clockText(meta.clock), trigger: meta.trigger, text: cue.text, topic: advice.focus }),
  onStatus: s => { if ((s.suppressed || 0) > seenSuppressed) { seenSuppressed = s.suppressed!; skipped.push({ clock: clockText(lastClock), reason: s.message }); } } });
const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };

const lines = readFileSync(replayPath, 'utf8').split(/\r?\n/).filter(l => l.trim()).map(l => JSON.parse(l) as { t?: number; snapshot: Record<string, unknown>; cues?: Cue[] });
const samples: Record<'periodic' | 'death' | 'respawn', { clock: string; prompt: Prompt } | null> = { periodic: null, death: null, respawn: null };
let start = lines.find(l => typeof l.t === 'number')?.t ?? 0;
for (const [i, rec] of lines.entries()) {
  now = typeof rec.t === 'number' ? rec.t : start + i * 3000; if (i === 0) start = now;
  const before = calls.length;
  const clock = typeof rec.snapshot.game_clock_secs === 'number' ? rec.snapshot.game_clock_secs : null;
  if (clock != null) lastClock = clock;
  coach.observe(rec.snapshot, rec.cues || [], true);
  await flush();
  if (calls.length > before) {
    const c = calls[before];
    if (c.trigger === 'periodic' && !samples.periodic && clock != null && clock >= 1500 && !coach.history.respawned(now)) samples.periodic = { clock: c.clock, prompt: { system: SYSTEM_PROMPT, user: c.body, chars: 0, tokens: 0 } };
    if (c.trigger === 'death' && clock != null && clock >= 1600 && !samples.death) samples.death = { clock: c.clock, prompt: { system: SYSTEM_PROMPT, user: c.body, chars: 0, tokens: 0 } };
  }
  const back = coach.history.respawned(now);
  if (back && back.ago >= 5 && !samples.respawn && clock != null && clock >= 1600) samples.respawn = { clock: clockText(clock), prompt: await coach.prompt('periodic') };
}

const original = existsSync(spokenPath) ? readFileSync(spokenPath, 'utf8').split(/\r?\n/).filter(l => l.includes('"source":"ai"')).map(l => JSON.parse(l) as { clock: number; text: string }) : [];
const soulsy = (t: string) => /\bsouls?\b(?!\s*urn)|\bspend\b|\b(1,?600|3,?200|6,?400)\b/i.test(t);
console.log(`AI COACH OFFLINE EVAL · ${path.relative(repo, replayPath) || replayPath} · ${lines.length} frames · profile from ${profileFiles.length} earlier match file(s)`);
console.log(`Fake model: answers souls advice on every request.\n`);
if (original.length) {
  console.log(`BEFORE (live recording, previous AI coach): ${original.length} AI calls spoken, ${original.filter(o => soulsy(o.text)).length} about souls`);
  for (const o of original) console.log(`  [${clockText(o.clock)}] ${o.text}`);
  console.log('');
}
console.log(`AFTER (this build, same frames, fake souls model):`);
console.log(`  requests ${calls.length} (${calls.filter(c => c.retry).length} cheap retries) · spoken ${delivered.length} · souls advice spoken ${delivered.filter(d => soulsy(d.text)).length} · skipped ${coach.status.suppressed || 0}`);
for (const d of delivered) console.log(`  SPOKEN  [${d.clock}] ${d.trigger}/${d.topic}: ${d.text}`);
const grouped: Array<{ clock: string; reason: string; n: number }> = [];
for (const s of skipped) { const g = grouped.at(-1); if (g && g.clock === s.clock && g.reason === s.reason) g.n++; else grouped.push({ ...s, n: 1 }); }
for (const s of grouped) console.log(`  SKIPPED [${s.clock}] ${s.reason.replace(/^Skipped a repeat: /, '')}${s.n > 1 ? ` (again after the cheap retry)` : ''}`);
if (!quiet) for (const [kind, s] of Object.entries(samples)) {
  console.log(`\n${'='.repeat(100)}\nSAMPLE PROMPT · ${kind} · ${s ? s.clock : 'not found in this recording'}\n${'='.repeat(100)}`);
  if (!s) continue;
  if (kind === 'periodic') console.log(`--- system (${s.prompt.system.length} chars) ---\n${s.prompt.system}\n--- user ---`);
  console.log(s.prompt.user);
}
