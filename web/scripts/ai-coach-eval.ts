// Offline evaluation of the AI coach's cadence, theme memory, souls gating and repeat suppression on
// a recorded match. Feeds the recording's snapshots and rule cues through the real AiCoach
// controller with a FAKE model that answers like the live rec11 model did (the same stock lines:
// "hold max range behind your front line", "walk out with a teammate", "heal first", "Midboss is up,
// don't contest it alone", souls), then prints what would have been asked and spoken, what was gated
// and why, and two full sample prompts. Read-only on its inputs; no network unless AI_EVAL_* is set.
//   node --experimental-strip-types scripts/ai-coach-eval.ts [recording.json | replay.jsonl] [--profile a.jsonl,b.jsonl] [--spoken spoken.jsonl] [--quiet]
// Defaults: frames/rec11/recording.json of the main checkout (live export with frames, spoken cues
// and the AI exchange log); the player profile is the one the live coach used (from its prompts).
// Optional real-model check: set AI_EVAL_URL (base URL), AI_EVAL_KEY, AI_EVAL_MODEL (and optionally
// AI_EVAL_PROVIDER=anthropic, AI_EVAL_REASONING=off|low|default, AI_EVAL_N=5) to send a handful of the
// generated prompts to that gateway and print the answers. Keys are never printed or written.
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { AiCoach, EVENT_GAP_SECS, HERO_NAMES, RULE_THEME_SECS, SYSTEM_PROMPT, ruleThemes, topicOf, callGateway, checkAdvice, clockText, parseAdvice, setProfileProvider, soulsView, type AbilityRow, type AiConfig, type AiReasoning, type ItemInfo } from '../src/ai-coach.ts';
import { defaults, normalize } from '../src/settings.ts';
import type { Cue, Knowledge } from '../src/types.ts';
import { profileOf } from './player-profile-replay.ts';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
// Recordings live in the main checkout (git-ignored); worktrees sit next to it.
const framesRoot = [path.join(repo, 'frames'), path.resolve(repo, '..', '..', 'DeadlockCoach', 'frames')].find(p => existsSync(p)) || path.join(repo, 'frames');
const args = process.argv.slice(2);
const opt = (flag: string) => { const i = args.indexOf(flag); return i >= 0 ? args[i + 1] : undefined; };
const inputPath = args.find((a, i) => !a.startsWith('--') && !args[i - 1]?.startsWith('--')) || path.join(framesRoot, 'rec11', 'recording.json');
const quiet = args.includes('--quiet');

// Knowledge, hero names and the item catalog from docs/ and data/ (as the unit tests do).
const docs: Knowledge[] = [];
for (const folder of ['timings', 'jungle', 'strategy', 'heroes']) for (const file of readdirSync(path.join(repo, 'docs', folder)).filter(n => n.endsWith('.md')).sort()) {
  const body = readFileSync(path.join(repo, 'docs', folder, file), 'utf8').replace(/^\uFEFF/, '');
  docs.push({ id: `${folder}/${file}`, category: folder, title: body.match(/^#\s+(.+)$/m)?.[1] || file, body });
}
const heroNames: Record<string, string> = {};
for (const d of docs.filter(d => d.category === 'heroes')) { const id = d.body.match(/`(hero_[a-z0-9_]+)`/)?.[1]; if (id && !heroNames[id]) heroNames[id] = d.title.replace(/\s*[(:—-].*$/, '').trim(); }
const itemList = JSON.parse(readFileSync(path.join(repo, 'data', 'items.json'), 'utf8')) as Array<{ class_name: string; name: string; cost: number; slot?: string }>;
const items: Record<string, ItemInfo> = Object.fromEntries(itemList.map(i => [i.class_name, { name: i.name, cost: i.cost, slot: i.slot ?? null }]));
const name = (id: string) => heroNames[id] || items[id]?.name || id.replace(/^(hero_|upgrade_|item_)/, '').replaceAll('_', ' ');

// Frames: a live recording export ({frames:[{timestamp, snapshot, cues}], spoken, ai}) or replay jsonl.
type Frame = { t: number; snapshot: Record<string, unknown>; cues: Cue[] };
let frames: Frame[] = [], original: Array<{ clock: number | null; text: string }> = [], liveProfile: string | null = null, liveAi = 0;
let liveSpoken: Array<{ t: number; clock: number | null; source: string; key?: string; text: string }> = [];
let livePrompts: Array<{ trigger: string; chars: number; brief: number }> = [], merged = 0, mergedFrom: string | null = null;
if (inputPath.endsWith('.json')) {
  const rec = JSON.parse(readFileSync(inputPath, 'utf8')) as { frames: Array<{ timestamp: number; snapshot: Record<string, unknown>; cues?: Cue[] }>; spoken?: Array<{ t: number; clock: number | null; source: string; key?: string; text: string }>; ai?: Array<{ t: number; prompt: string; trigger: string }> };
  frames = rec.frames.map(f => ({ t: f.timestamp, snapshot: f.snapshot, cues: f.cues || [] }));
  const first = frames[0]?.t ?? 0;
  original = (rec.spoken || []).filter(c => c.source === 'ai' && c.t >= first - 3600000).map(c => ({ clock: c.clock, text: c.text }));
  liveSpoken = (rec.spoken || []).filter(c => c.t >= first - 3600000) as typeof liveSpoken;
  liveAi = (rec.ai || []).filter(x => x.t >= first).length;
  liveProfile = (rec.ai || []).map(x => x.prompt.split(/PLAYER PROFILE \(from earlier matches[^\n]*\)\n/)[1]?.split('\n')[0]).find(Boolean) || null;
  // The live AI log (frames/<rec>/ai-log.json) has the full prompts: their sizes are the "before".
  const logPath = path.join(path.dirname(inputPath), 'ai-log.json');
  const log = existsSync(logPath) ? JSON.parse(readFileSync(logPath, 'utf8')) as { exchanges?: Array<{ trigger: string; system_chars?: number; prompt?: string }> } : null;
  const xs = log?.exchanges?.length ? log.exchanges : (rec.ai || []).map(x => ({ trigger: x.trigger, system_chars: undefined, prompt: x.prompt }));
  livePrompts = xs.filter(x => typeof x.prompt === 'string').map(x => ({ trigger: x.trigger.replace(/:.*/, ''), chars: (x.system_chars ?? 3200) + x.prompt!.length, brief: x.prompt!.includes('DEADLOCK FACTS') ? (x.prompt!.split('DEADLOCK FACTS')[1]?.split('\n\n')[0].length ?? 0) + 14 : 0 }));
  // Newer scans of the same frames (replay*.jsonl next to it) read enemy items and scoreboard rows
  // that the live export lacks: merged by game clock (--merge none skips it).
  const mergePath = opt('--merge') ?? readdirSync(path.dirname(inputPath)).filter(n => /^replay\d*\.jsonl$/.test(n)).map(n => path.join(path.dirname(inputPath), n))[0];
  if (mergePath && mergePath !== 'none' && existsSync(mergePath)) {
    const byClock = new Map<number, Record<string, unknown>>();
    for (const l of readFileSync(mergePath, 'utf8').split(/\r?\n/).filter(x => x.trim())) { const s = (JSON.parse(l) as { snapshot: Record<string, unknown> }).snapshot; if (typeof s?.game_clock_secs === 'number' && !byClock.has(s.game_clock_secs)) byClock.set(s.game_clock_secs, s); }
    for (const f of frames) {
      const m = typeof f.snapshot.game_clock_secs === 'number' ? byClock.get(f.snapshot.game_clock_secs) : undefined; if (!m) continue;
      let used = false;
      for (const k of ['enemy_items', 'players', 'match_result', 'structures', 'midboss_taken_by', 'abilities']) if (m[k] != null && (!Array.isArray(m[k]) || (m[k] as unknown[]).length) && (f.snapshot[k] == null || (Array.isArray(f.snapshot[k]) && !(f.snapshot[k] as unknown[]).length))) { f.snapshot = { ...f.snapshot, [k]: m[k] }; used = true; }
      if (used) merged++;
    }
    mergedFrom = mergePath;
  }
} else {
  const lines = readFileSync(inputPath, 'utf8').split(/\r?\n/).filter(l => l.trim()).map(l => JSON.parse(l) as { t?: number; snapshot: Record<string, unknown>; cues?: Cue[] });
  const start = lines.find(l => typeof l.t === 'number' && l.t > 1e9)?.t ?? 0;
  frames = lines.map((l, i) => ({ t: typeof l.t === 'number' && l.t > 1e9 ? l.t : start + i * 3000, snapshot: l.snapshot, cues: l.cues || [] }));
  const spokenPath = opt('--spoken') || path.join(path.dirname(inputPath), 'review', 'spoken.jsonl');
  if (existsSync(spokenPath)) original = readFileSync(spokenPath, 'utf8').split(/\r?\n/).filter(l => l.includes('"source":"ai"')).map(l => JSON.parse(l) as { clock: number; text: string });
}
// The previous match of the same live session first (rec12 followed rec11's Violet match in one
// session; the live coach carried that match's deaths and heroes into rec12). --prepend none skips it.
const prependPath = opt('--prepend') ?? (/rec12[\\/]recording\.json$/.test(inputPath) ? path.join(framesRoot, 'rec11', 'recording.json') : undefined);
let prepended = 0;
const mainStart = frames[0]?.t ?? 0;
if (prependPath && prependPath !== 'none' && existsSync(prependPath)) {
  const prev = JSON.parse(readFileSync(prependPath, 'utf8')) as { frames: Array<{ timestamp: number; snapshot: Record<string, unknown>; cues?: Cue[] }> };
  const before = prev.frames.filter(f => f.timestamp < mainStart).map(f => ({ t: f.timestamp, snapshot: f.snapshot, cues: f.cues || [] }));
  prepended = before.length; frames = [...before, ...frames];
}
const profileFiles = (opt('--profile')?.split(',') || ['rec4/replay9.jsonl', 'rec5/replay7.jsonl', 'rec6/replay5.jsonl'].map(f => path.join(framesRoot, f))).filter(f => existsSync(f));
const profile = liveProfile ?? (profileFiles.length ? profileOf(profileFiles, false).promptText() : null);
setProfileProvider(() => profile);

// The fake model: the stock answers the live model gave in rec11, picked from the prompt.
let now = 0;
const calls: Array<{ clock: string; trigger: string; retry: boolean; body: string; why: string | null }> = [];
function fakeAnswer(user: string, trigger: string): { speak: string; focus: string; priority: string } {
  const angles = user.split('COACHING ANGLES')[1]?.split('\n\n')[0] || '';
  const souls = user.match(/Unspent souls ([\d,]+)/)?.[1];
  if (trigger === 'death') {
    // Like the live model: it names the killer from the rule death review or the damage-by-hero line.
    const killer = user.match(/Rule death review of this death \(already spoken\): Why you died: ([A-Z][\w &'.-]+?) did/)?.[1] || user.match(/^Damage by hero[^:]*: ([A-Z][\w &'.-]+?) \d/m)?.[1];
    return { speak: `${killer ? `${killer} caught you at close range.` : 'Melee finished you again.'} Next life, hold max range behind your front line and heal above 60% before you go back in.`, focus: 'death', priority: 'High' };
  }
  // A good play the summary allows praising: one short, specific praise and the next step.
  const good = user.match(/\nPraise: allowed \(fresh good play: ([^\n]+?)\)\n/)?.[1]?.replace(/ at \d+:\d\d.*$/, '').replace(/ \(.*$/, '') ?? null;
  if (good && calls.length % 2 === 0) return { speak: `Good play: you ${good.trim()}. Keep doing that, then reset before they respawn.`, focus: 'reinforce', priority: 'Normal' };
  if (/Respawned \d+s ago/.test(angles)) return { speak: 'You just respawned: walk out with a teammate, not alone, and stay at max range behind your front line.', focus: 'team', priority: 'High' };
  if (/enemies dead/.test(angles)) return { speak: 'Enemies are down: group with your team and push the furthest lane now. Stay at max range from melee.', focus: 'objective', priority: 'High' };
  if (/Health \d+% and not recovering/.test(angles)) return { speak: 'Fall back to your Guardian and heal to full before any fight, since you get burst down.', focus: 'health', priority: 'High' };
  if (souls && /Souls advice: allowed/.test(user)) return { speak: `Go back to your Guardian shop and spend your ${souls} souls on the usual buy now.`, focus: 'economy', priority: 'Normal' };
  if (/allies dead vs/.test(angles)) return { speak: 'Fall back to your Guardian with the living team and hold until your allies respawn.', focus: 'team', priority: 'High' };
  if (/Midboss alive/.test(user) && calls.length % 3 === 0) return { speak: 'Midboss is up, so do not contest it alone. Take it together once your group is back.', focus: 'objective', priority: 'Normal' };
  if (/missing/.test(user) && calls.length % 2) return { speak: 'Enemies are missing, so check the minimap before you step past your wave and do not cross the map alone.', focus: 'map', priority: 'Normal' };
  return { speak: 'Calico is alive and melee killed you before, so hold max range behind your front line and save your escape for their dive.', focus: 'positioning', priority: 'High' };
}
const fetchImpl = (async (_url: string, init: RequestInit) => {
  const body = JSON.parse(String(init.body)); const user: string = body.messages.at(-1).content;
  const trigger = user.match(/^Trigger: (\w+)/m)?.[1] || '?', clock = user.match(/clock (\d+:\d\d)/)?.[1] || '?:??';
  calls.push({ clock, trigger, retry: user.includes('WAS NOT USED'), body: user, why: user.match(/^WHY NOW: (.*)$/m)?.[1] || null });
  const a = fakeAnswer(user, trigger);
  return new Response(JSON.stringify({ model: 'fake-rec11-model', choices: [{ message: { content: JSON.stringify({ speak: a.speak, priority: a.priority, why: 'fake', focus: a.focus }) }, finish_reason: 'stop' }],
    usage: { prompt_tokens: Math.ceil(user.length / 4), completion_tokens: 60, completion_tokens_details: { reasoning_tokens: 0 } } }), { status: 200 });
}) as unknown as typeof fetch;

const abilities = JSON.parse(readFileSync(path.join(repo, 'data', 'abilities.json'), 'utf8')) as AbilityRow[];
const settings = normalize({ ...defaults, aiEnabled: true, aiModel: 'fake-rec11-model' });
const delivered: Array<{ clock: string; trigger: string; text: string; topic: string; why: string | null }> = [];
const skipped: Array<{ clock: string; reason: string }> = [];
let lastClock: number | null = null, seenSuppressed = 0;
const coach = new AiCoach({ settings: () => settings, isLive: () => true, generation: () => 1, knowledge: async () => docs, now: () => now, fetchImpl, name, items: id => items[id] ?? null,
  onAdvice: (cue, advice, meta) => delivered.push({ clock: clockText(meta.clock), trigger: meta.trigger, text: cue.text, topic: advice.focus, why: meta.reason ?? null }),
  onStatus: s => { if ((s.suppressed || 0) > seenSuppressed) { seenSuppressed = s.suppressed!; skipped.push({ clock: clockText(lastClock), reason: s.message }); } } });
coach.history.setAbilities(abilities);
const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };

const samples: Record<'event' | 'death', { clock: string; user: string } | null> = { event: null, death: null };
const goodSeen: Array<{ key: string; clock: number | null; kind: string; text: string; match: number }> = [];
const boundaries: typeof coach.history.boundaries = [];
let offsets: { calls: number; delivered: number; skipped: number; event: number; quiet: number; suppressed: number } | null = null;
for (const rec of frames) {
  now = rec.t;
  if (!offsets && rec.t >= mainStart) offsets = { calls: calls.length, delivered: delivered.length, skipped: skipped.length, event: coach.status.eventRequests || 0, quiet: coach.status.quietRequests || 0, suppressed: coach.status.suppressed || 0 };
  const before = calls.length;
  const clock = typeof rec.snapshot.game_clock_secs === 'number' ? rec.snapshot.game_clock_secs : null;
  if (clock != null) lastClock = clock;
  coach.observe(rec.snapshot, rec.cues, true);
  await flush();
  for (const g of coach.history.good) if (!goodSeen.some(x => x.key === g.key && x.match === coach.history.match)) goodSeen.push({ ...g, match: coach.history.match });
  for (const b of coach.history.boundaries) if (!boundaries.includes(b)) boundaries.push(b);
  if (calls.length > before) {
    const c = calls[before];
    // The sample periodic prompt: the first event-driven one after 18:20, replaced by a late-game one
    // that shows the new game state (fed enemy, a lead being thrown, a fight just won or lost).
    const rich = /THROW RISK|Fed enemy|^Fight: /m.test(c.body);
    if (c.trigger === 'periodic' && !c.retry && c.why && !/routine check/.test(c.why) && clock != null && clock >= 1100 && (!samples.event || (rich && !/THROW RISK|Fed enemy|^Fight: /m.test(samples.event.user)))) samples.event = { clock: c.clock, user: c.body };
    if (c.trigger === 'death' && clock != null && clock >= 1100 && rec.t >= mainStart && !samples.death) samples.death = { clock: c.clock, user: c.body };
  }
}
// Report only the main recording (the prepended match only feeds the history, as it did live).
const prevMatch = offsets ? { calls: calls.splice(0, offsets.calls), delivered: delivered.splice(0, offsets.delivered) } : { calls: [], delivered: [] };
if (offsets) { skipped.splice(0, offsets.skipped); coach.status.eventRequests = (coach.status.eventRequests || 0) - offsets.event; coach.status.quietRequests = (coach.status.quietRequests || 0) - offsets.quiet; coach.status.suppressed = (coach.status.suppressed || 0) - offsets.suppressed; }
if (prepended) console.log(`(Prepended ${prepended} frames of the previous match from ${path.relative(repo, prependPath!)}, as in the live session: ${prevMatch.calls.length} requests and ${prevMatch.delivered.length} tips there, not counted below.)`);
const clocks = frames.filter(f => f.t >= mainStart).map(f => f.snapshot.game_clock_secs).filter((c): c is number => typeof c === 'number');
const minutes = clocks.length ? (Math.max(...clocks) - Math.min(...clocks)) / 60 : 0;
const firstClock = clocks.length ? Math.min(...clocks) : 0;
const before = original.filter(o => o.clock == null || o.clock >= firstClock);
console.log(`AI COACH OFFLINE EVAL · ${path.relative(repo, inputPath) || inputPath} · ${frames.length} frames · ${clockText(firstClock)}-${clockText(Math.max(...clocks))} (${minutes.toFixed(1)} game min) · profile ${liveProfile ? 'from the live prompts' : `from ${profileFiles.length} earlier match file(s)`}`);
console.log(`Fake model: answers with the rec11 model's stock lines (max range, walk out, heal, Midboss, souls).\n`);
if (before.length) {
  console.log(`BEFORE (live recording, previous AI coach, same clock range): ${before.length} AI tips spoken${liveAi ? ` from ${liveAi} gateway calls` : ''} · ${(before.length / Math.max(1, minutes)).toFixed(2)} tips/min`);
  const words = ['max range', 'melee', 'front line', 'heal', 'midboss', 'walk out', 'missing', 'alone'];
  console.log(`  stock phrases: ${words.map(w => `"${w}" ${before.filter(o => o.text.toLowerCase().includes(w)).length}`).join(' · ')}`);
  if (!quiet) for (const o of before) console.log(`  [${clockText(o.clock)}] ${o.text}`);
  console.log('');
}
const periodic = calls.filter(c => c.trigger === 'periodic' && !c.retry);
console.log(`AFTER (this build, same frames, fake model, defaults: interval ${settings.aiInterval}s quiet, events at most every ${EVENT_GAP_SECS} s):`);
console.log(`  requests ${calls.length} (${(calls.filter(c => !c.retry).length / Math.max(1, minutes)).toFixed(2)}/min without retries) · periodic ${periodic.length}: ${coach.status.eventRequests || 0} on events, ${coach.status.quietRequests || 0} quiet checks · death ${calls.filter(c => c.trigger === 'death' && !c.retry).length} · cheap retries ${calls.filter(c => c.retry).length}`);
console.log(`  spoken ${delivered.length} (${(delivered.length / Math.max(1, minutes)).toFixed(2)}/min) · gated ${coach.status.suppressed || 0} answers`);
const words = ['max range', 'melee', 'front line', 'heal', 'midboss', 'walk out', 'missing', 'alone'];
console.log(`  stock phrases spoken: ${words.map(w => `"${w}" ${delivered.filter(o => o.text.toLowerCase().includes(w)).length}`).join(' · ')}`);
const themes = new Map<string, number>(); for (const d of delivered) themes.set(d.topic, (themes.get(d.topic) || 0) + 1);
console.log(`  themes spoken: ${[...themes].map(([k, v]) => `${k} ${v}`).join(' · ')}`);
const kinds = new Map<string, number>(); for (const c of periodic) { const k = (c.why || '').match(/^(new phase|respawn|enemy-dead window|objective within 60 s|souls reach the usual buy|big team souls swing|routine check|good play)/)?.[1] || 'other'; kinds.set(k, (kinds.get(k) || 0) + 1); }
console.log(`  periodic triggers: ${[...kinds].map(([k, v]) => `${k} ${v}`).join(' · ')}`);
const reasons = new Map<string, number>(); for (const s of skipped) { const k = s.reason.replace(/^Skipped a repeat: /, '').replace(/ at \d+:\d\d/g, '').replace(/"[^"]*"/, '"…"').replace(/\.$/, ''); reasons.set(k, (reasons.get(k) || 0) + 1); }
console.log(`  gate reasons: ${[...reasons].sort((a, b) => b[1] - a[1]).map(([k, v]) => `${v}× ${k}`).join(' | ')}`);
// Match boundaries, good plays and praise share, and the death prompts' hero names vs this match.
console.log(`  match boundaries: ${boundaries.length ? boundaries.map(b => `[${clockText(b.clock)}] ${b.reason}`).join(' | ') : 'none'}`);
const praise = delivered.filter(d => d.topic === 'reinforce');
console.log(`  good plays seen ${goodSeen.length}: ${goodSeen.map(g => `[${clockText(g.clock)}] ${g.kind}${g.match ? '' : ' (previous match)'}`).join(', ')}`);
console.log(`  praise spoken ${praise.length} of ${delivered.length} AI tips (gate: at most 1 in 4)`);
for (const g of goodSeen) console.log(`    GOOD [${clockText(g.clock)}] ${g.kind}: ${g.text}`);
const heroRe = Object.values(HERO_NAMES).map(n => [n, new RegExp(`\\b${n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`)] as const);
for (const c of calls.filter(x => x.trigger === 'death' && !x.retry)) {
  const state = c.body.split('\nPLAYER PROFILE')[0];
  const roster = (state.match(/^(?:Enemies|Allies|Enemy heroes this match|Enemy heroes|Allied heroes|Enemy roles)[^\n]*$/gm) || []).join(' ');
  const named = heroRe.filter(([, re]) => re.test(state.replace(/^(?:RECENT|- \[)[^\n]*$/gm, ''))).map(([n]) => n);
  const lines = state.split('\n').filter(l => /^(?:Death at|Earlier Death|Damage by hero|Rule death review|Souls |GOOD PLAYS|- \[|Praise:)/.test(l) && !/rule:/.test(l)).slice(0, 8);
  console.log(`  DEATH PROMPT [${c.clock}] heroes named in STATE: ${named.join(', ') || 'none'}${roster ? '' : ' (no roster line)'}\n${lines.map(l => `      ${l.slice(0, 260)}`).join('\n')}`);
}
for (const d of delivered.filter(x => x.trigger === 'death')) console.log(`  DEATH SPOKEN [${d.clock}] ${d.text}`);
// --prompt m:ss[,m:ss]: the full user prompt of the requests at those clocks (death or periodic).
for (const at of (opt('--prompt') || '').split(',').filter(Boolean)) {
  const secs = (x: string) => { const [m, s] = x.split(':').map(Number); return Number.isFinite(m) && Number.isFinite(s) ? m * 60 + s : -1; };
  const c = calls.find(x => x.clock === at && !x.retry) ?? calls.find(x => secs(x.clock) >= secs(at) && !x.retry);
  console.log(`\n${'='.repeat(100)}\nPROMPT at ${at} → ${c ? `${c.trigger} request at ${c.clock}` : 'none'}\n${'='.repeat(100)}\n${c?.body ?? ''}`);
}

// Prompt size and one coach voice (docs/coach-review-13.md item 12-14): the live log vs this build.
{
  const avg = (xs: number[]) => xs.length ? Math.round(xs.reduce((a, b) => a + b, 0) / xs.length) : 0;
  const size = (trigger: string) => calls.filter(c => c.trigger === trigger && !c.retry).map(c => SYSTEM_PROMPT.length + c.body.length);
  if (livePrompts.length) {
    const b = (t: string) => livePrompts.filter(p => p.trigger === t).map(p => p.chars);
    console.log(`\nPROMPT SIZE (system + user, characters; ~4 per token)`);
    for (const t of ['periodic', 'death']) {
      const before = avg(b(t)), after = avg(size(t));
      console.log(`  ${t.padEnd(8)} before (live ai-log, ${b(t).length} calls) avg ${before.toLocaleString('en-US')} · after (${size(t).length} calls) avg ${after.toLocaleString('en-US')}${before ? ` · ${Math.round((1 - after / before) * 100)}% smaller` : ''}`);
    }
    const brief = livePrompts.filter(p => p.brief).map(p => p.brief);
    console.log(`  generic wiki block in the live prompts: ${brief.length} of ${livePrompts.length} calls, ~${avg(brief).toLocaleString('en-US')} chars each (now 0; its essentials are SYSTEM_PROMPT rule 9) · system prompt now ${SYSTEM_PROMPT.length.toLocaleString('en-US')} chars`);
  }
  // Before: live AI calls on a theme a rule call covered in the 30 s before (same rule as the gate now).
  const live = liveSpoken.filter(c => c.source !== 'ai' && c.key), ai = liveSpoken.filter(c => c.source === 'ai');
  const dup = ai.filter(a => { const th = topicOf(a.key?.split(':')[2] || '', a.text); return live.some(r => r.clock != null && a.clock != null && a.clock - r.clock >= 0 && a.clock - r.clock <= RULE_THEME_SECS && ruleThemes(r.key!).includes(th as never)); });
  const st = coach.status;
  console.log(`\nONE COACH VOICE`);
  if (ai.length) console.log(`  before (live): ${dup.length} of ${ai.length} AI calls on the theme of a rule call spoken in the 30 s before · AI death calls ${ai.filter(a => /^ai:death/.test(a.key || '')).length} · rule death reviews ${live.filter(r => /^death:review/.test(r.key!)).length}`);
  const gatedRule = skipped.filter(s => /as the rule call/.test(s.reason)).length, gatedDeath = skipped.filter(s => /rule death review/.test(s.reason)).length;
  console.log(`  after: ${gatedRule} answers gated as a rule-call theme (30 s) · ${st.ruleSkips || 0} event requests left to the rule coach · ${st.fightSkips || 0} fights without a periodic request · ${st.windowSkips || 0} windows too short to ask · ${st.fightDrops || 0} answers dropped for a fight`);
  console.log(`  deaths: ${coach.history.deathCount} deaths (flicker frames merged), ${calls.filter(c => c.trigger === 'death' && !c.retry).length} AI death requests, ${delivered.filter(d => d.trigger === 'death').length} spoken, ${gatedDeath} gated as covered by the rule death review, ${st.deathSkips || 0} second calls refused`);
  console.log(`  measured latency ~${coach.history.latency} s (fake model: instant; live default 6 s)${mergedFrom ? ` · merged enemy items / scoreboard rows from ${path.basename(mergedFrom)} into ${merged} frames` : ''}`);
}
if (!quiet) {
  for (const d of delivered) console.log(`  SPOKEN  [${d.clock}] ${d.trigger}/${d.topic}${d.why ? ` (why now: ${d.why.slice(0, 70)})` : ''}: ${d.text}`);
  for (const s of skipped) console.log(`  GATED   [${s.clock}] ${s.reason.replace(/^Skipped a repeat: /, '')}`);
  for (const [kind, s] of Object.entries(samples)) {
    console.log(`\n${'='.repeat(100)}\nSAMPLE PROMPT · ${kind} · ${s ? s.clock : 'not found in this recording'}\n${'='.repeat(100)}`);
    if (!s) continue;
    if (kind === 'event') console.log(`--- system (${SYSTEM_PROMPT.length} chars) ---\n${SYSTEM_PROMPT}\n--- user (${s.user.length} chars) ---`);
    console.log(s.user);
  }
}

// Optional: the real gateway on a handful of the generated periodic prompts (no keys printed).
const url = process.env.AI_EVAL_URL, model = process.env.AI_EVAL_MODEL;
if (url && model) {
  const cfg: AiConfig = { provider: process.env.AI_EVAL_PROVIDER === 'anthropic' ? 'anthropic' : 'openai', baseUrl: url, model, key: process.env.AI_EVAL_KEY || '', maxTokens: 1024, headers: '',
    reasoning: (['off', 'low', 'default'].includes(process.env.AI_EVAL_REASONING || '') ? process.env.AI_EVAL_REASONING : 'low') as AiReasoning };
  const n = Math.max(1, Number(process.env.AI_EVAL_N) || 5);
  const picks = periodic.filter(c => !/routine check/.test(c.why || '')).filter((_, i, a) => i % Math.max(1, Math.floor(a.length / n)) === 0).slice(0, n);
  console.log(`\nREAL GATEWAY · ${model} · reasoning ${cfg.reasoning} · ${picks.length} prompts`);
  for (const c of picks) {
    try {
      const r = await callGateway(cfg, SYSTEM_PROMPT, c.body, { timeoutMs: 45000, trigger: 'eval' });
      const p = parseAdvice(r.text);
      console.log(`  [${c.clock}] ${r.ms} ms · reasoning ${r.usage?.reasoning ?? '?'} · why now: ${(c.why || '').slice(0, 80)}\n    → ${p.ok ? (p.advice.speak ?? 'PASS') + ` (${p.advice.focus})` : `unparsed: ${r.text.slice(0, 160)}`}`);
    } catch (e) { console.log(`  [${c.clock}] error: ${(e as Error).message.slice(0, 200)}`); }
  }
}
void checkAdvice; void soulsView;
