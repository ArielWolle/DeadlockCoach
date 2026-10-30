// Start-of-game pep talk (src/pregame.ts; docs/ai-coach.md "Start-of-game pep talk").
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PREGAME, PregameStore, PregameWatch, buildPregameDigest, buildPregamePrompt, foreignHeroes, parsePregame, requestPregame, sanitizeBriefing, storedBriefing, templateBriefing, threatData, threatsFor, type PregameEvent, type Roster } from '../src/pregame.ts';
import { sanitizeMatch, type MatchRecord } from '../src/player-profile.ts';
import type { AiConfig } from '../src/ai-coach.ts';
import type { Snapshot } from '../src/types.ts';

const data = (p: string) => JSON.parse(readFileSync(new URL(`../../data/${p}`, import.meta.url), 'utf8').replace(/^\uFEFF/, ''));
const threats = threatData(data('coaching/plans.json'), data('coaching/review.json'));
const OLD_E = ['hero_ghost', 'hero_baba', 'hero_nurse', 'hero_dynamo', 'hero_nano', 'hero_necro'], OLD_A = ['hero_werewolf', 'hero_ratking', 'hero_artist', 'hero_bookworm', 'hero_fencer', 'hero_unicorn'];
const NEW_E = ['hero_astro', 'hero_drifter', 'hero_inferno', 'hero_dynamo', 'hero_atlas', 'hero_necro'], NEW_A = ['hero_werewolf', 'hero_warden', 'hero_mirage', 'hero_doorman', 'hero_orion', 'hero_nano'];
const frame = (clock: number | null, extra: Partial<Snapshot> = {}): Snapshot => ({ game_clock_secs: clock, screen: 'Gameplay', ...extra });
/** A match record (sanitizeMatch fills the rest). */
function match(o: { id?: string; hero?: string; span?: number; dead?: number; deaths?: Array<{ clock: number; killer?: string; souls?: number }>; result?: 'win' | 'loss'; startedAt?: number; windows?: [number, number] }): MatchRecord {
  const span = o.span ?? 1800;
  return sanitizeMatch({ id: o.id ?? 'm1', hero: o.hero ?? 'hero_orion', startedAt: o.startedAt ?? 1000, endedAt: (o.startedAt ?? 1000) + span * 1000, startClock: 0, endClock: span, observedSecs: span,
    spanSecs: span, deadSecs: o.dead ?? 60, result: o.result ?? null, time: { lane: span * 0.6, jungle: span * 0.3, base: span * 0.05, dead: o.dead ?? 60, unknown: 0 },
    deaths: (o.deaths ?? []).map(d => ({ clock: d.clock, killer: d.killer ?? null, souls: d.souls ?? 500, deadSecs: 20, evidence: true })),
    windows: { used: o.windows?.[0] ?? 0, unused: o.windows?.[1] ?? 0, unusedAt: [] } })!;
}
function feed(w: PregameWatch, frames: Array<[number, Snapshot]>): PregameEvent[] {
  const out: PregameEvent[] = []; for (const [t, s] of frames) { const e = w.frame(s, t); if (e) out.push(e); } return out;
}

test('pregame watch: once per match after the roster latched; the previous match\'s roster held by the vision (rec12 0:31-0:42) does not count', () => {
  const w = new PregameWatch();
  // The end of the previous match (rec11), then the countdown, then the new match with the stale roster first.
  const frames: Array<[number, Snapshot]> = [];
  for (let c = 2300; c <= 2340; c += 4) frames.push([c * 1000, frame(c, { enemy_heroes: OLD_E, ally_heroes: OLD_A, hero: 'hero_artist' })]);
  frames.push([2400_000, { screen: 'Countdown' }], [2410_000, { screen: 'Unknown' }]);
  let t = 2420_000;
  for (const c of [31, 34, 37, 42]) frames.push([t += 3000, frame(c, { enemy_heroes: OLD_E, ally_heroes: OLD_A, hero: 'hero_artist' })]);
  const before = feed(w, frames);
  assert.equal(before.length, 1); assert.equal(before[0].type, 'skipped'); // rec11 was first seen at 38:20
  assert.ok(w.waiting, 'the new match waits for a fresh roster');
  const next: Array<[number, Snapshot]> = [];
  for (const c of [45, 47, 50, 52, 57, 60]) next.push([t += 3000, frame(c, { enemy_heroes: NEW_E, ally_heroes: NEW_A, hero: 'hero_orion' })]);
  const ev = feed(w, next);
  assert.equal(ev.length, 1, 'one event per match');
  assert.equal(ev[0].type, 'ready');
  if (ev[0].type !== 'ready') return;
  assert.equal(ev[0].clock, 50); assert.equal(ev[0].start.clock, 31); assert.match(ev[0].start.key, /^pregame:/);
  assert.deepEqual([...ev[0].roster.enemies].sort(), [...NEW_E].sort()); assert.equal(ev[0].roster.hero, 'hero_orion'); assert.ok(ev[0].roster.heroSure);
  assert.ok(!ev[0].roster.enemies.includes('hero_ghost'));
  // A mid-match loading screen (rec12 9:36) and later frames: no second pep talk.
  assert.deepEqual(feed(w, [[t += 3000, { screen: 'Loading' }], [t += 3000, frame(70, { enemy_heroes: NEW_E })], [t += 3000, frame(600, { enemy_heroes: NEW_E })]]), []);
});
test('pregame watch: a match joined late (rec13 at 5:52) is skipped; the deadline speaks without a roster; a missed window is skipped', () => {
  const late = feed(new PregameWatch(), [[1000, frame(352, { enemy_heroes: NEW_E })], [4000, frame(355, { enemy_heroes: NEW_E })]]);
  assert.deepEqual(late.map(e => [e.type, e.type === 'skipped' && e.reason]), [['skipped', 'late_start']]);
  // No roster read at all: the briefing comes at the 1:30 deadline (then without matchup lines).
  const w = new PregameWatch(); const fs: Array<[number, Snapshot]> = [];
  for (let c = 5; c <= 100; c += 5) fs.push([c * 1000, frame(c)]);
  const ev = feed(w, fs);
  assert.equal(ev.length, 1); assert.equal(ev[0].type, 'ready'); assert.equal(ev[0].clock, PREGAME.deadline_secs);
  if (ev[0].type === 'ready') assert.deepEqual(ev[0].roster.enemies, []);
  // Frames resume only after 2:30 (e.g. alt-tabbed during the countdown): skipped.
  const m = feed(new PregameWatch(), [[1000, frame(10)], [200_000, frame(160)]]);
  assert.deepEqual(m.map(e => e.type === 'skipped' ? e.reason : e.type), ['missed_window']);
  // A new match (the clock reset) gets its own pep talk.
  const again = new PregameWatch();
  const a = feed(again, [[1000, frame(40, { enemy_heroes: NEW_E, hero: 'hero_orion' })], [4000, frame(43, { enemy_heroes: NEW_E, hero: 'hero_orion' })], [7000, frame(46, { enemy_heroes: NEW_E, hero: 'hero_orion' })],
    [8000, frame(1500, { enemy_heroes: NEW_E })], [9000_000, frame(20, { enemy_heroes: OLD_E })], [9003_000, frame(36, { enemy_heroes: OLD_E })], [9006_000, frame(39, { enemy_heroes: OLD_E })], [9009_000, frame(42, { enemy_heroes: OLD_E })]]);
  assert.deepEqual(a.map(e => e.type), ['ready', 'ready']); assert.notEqual(a[0].start.key, a[1].start.key);
  // A hero read that disagrees with the roster is not named.
  const odd = feed(new PregameWatch(), [40, 43, 46].map(c => [c * 1000, frame(c, { enemy_heroes: NEW_E, ally_heroes: NEW_A, hero: 'hero_drifter' })] as [number, Snapshot]));
  assert.equal(odd[0].type === 'ready' && odd[0].roster.hero, null);
});

const roster: Roster = { hero: 'hero_orion', heroSure: true, allies: NEW_A, enemies: NEW_E };
test('threat table: built from plans.json / review.json; long-range heroes fear the divers; past killers rank first', () => {
  assert.ok(threats.heroes.hero_atlas && threats.heroes.hero_drifter); assert.ok(threats.longRange.has('hero_orion'));
  assert.equal(threats.counterItems.cc.item, 'Reactive Barrier');
  // Every counter item named exists in data/items.json.
  const items = new Set((data('items.json') as Array<{ name: string }>).map(i => i.name));
  for (const c of Object.values(threats.counterItems)) assert.ok(items.has(c.item), c.item);
  const name = (id: string) => ({ hero_drifter: 'Drifter', hero_atlas: 'Abrams', hero_inferno: 'Infernus', hero_necro: 'Graves' } as Record<string, string>)[id] || id;
  const t = threatsFor(NEW_E, 'hero_orion', [], threats, name);
  assert.deepEqual(t.map(x => x.hero), ['hero_drifter', 'hero_atlas']);
  assert.equal(t[1].counter, threats.heroes.hero_atlas.counter.long_range); // Grey Talon: "Snare at your feet"
  assert.equal(t[0].item, 'Dispel Magic'); assert.match(t[0].itemWhy!, /Stalker's Mark/);
  // Graves killed you three times in the last matches: Graves leads.
  const prev = [match({ deaths: [{ clock: 300, killer: 'hero_necro' }, { clock: 900, killer: 'hero_necro' }] }), match({ id: 'm2', startedAt: 0, deaths: [{ clock: 400, killer: 'hero_necro' }] })];
  const k = threatsFor(NEW_E, 'hero_orion', prev, threats, name);
  assert.equal(k[0].hero, 'hero_necro'); assert.equal(k[0].killedYou, 3);
  assert.deepEqual(threatsFor([], 'hero_orion', prev, threats, name), []);
});

test('digest + template: honest after a rough game, praise first after an improved one, goals with concrete triggers, at most 120 words', () => {
  const rough = [match({ hero: 'hero_artist', span: 1980, dead: 340, deaths: Array.from({ length: 7 }, (_, i) => ({ clock: 300 + i * 200, souls: 3000 })) })];
  const d = buildPregameDigest({ key: 'pregame:1:31', clock: 50, roster, previous: rough, goals: [{ id: 'unused_windows', text: 'Push every enemy-dead window' }, { id: 'unspent_souls', text: 'Spend at the usual buy' }], threats });
  assert.equal(d.last?.rough, true); assert.equal(d.last?.deaths, 7); assert.equal(d.role, 'long_range');
  assert.match(d.text, /Your hero: Grey Talon/); assert.match(d.text, /LAST GAME\n.*7 deaths, dead 17%/); assert.match(d.text, /FOCUS GOALS THIS MATCH/); assert.match(d.text, /THREATS TO YOU[^\n]*\n- Drifter/);
  const b = templateBriefing(d);
  const n = b.spoken.split(/\s+/).length;
  assert.ok(n <= 120 && n >= 50, `${n} words: ${b.spoken}`);
  assert.match(b.spoken, /^Honest look at last game: 7 deaths/);
  assert.match(b.spoken, /when two enemies are dead, we push the nearest tower together\. Don't farm\./);
  assert.match(b.spoken, /Drifter/); assert.match(b.spoken, /(let's|Let's)[^.]*\.$/);
  assert.ok(b.spoken.indexOf('Focus one') < b.spoken.indexOf('Drifter'), 'goals before the matchup');
  assert.equal(b.matchup_tips.length, 2); assert.ok(b.focus.length > 10); assert.match(b.plan, /max range/);
  assert.equal(templateBriefing(d).spoken, b.spoken, 'deterministic');
  // Improved: the specific positive first.
  const before = [match({ id: 'new', startedAt: 3000, span: 1800, dead: 120, deaths: [{ clock: 600 }, { clock: 1200 }] }), match({ id: 'old1', startedAt: 2000, dead: 400, deaths: Array.from({ length: 8 }, (_, i) => ({ clock: 200 + i * 200 })) }),
    match({ id: 'old2', startedAt: 1000, dead: 380, deaths: Array.from({ length: 7 }, (_, i) => ({ clock: 200 + i * 200 })) })];
  const good = templateBriefing(buildPregameDigest({ key: 'pregame:2:20', clock: 40, roster, previous: before, goals: [], threats }));
  assert.match(good.spoken, /^Last game was a real step up: you were dead 7% of the time, down from 2\d%/); assert.match(good.spoken, /Same discipline as last game/);
  // First match, hero not read, no roster: a short generic briefing that names no hero.
  const blank = buildPregameDigest({ key: 'pregame:3:10', clock: 90, roster: { hero: null, heroSure: false, allies: [], enemies: [] }, previous: [], goals: [], threats });
  assert.match(blank.text, /Your hero: not read yet/); assert.match(blank.text, /none on the profile yet/);
  const bb = templateBriefing(blank); assert.match(bb.spoken, /^First coached match/); assert.equal(bb.matchup_tips.length, 0);
  // A tight budget drops the item, then the second threat, keeping goals and the close.
  const tight = templateBriefing(d, 70);
  assert.ok(tight.spoken.split(/\s+/).length <= 70, tight.spoken); assert.doesNotMatch(tight.spoken, /counter item/); assert.match(tight.spoken, /Focus one/);
});

test('AI prompt and strict-JSON reply: fenced, cut off, not JSON, heroes of another match', () => {
  const d = buildPregameDigest({ key: 'pregame:1:31', clock: 50, roster, previous: [], goals: [{ id: 'close_range', text: 'No close-range deaths' }], threats });
  const p = buildPregamePrompt(d, 1024);
  assert.match(p.system, /start-of-game pep talk/); assert.match(p.system, /"matchup_tips"/); assert.match(p.system, /at most 110 words/);
  assert.match(p.user, /PRE-GAME DIGEST/); assert.ok(p.chars < 6000); assert.equal(p.maxTokens, 1024);
  const reply = { spoken: 'Fresh start. Our focus: when a diver jumps in, step back behind your front line. Watch Drifter, stay next to a teammate. Let\'s win this.', focus: 'No close-range deaths.', matchup_tips: ['Drifter: stay next to a teammate'], plan: 'Farm Green safely.' };
  const ok = parsePregame('```json\n' + JSON.stringify(reply) + '\n```', [...NEW_A, ...NEW_E]);
  assert.ok(ok.ok); if (ok.ok) { assert.equal(ok.briefing.spoken, reply.spoken); assert.deepEqual(ok.briefing.matchup_tips, reply.matchup_tips); assert.equal(ok.partial, false); }
  const cut = parsePregame('{"spoken": "Fresh start today. When a diver jumps in, step back behind your front line and shoot.", "focus": "No close', null);
  assert.ok(cut.ok); if (cut.ok) assert.equal(cut.partial, true);
  assert.equal(parsePregame('Good luck out there!').ok, false);
  assert.equal(parsePregame('{"spoken": "Go."}').ok, false);
  const foreign = parsePregame(JSON.stringify({ ...reply, spoken: reply.spoken + ' And watch Victor\'s aura.' }), [...NEW_A, ...NEW_E]);
  assert.equal(foreign.ok, false); if (!foreign.ok) assert.match(foreign.error, /Victor/);
  assert.deepEqual(foreignHeroes('Seven souls and a Drifter', NEW_E), []);
  // Long replies are cut to the word budget at a sentence end.
  const long = parsePregame(JSON.stringify({ spoken: Array.from({ length: 40 }, (_, i) => `Sentence number ${i} is here.`).join(' ') }));
  assert.ok(long.ok && long.briefing.spoken.split(/\s+/).length <= PREGAME.max_words);
});

test('requestPregame: gateway reply parsed; failures throw for the template fallback; the last briefing is stored', async () => {
  const cfg: AiConfig = { provider: 'openai', baseUrl: 'https://gateway.test/v1', model: 'm', key: 'k', maxTokens: 1024, headers: '', reasoning: 'off' };
  const d = buildPregameDigest({ key: 'pregame:9:30', clock: 45, roster, previous: [], goals: [], threats });
  const body = (content: string) => new Response(JSON.stringify({ model: 'mock', choices: [{ message: { content }, finish_reason: 'stop' }] }), { status: 200 });
  const r = await requestPregame(cfg, buildPregamePrompt(d), [...NEW_A, ...NEW_E], { fetchImpl: async () => body(JSON.stringify({ spoken: 'Let us go. Watch Drifter and stay with a teammate today.', focus: 'f', matchup_tips: [], plan: 'p' })) });
  assert.equal(r.model, 'mock'); assert.match(r.briefing.spoken, /^Let us go/);
  await assert.rejects(requestPregame(cfg, buildPregamePrompt(d), null, { fetchImpl: async () => new Response('nope', { status: 500 }) }), /HTTP 500/);
  await assert.rejects(requestPregame(cfg, buildPregamePrompt(d), null, { fetchImpl: async () => body('Good luck!') }), /not the requested JSON/);
  const mem = new Map<string, string>(); const kv = { getItem: (k: string) => mem.get(k) ?? null, setItem: (k: string, v: string) => { mem.set(k, v); }, removeItem: (k: string) => { mem.delete(k); } };
  const st = new PregameStore(kv); st.put(storedBriefing(d, r.briefing, 'ai', { model: 'mock', now: 5 }));
  const again = new PregameStore(kv);
  assert.equal(again.last?.key, 'pregame:9:30'); assert.equal(again.last?.source, 'ai'); assert.equal(again.last?.spoken, r.briefing.spoken); assert.deepEqual(again.last?.enemies, NEW_E);
  assert.equal(sanitizeBriefing({ key: 'other:1', spoken: 'x' }), null);
  mem.set('deadlock-coach-pregame-v1', '{corrupt'); assert.equal(new PregameStore(kv).last, null);
});
