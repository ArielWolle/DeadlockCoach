// Skills and mastery (src/mastery.ts, docs/player-profile.md "Skills and mastery").
import test from 'node:test';
import assert from 'node:assert/strict';
import { SkillMastery, SKILLS, MASTERY, MASTERY_KEY, skillOfKey, skillOfCue, skillOfAdvice, metricLevel, type LevelMetrics } from '../src/mastery.ts';
import { buildPrompt, setMasteryProvider, masteryText } from '../src/ai-coach.ts';
import { buildReviewPrompt } from '../src/ai-review.ts';
import type { Cue, Snapshot } from '../src/types.ts';

const memory = () => { const m = new Map<string, string>(); return { m, getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => { m.set(k, v); }, removeItem: (k: string) => { m.delete(k); } }; };
const cue = (key: string, priority: Cue['priority'] = 'Normal', text = 'Say it.', skill?: string): Cue => ({ key, text, priority, cooldown_secs: 0, ...(skill ? { skill } : {}) });
const alive = (c: number, souls = 500, extra: Partial<Snapshot> = {}): Snapshot => ({ game_clock_secs: c, screen: 'Gameplay', hp: 1900, max_hp: 2000, souls, ...extra });
const dead = (c: number, souls: number | null, respawn: number | null = 20): Snapshot => ({ game_clock_secs: c, screen: 'Dead', dead: true, souls, respawn_secs: respawn });
/** A tracker with a match in progress at `clock` (wall time follows the clock). */
function tracker(opts: { metrics?: LevelMetrics | null; store?: ReturnType<typeof memory> } = {}) {
  let now = 1_800_000_000_000;
  const m = new SkillMastery({ now: () => now, store: opts.store ?? null, metrics: () => opts.metrics ?? null });
  const feed = (s: Snapshot, cues: Cue[] = []) => { now = 1_800_000_000_000 + (s.game_clock_secs ?? 0) * 1000; return m.observe(s, now, cues); };
  return { m, feed, set: (t: number) => { now = t; } };
}
/** One death at `at` holding `souls`, spending `spend` on the death screen (frames every 3 s). */
function death(feed: (s: Snapshot) => unknown, at: number, souls: number, spend: number) {
  for (let c = at - 30; c < at; c += 3) feed(alive(c, souls));
  for (let c = at; c < at + 30; c += 3) feed(dead(c, c >= at + 9 ? souls - spend : souls, at + 30 - c));
  for (let c = at + 30; c < at + 45; c += 3) feed(alive(c, souls - spend, { minimap: { observations: 3, you: { x: .5, y: .95 }, zone_area: 'OwnBase' } as Snapshot['minimap'] }));
}
/** End the match in progress and start the next one (a clock back near 0). */
function nextMatch(t: ReturnType<typeof tracker>) { t.m.finishSession(); t.feed(alive(30)); }

test('skill taxonomy: 20-30 plain-language skills, every level, cue keys and AI advice mapped like the engine', () => {
  assert.ok(SKILLS.length >= 20 && SKILLS.length <= 30);
  for (const lv of ['basic', 'intermediate', 'advanced']) assert.ok(SKILLS.some(s => s.level === lv), lv);
  assert.equal(new Set(SKILLS.map(s => s.id)).size, SKILLS.length);
  for (const s of SKILLS) assert.ok(s.label && s.description && s.mastered_after.observations > 0 && s.mastered_after.window > 0, s.id);
  // Same answers as crates/engine/src/skills.rs keys_map_to_the_most_specific_skill.
  assert.equal(skillOfKey('unspent-dead'), 'shop_on_death');
  assert.equal(skillOfKey('unspent:base'), 'shop_in_base');
  assert.equal(skillOfKey('unspent'), 'dont_die_holding_souls');
  assert.equal(skillOfKey('plan:urn_contest:420'), 'urn_play');
  assert.equal(skillOfKey('plan:guardian_weak'), 'objective_timing');
  assert.equal(skillOfKey('praise:lead:3'), 'throw_awareness');
  assert.equal(skillOfKey('review:pattern:3'), null);
  // The engine's tag wins; older cues fall back to the key; AI advice by text, then focus.
  assert.equal(skillOfCue(cue('active:cc', 'High', 'x', 'escape_after_cc')), 'escape_after_cc');
  assert.equal(skillOfCue(cue('active:cc', 'High', 'Knocked off the zipline: get to cover.')), 'zipline_discipline');
  assert.equal(skillOfCue(cue('player:focus:goal:unspent_souls')), 'dont_die_holding_souls');
  assert.equal(skillOfAdvice('ai:death:economy', 'You are dead with 6,000 souls: buy Plated Armor before you respawn.'), 'shop_on_death');
  assert.equal(skillOfAdvice('ai:periodic:objective', 'Group mid for the Midboss in 30 seconds.'), 'objective_timing');
  assert.equal(skillOfAdvice('ai:periodic:healing', 'Stay with Abrams.'), 'heal_before_leaving');
});

test('evidence: buying on the death screen without a reminder is an unprompted success; after the reminder it is prompted', () => {
  const t = tracker();
  t.feed(alive(1200, 4000));
  death(t.feed, 1230, 4000, 3200);
  const ok = t.m.events.find(e => e.skill === 'shop_on_death');
  assert.equal(ok?.kind, 'success'); assert.equal(ok?.prompted, false);
  // The same with the death-screen reminder spoken before the souls fell: prompted.
  const u = tracker();
  u.feed(alive(1200, 4000));
  for (let c = 1200; c < 1230; c += 3) u.feed(alive(c, 4000));
  u.feed(dead(1230, 4000, 30)); u.m.noteSpoken(cue('unspent-dead', 'High', 'You are dead with 4,000 souls. Buy now.'), 1233);
  for (let c = 1236; c < 1260; c += 3) u.feed(dead(c, c >= 1239 ? 800 : 4000, 1260 - c));
  for (let c = 1260; c < 1275; c += 3) u.feed(alive(c, 800));
  const pr = u.m.events.find(e => e.skill === 'shop_on_death');
  assert.equal(pr?.kind, 'success'); assert.equal(pr?.prompted, true);
  // Respawning with the usual buy unspent is a failure.
  const f = tracker(); f.feed(alive(1300, 7000)); death(f.feed, 1330, 7000, 0);
  assert.equal(f.m.events.find(e => e.skill === 'shop_on_death')?.kind, 'failure');
  assert.ok(f.m.events.some(e => e.skill === 'dont_die_holding_souls' && e.kind === 'failure' && /7,000 souls/.test(e.detail)));
});

test('evidence: base visits (spending, healing), a teammate on the way out and the engine cues', () => {
  const t = tracker();
  const base = { observations: 3, you: { x: .5, y: .95 }, zone_area: 'OwnBase' } as Snapshot['minimap'];
  const lane = { observations: 3, you: { x: .5, y: .6 }, zone_area: 'Lane', heroes: [{ id: 1, ally: true, pos: { x: .52, y: .62 }, unseen_ms: 0, hits: 4 }] } as Snapshot['minimap'];
  for (let c = 900; c < 930; c += 3) t.feed(alive(c, 3500, { hp: 800, minimap: lane }));
  // Back in base hurt with 3,500 souls: buys (two reads under the peak), heals to full, walks out.
  t.feed(alive(930, 3500, { hp: 900, minimap: base }));
  t.feed(alive(933, 400, { hp: 1500, minimap: base }));
  t.feed(alive(936, 420, { hp: 2000, minimap: base }));
  t.feed(alive(939, 430, { hp: 2000, minimap: lane })); t.feed(alive(942, 430, { hp: 2000, minimap: lane }));
  const kinds = (skill: string) => t.m.events.filter(e => e.skill === skill).map(e => `${e.kind}${e.prompted ? ':prompted' : ''}`);
  assert.deepEqual(kinds('shop_in_base'), ['success']);
  assert.deepEqual(kinds('heal_before_leaving'), ['success']);
  // Walking out of base with the usual buy in your pocket is a failure; leaving at 40% too.
  t.feed(alive(1000, 7000, { hp: 800, minimap: base })); t.feed(alive(1003, 7000, { hp: 800, minimap: base }));
  t.feed(alive(1006, 7000, { hp: 800, minimap: lane })); t.feed(alive(1009, 7000, { hp: 800, minimap: lane }));
  assert.deepEqual(kinds('shop_in_base'), ['success', 'failure']);
  assert.deepEqual(kinds('heal_before_leaving'), ['success', 'failure']);
  // Praise is a success the engine saw; "alone after a respawn" is a failure; the same offer from both paths counts once.
  t.m.offered(cue('praise:group:2'), 1100); t.m.offered(cue('praise:group:2'), 1102);
  t.m.offered(cue('review:respawn-alone'), 1200);
  assert.deepEqual(kinds('walk_out_with_team'), ['success', 'failure']);
});

test('mastery: 4 unprompted successes in the last 3 matches with at most 1 failure; unmastered after repeated failures', () => {
  const t = tracker(); t.feed(alive(100));
  const def = SKILLS.find(s => s.id === 'shop_in_base')!;
  assert.deepEqual([def.mastered_after.observations, def.mastered_after.window, def.mastered_after.max_failures], [4, 3, 1]);
  t.m.add('shop_in_base', 'success', 100, 'x'); t.m.add('shop_in_base', 'success', 200, 'x', true); // prompted: not counted
  nextMatch(t); t.m.add('shop_in_base', 'success', 100, 'x'); t.m.add('shop_in_base', 'failure', 150, 'x');
  t.m.add('shop_in_base', 'success', 200, 'x');
  assert.equal(t.m.data.skills.shop_in_base.mastered, false);
  nextMatch(t); t.m.add('shop_in_base', 'success', 100, 'x');
  assert.equal(t.m.data.skills.shop_in_base.mastered, true);
  assert.ok(t.m.events.some(e => e.kind === 'mastered' && e.skill === 'shop_in_base'));
  // Matches older than the window do not count: 3 new matches, only 3 successes.
  const u = tracker(); u.feed(alive(100));
  for (let i = 0; i < 3; i++) { u.m.add('last_hits', 'success', 100, 'x'); nextMatch(u); }
  nextMatch(u); u.m.add('last_hits', 'success', 100, 'x');
  assert.equal(u.m.window('last_hits').ok, 2); assert.equal(u.m.data.skills.last_hits.mastered, false);
  // Two failures in one match unmaster it.
  nextMatch(t); t.m.add('shop_in_base', 'failure', 100, 'x');
  assert.equal(t.m.data.skills.shop_in_base.mastered, true);
  t.m.add('shop_in_base', 'failure', 200, 'x');
  assert.equal(t.m.data.skills.shop_in_base.mastered, false);
  assert.equal(MASTERY.match_unmaster_failures, 2);
});

test('suppression: mastered skills stay quiet, High safety calls pass, a failure this match earns exactly one reminder', () => {
  const t = tracker(); t.feed(alive(100));
  for (const s of ['shop_on_death', 'shop_in_base', 'back_off_low_hp', 'dont_die_holding_souls']) t.m.setManual(s, 'mastered');
  assert.equal(t.m.gate(cue('unspent:base'), 300).speak, false);
  assert.equal(t.m.gate(cue('praise:spent_dead:1'), 300).speak, false); // no praise for the obvious either
  // The death-screen buy reminder is High but not a safety call (MASTERY.quiet_high).
  assert.equal(t.m.gate(cue('unspent-dead', 'High'), 300).speak, false);
  assert.equal(t.m.gate(cue('lane:critical', 'High'), 300).speak, true);
  assert.equal(t.m.gate(cue('lane:back_off', 'Normal'), 300).speak, false);
  // The death review keeps its lesson but drops the "Spend N souls now." tail.
  const g = t.m.gate(cue('death:review:900', 'High', 'You died because you were alone. Next life: walk with Abrams. Spend 3,200 souls now.', 'walk_out_with_team'), 905);
  assert.equal(g.speak, true); assert.equal(g.text, 'You died because you were alone. Next life: walk with Abrams.');
  // Died holding 6,241 souls (rec13 19:27): one shop reminder (of any shop skill), then quiet again.
  t.m.add('dont_die_holding_souls', 'failure', 1167, 'died holding 6,241 souls');
  const first = t.m.gate(cue('unspent-dead', 'High', 'You are dead with 6,241 souls. Buy now.'), 1170);
  assert.equal(first.speak, true); assert.match(first.reason, /one reminder/);
  t.m.noteSpoken(cue('unspent-dead', 'High'), 1170);
  assert.equal(t.m.gate(cue('unspent:base'), 1218).speak, false);
  assert.equal(t.m.gate(cue('unspent'), 1300).speak, false);
});

test('suppression: a skill in progress gets one reminder per match after an unprompted success; unseen skills are coached', () => {
  const t = tracker(); t.feed(alive(100));
  assert.equal(t.m.state('urn_play'), 'unseen');
  assert.equal(t.m.gate(cue('map:urn-ground'), 200).speak, true);
  t.m.add('urn_play', 'success', 300, 'Urn delivered');
  assert.equal(t.m.state('urn_play'), 'working');
  assert.equal(t.m.gate(cue('map:urn-ground'), 400).speak, true); t.m.noteSpoken(cue('map:urn-ground'), 400);
  const g = t.m.gate(cue('map:urn-ground'), 600);
  assert.equal(g.speak, false); assert.match(g.reason, /one reminder per match/);
  // A failure brings the reminder back once.
  t.m.add('urn_play', 'failure', 650, 'died carrying the Urn');
  assert.equal(t.m.gate(cue('map:urn-ground'), 700).speak, true);
});

test('level: from mastered skills and profile metrics; basics are assumed at intermediate+ unless failed', () => {
  assert.equal(metricLevel(null), null);
  assert.equal(metricLevel({ deadPct: 25, deathsPer10: 4, kda: 1.5, matches: 3 }), 'basic');
  assert.equal(metricLevel({ deadPct: 14, deathsPer10: 1.6, kda: 2.6, matches: 3 }), 'intermediate');
  assert.equal(metricLevel({ deadPct: 14, deathsPer10: 1.6, kda: 4.9, matches: 3 }), 'advanced');
  assert.equal(metricLevel({ deadPct: 8, deathsPer10: 1.2, kda: 0.8, matches: 3 }), 'intermediate');
  const t = tracker({ metrics: { deadPct: 14, deathsPer10: 1.6, kda: 2.6, matches: 3 } }); t.feed(alive(100));
  assert.equal(t.m.level().level, 'intermediate');
  assert.equal(t.m.state('secure_souls'), 'assumed');
  assert.equal(t.m.gate(cue('active:unsecured'), 200).speak, false);
  assert.equal(t.m.state('urn_play'), 'unseen'); // intermediate skills are not assumed
  t.m.add('secure_souls', 'failure', 300, 'died with unsecured souls');
  assert.equal(t.m.state('secure_souls'), 'working');
  const basic = tracker({ metrics: { deadPct: 30, deathsPer10: 5, kda: 1, matches: 3 } }); basic.feed(alive(100));
  assert.equal(basic.m.level().level, 'basic'); assert.equal(basic.m.state('secure_souls'), 'unseen');
  // Manual marks win over the evidence.
  t.m.setManual('secure_souls', 'mastered'); assert.equal(t.m.state('secure_souls'), 'mastered');
  t.m.setManual('heal_before_leaving', 'learning'); assert.equal(t.m.gate(cue('active:heal'), 400).speak, true);
});

test('persistence: versioned localStorage store, export / import merge with the profile export, reset; newer schemas are kept untouched', () => {
  const store = memory();
  const t = tracker({ store }); t.feed(alive(100));
  t.m.add('shop_on_death', 'success', 100, 'x'); t.m.setManual('urn_play', 'mastered');
  const saved = JSON.parse(store.m.get(MASTERY_KEY)!);
  assert.equal(saved.schema, 1); assert.equal(saved.skills.shop_on_death.performed, 1); assert.equal(saved.skills.urn_play.manual, 'mastered');
  const again = new SkillMastery({ store }).load();
  assert.equal(again.data.skills.shop_on_death.performed, 1); assert.equal(again.state('urn_play'), 'mastered');
  // Import from a player-profile export (mastery under "mastery"): new match histories are merged once.
  const other = new SkillMastery();
  assert.equal(other.importJSON(JSON.stringify({ schema: 1, matches: [], mastery: again.toJSON() })), 2);
  assert.equal(other.importJSON(JSON.stringify({ schema: 1, matches: [], mastery: again.toJSON() })), 0);
  assert.equal(other.data.skills.shop_on_death.performed, 1);
  assert.equal(other.importJSON(JSON.stringify({ schema: 1, matches: [] })), 0); // a profile export without mastery
  // Unknown skills are dropped; a newer schema throws on import and leaves the stored data untouched on load.
  assert.equal(new SkillMastery().importJSON(JSON.stringify({ schema: 1, skills: { not_a_skill: { performed: 3 } } })), 0);
  assert.throws(() => new SkillMastery().importJSON(JSON.stringify({ schema: 9, skills: {} })), /newer/);
  const newer = memory(); newer.setItem(MASTERY_KEY, JSON.stringify({ schema: 9, skills: {} }));
  const n = new SkillMastery({ store: newer }).load(); n.setManual('urn_play', 'mastered');
  assert.equal(JSON.parse(newer.m.get(MASTERY_KEY)!).schema, 9);
  again.reset(); assert.equal(store.m.get(MASTERY_KEY), undefined); assert.equal(again.state('urn_play'), 'unseen');
});

test('AI prompts: the level and mastered skills reach the live and the end-of-game prompts', () => {
  const t = tracker({ metrics: { deadPct: 14, deathsPer10: 1.6, kda: 2.6, matches: 3 } }); t.feed(alive(100));
  t.m.setManual('shop_on_death', 'mastered'); t.m.add('dont_die_holding_souls', 'failure', 1167, 'x');
  const text = t.m.promptText()!;
  assert.match(text, /^Level: Intermediate/);
  assert.match(text, /Mastered, do not coach unless failed now: [^.]*Spending souls on the death screen/);
  assert.match(text, /Spending souls before they pile up \(failed this match\)/);
  assert.ok(text.length <= 600);
  setMasteryProvider(() => text);
  try {
    assert.equal(masteryText(), text);
    const p = buildPrompt({ trigger: 'periodic', summary: 'STATE x', level: masteryText() });
    assert.match(p.user, /PLAYER LEVEL \(do not coach mastered skills[^\n]*\)\nLevel: Intermediate/);
    assert.match(buildReviewPrompt('MATCH DIGEST').user, /PLAYER LEVEL[^\n]*\nLevel: Intermediate/);
  } finally { setMasteryProvider(undefined); }
  assert.doesNotMatch(buildReviewPrompt('MATCH DIGEST').user, /PLAYER LEVEL/);
  assert.doesNotMatch(buildPrompt({ trigger: 'periodic', summary: 'STATE x' }).user, /PLAYER LEVEL/);
});

// Live recordings (read-only, outside the repository: skipped when absent). docs/player-profile.md
// "Skills and mastery" lists the numbers.
const REC = process.env.DEADLOCK_FRAMES || 'C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
const recs = ['rec8', 'rec11', 'rec12', 'rec13'].map(r => `${REC}/${r}/recording.json`);
test('recordings rec8 -> rec13: shopping is mastered by rec12-13, the 19:27 death holding 6,241 souls gets one reminder', async t => {
  const { existsSync } = await import('node:fs');
  if (!recs.every(f => existsSync(f))) { t.skip('recordings not found'); return; }
  const { validate } = await import('../scripts/mastery-validate.ts');
  const [, rec11, rec12, rec13] = validate(recs, false, false);
  for (const s of ['shop_on_death', 'shop_in_base']) assert.ok(rec11.mastered.includes(s), s);
  const shop = (r: typeof rec12) => r.allowed.filter(a => /^unspent/.test(a.key));
  assert.deepEqual(shop(rec12), []);
  assert.deepEqual(shop(rec13).map(a => [a.clock, a.key, a.reason]), [[1218, 'unspent:base', 'failed this match: one reminder']]);
  assert.ok(rec13.suppressedCues.some(a => a.key === 'unspent-dead' && a.clock === 2598));
  assert.ok(rec13.events.some(e => e.skill === 'dont_die_holding_souls' && e.kind === 'failure' && /6,241 souls at 19:30/.test(e.detail)));
  assert.ok(rec12.suppressed >= 15 && rec13.suppressed >= 15, `${rec12.suppressed} / ${rec13.suppressed}`);
});
