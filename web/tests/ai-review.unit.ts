// End-of-game AI review (src/ai-review.ts): digest, prompt, parsing, storage. Imported from unit.test.ts.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { DIGEST_MAX_CHARS, MAX_REVIEWS, REVIEW_SYSTEM_PROMPT, ReviewStore, ReviewTracker, autoReviewable, buildDigest, buildReviewPrompt, closeJson, lastMatchOf, parseReview, repairJson,
  requestReview, reviewKey, reviewMaxTokens, sentencesOf, speakable, storedReview, type EndedMatch } from '../src/ai-review.ts';
import { clearAiExchanges, type AiConfig } from '../src/ai-coach.ts';
import type { Cue, Snapshot } from '../src/types.ts';

const memory = () => { const m = new Map<string, string>(); return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => { m.set(k, v); }, removeItem: (k: string) => { m.delete(k); }, m }; };
const status12 = (deadEnemies: number) => Array.from({ length: 12 }, (_, i) => ({ ally: i < 6, slot: i % 6, dead: i >= 6 && i - 6 < deadEnemies, networth_observations: 1, observations: 1, position_observations: 0 }));
/** A synthetic match: gameplay every 5 s from `from` to `to`, deaths at `deaths` (25 s each), then optionally a PostGame screen. */
function match(hero: string, from: number, to: number, deaths: number[], opts: { postGame?: boolean; souls?: number; day?: number } = {}) {
  const base = 1_800_000_000_000 + (opts.day ?? 0) * 86_400_000;
  const frames: Array<{ snapshot: Snapshot; cues: Cue[]; wall: number }> = [];
  for (let t = from; t <= to; t += 5) {
    const d = deaths.find(x => t >= x && t < x + 25);
    const s: Snapshot = { game_clock_secs: t, hero, screen: d != null ? 'Dead' : 'Gameplay', dead: d != null, souls: opts.souls ?? 900, hp: d != null ? 0 : t % 60 < 30 ? 900 : 400, max_hp: 1000,
      ally_networth: 1000 + t * 40, enemy_networth: 1000 + t * 36, own_networth: 200 + t * 7, enemy_heroes: ['hero_drifter', 'hero_inferno'], ally_heroes: ['hero_orion'],
      hero_status: status12(t > 600 && t < 640 ? 2 : 0) as Snapshot['hero_status'], kills: Math.floor(t / 200), deaths: deaths.filter(x => x < t).length, assists: 3,
      minimap: { observations: 1, green: { ally_troopers: 0, enemy_troopers: 0 }, blue: { ally_troopers: 0, enemy_troopers: 0 }, yellow: { ally_troopers: 0, enemy_troopers: 0 }, midboss_up: true, enemies: [], allies: [], enemy_count: 0, you: { x: 0.5, y: 0.7 }, your_lane: 'Blue' } };
    if (d != null) { s.death_damage = [{ source: 'Melee', damage: 400 }, { source: 'Rend', damage: 300 }, { source: 'Gun Damage', damage: 200 }]; s.death_cause = 'Gun Damage'; s.respawn_secs = Math.max(1, d + 25 - t); }
    const cues: Cue[] = d === t ? [{ key: `death:review:${t}`, text: `Why you died: Drifter did 400 damage with Melee at ${t}.`, priority: 'High', cooldown_secs: 0 }] : [];
    frames.push({ snapshot: s, cues, wall: base + t * 1000 });
  }
  if (opts.postGame) frames.push({ snapshot: { screen: 'PostGame' }, cues: [], wall: base + (to + 5) * 1000 });
  return frames;
}
function ended(frames: ReturnType<typeof match>): EndedMatch[] {
  const tr = new ReviewTracker('live', () => 1_800_000_000_000); const out: EndedMatch[] = [];
  for (const f of frames) { const e = tr.add(f.snapshot, f.wall, f.cues); if (e) out.push(e); }
  const last = tr.finish(); if (last) out.push(last); return out;
}

test('review tracker: a PostGame screen ends the match once; only that match is in the digest; auto review rules', () => {
  const ms = ended(match('hero_orion', 30, 900, [300, 700], { postGame: true }));
  assert.equal(ms.length, 1); const m = ms[0];
  assert.equal(m.byPostGame, true); assert.equal(m.record.deaths.length, 2); assert.ok(autoReviewable(m));
  assert.equal(m.timeline.cues.filter(c => c.key.startsWith('death:review')).length, 2);
  // A short match closed without PostGame is not reviewed automatically (>= 10 min observed needed).
  const short = ended(match('hero_orion', 30, 400, [200]))[0];
  assert.equal(short.byPostGame, false); assert.equal(autoReviewable(short), false);
  // Two matches back to back (clock reset): each is its own match; the last one is the replay's.
  const two = [...match('hero_artist', 100, 900, [200, 400, 600]), ...match('hero_orion', 30, 800, [500])];
  const both = ended(two); assert.equal(both.length, 2); assert.equal(both[0].record.hero, 'hero_artist'); assert.equal(both[1].record.hero, 'hero_orion');
  const lastOne = lastMatchOf(two.map(f => ({ snapshot: f.snapshot, cues: f.cues })))!;
  assert.equal(lastOne.record.hero, 'hero_orion'); assert.equal(lastOne.record.deaths.length, 1);
  assert.ok(lastOne.timeline.points.every(p => p.t == null || p.t <= 800));
});

test('digest: deterministic, bounded, observed facts with unknowns marked, improvement vs earlier matches', () => {
  const [prev] = ended(match('hero_orion', 30, 900, [100, 250, 400, 550, 700], { postGame: true, souls: 3000, day: -1 }));
  const [m] = ended(match('hero_orion', 30, 900, [450], { postGame: true }));
  const name = (id: string) => ({ hero_orion: 'Grey Talon', hero_drifter: 'Drifter', hero_inferno: 'Infernus' } as Record<string, string>)[id] || id;
  const a = buildDigest({ match: m, previous: [prev.record], name }), b = buildDigest({ match: m, previous: [prev.record], name });
  assert.equal(a.text, b.text); assert.ok(a.chars <= DIGEST_MAX_CHARS);
  assert.match(a.text, /Hero: Grey Talon \(hero_orion\)/); assert.match(a.text, /Result: not read/);
  assert.match(a.text, /DEATHS \(1\)\n1\. 7:30: cause "Gun Damage"/);
  assert.match(a.text, /damage panel: Melee 400, Rend 300, Gun Damage 200/); assert.match(a.text, /close-range damage on the panel: 44% \(Melee\)/);
  assert.match(a.text, /Rule coach said: "Drifter did 400 damage/);
  assert.match(a.text, /Enemies: Drifter, Infernus/); assert.match(a.text, /K\/D\/A \(scoreboard read at 15:00\): 4\/1\/3/);
  assert.match(a.text, /Team souls, your team vs enemy: 5:00 13k vs 11\.8k \(\+1\.2k\)/);
  assert.match(a.text, /- Time dead: \d+% .* -> improved/); assert.match(a.text, /- Deaths per 10 min: 0\.7 vs 3\.4 over 1 earlier match -> improved/);
  assert.equal(a.comparisons.find(c => c.id === 'dead')!.verdict, 'improved');
  assert.match(a.text, /Weaknesses the profile knew before this match: .*Dying too often \(improved\)/);
  // Without earlier matches nothing is compared; the size cap holds for a tiny budget too.
  assert.match(buildDigest({ match: m }).text, /No earlier matches in the profile/);
  assert.ok(buildDigest({ match: m, previous: [prev.record] }, 1500).chars <= 1500);
  // One review per match: the key ignores wall time and the profile's own match id.
  assert.equal(reviewKey(m.record), reviewKey({ ...m.record, id: 'other', startedAt: 1 }));
  assert.notEqual(reviewKey(m.record), reviewKey(prev.record));
});

test('prompt: persona, JSON contract, evidence rules; token budget', () => {
  const [m] = ended(match('hero_orion', 30, 900, [450], { postGame: true }));
  const p = buildReviewPrompt(buildDigest({ match: m }), 1024);
  assert.equal(p.system, REVIEW_SYSTEM_PROMPT); assert.match(p.system, /elite Deadlock coach/); assert.match(p.user, /MATCH DIGEST/);
  for (const k of ['"summary"', '"went_well"', '"improvements"', '"why_it_matters"', '"how_to_practice"', '"next_game_focus"', '"hero_tips"', '"spoken"', 'about 200 words (150-230)', '"keep_doing"', '"focus_goals"']) assert.ok(p.system.includes(k), k);
  // One voice with the live coach; the focus goal is scored first; a thrown lead is named.
  assert.match(p.system, /FOCUS GOALS: score each goal first/); assert.match(p.system, /LEAD AND THROW, name the throw/); assert.match(p.system, /"we" for the team/);
  // Strengths first, why they worked, balance after a good game, praise in the spoken text.
  assert.match(p.system, /Lead with what went well/); assert.match(p.system, /WHY it worked: the cause and its effect on the team's result/);
  assert.match(p.system, /at least as many went_well items as improvements/); assert.match(p.system, /when the game went well, then genuine, specific praise/); assert.match(p.system, /end with what to keep doing and the next-game focus/);
  assert.equal(reviewMaxTokens(1024), 3072); assert.equal(reviewMaxTokens(2000), 6000); assert.equal(reviewMaxTokens(4000), 8192); assert.equal(reviewMaxTokens(512), 3072);
});

test('digest (rec13 items 13-14): the Defeat banner (match_result), the focus goals scored, a thrown lead, structures and the Midboss taker', () => {
  // A loss after a +30k lead that fell to +5k; Defeat read over the last frames (match_result).
  const frames = match('hero_orion', 30, 1500, [700, 1300], { postGame: false });
  for (const f of frames) {
    const t = f.snapshot.game_clock_secs!;
    const lead = t < 1100 ? Math.round(t * 30) : Math.round(33000 - (t - 1100) * 70);
    f.snapshot.ally_networth = 100000 + lead; f.snapshot.enemy_networth = 100000;
    if (t === 900) { f.snapshot.structures = [{ lane: 'Yellow', kind: 'walker', own: false, alive: true }, { lane: 'Blue', kind: 'guardian', own: false, alive: true }]; f.snapshot.midboss_taken_by = 'ally'; }
    if (t === 1000) f.snapshot.structures = [{ lane: 'Blue', kind: 'guardian', own: false, alive: false }];
    if (t === 1250) f.snapshot.midboss_taken_by = 'enemy';
    if (t >= 1490) f.snapshot.match_result = 'loss';
  }
  const [m] = ended(frames);
  assert.equal(m.record.result, 'loss'); assert.equal(m.timeline.result, 'loss');
  assert.deepEqual(m.timeline.structures!.map(x => [x.t, x.lane, x.kind, x.alive]), [[900, 'Yellow', 'walker', true], [900, 'Blue', 'guardian', true], [1000, 'Blue', 'guardian', false]]);
  assert.deepEqual(m.timeline.midboss, [{ t: 900, by: 'ally' }, { t: 1250, by: 'enemy' }]);
  const d = buildDigest({ match: m, goals: [{ id: 'close_range', text: 'No close-range deaths' }, { id: 'respawn_deaths', text: 'No deaths within 60 s of a respawn' }] });
  assert.ok(d.chars <= DIGEST_MAX_CHARS); assert.equal(d.result, 'loss');
  assert.match(d.text, /Result: loss \(the Defeat banner\)/);
  assert.match(d.text, /FOCUS GOALS \(set at the match start\): score each one first\n- No close-range deaths: (MISSED|not measurable)[^\n]*\n- No deaths within 60 s of a respawn: MET: 2 deaths, none broke it/);
  assert.deepEqual(d.goals.map(g => g.id), ['close_range', 'respawn_deaths']);
  assert.match(d.text, /LEAD AND THROW\nTeam souls lead peaked at \+33k at 18:20 and was \+5k at the last read 25:00 \(28k lost; it started to fall by \d+:\d\d\), and the match was lost: name the throw/);
  assert.match(d.text, /Midboss \(Rejuvenator\) taken by: your team at about 15:00 \(we held the Rejuvenator until about 18:06\); the enemy at about 20:50 \(they held the Rejuvenator until about 23:56\)\./);
  assert.match(d.text, /Structures destroyed \(first read gone\): theirs Blue Guardian 16:40; ours none read\.\nStill standing at the last read: theirs Yellow Walker\./);
  // Without goals set at the start, the earlier matches' goals are scored and marked as not announced.
  const [prev] = ended(match('hero_orion', 30, 900, [100, 250, 400, 550, 700], { postGame: true, day: -1 }));
  const fallback = buildDigest({ match: m, previous: [prev.record] });
  assert.match(fallback.text, /FOCUS GOALS? \(what the earlier matches asked for; not announced live\)/);
  // The goals reach the stored review; the model's focus_goals are parsed.
  const stored = storedReview(d, m, { error: 'x' }); assert.deepEqual(stored.goals, d.goals);
  const r = parseReview(JSON.stringify({ summary: 'Loss.', spoken: 'We lost it.', focus_goals: [{ goal: 'No close-range deaths', met: false, note: '13:20 melee' }, 'junk'] }));
  assert.ok(r.ok && r.review.focus_goals?.length === 2 && r.review.focus_goals[0].met === false && r.review.focus_goals[1].met === null);
  const store = new ReviewStore(memory()); store.put(stored); assert.deepEqual(store.get(stored.id)!.goals!.map(g => g.id), ['close_range', 'respawn_deaths']);
});

const REVIEW = { summary: 'A much cleaner game: 3 deaths and 7% dead.', went_well: ['Only 3 deaths (9:48, 14:50, 22:10)'],
  improvements: [{ title: 'Respect Drifter', evidence: '14:50 and 22:10: melee and Rend', why_it_matters: 'Each death is 40+ s off the map', how_to_practice: 'Hold max range; keep a dash for Drifter' }],
  keep_doing: ['Push when two or more enemies are dead'], next_game_focus: 'Spend before you walk back out.', hero_tips: ['Grey Talon: shoot from max range'], spoken: 'Great game. You died only three times. Next game, spend before you walk out.' };
test('review parsing: fenced, prefixed, think blocks, cut off, not JSON', () => {
  const json = JSON.stringify(REVIEW);
  for (const raw of [json, '```json\n' + json + '\n```', `Here is your review:\n${json}\nGood luck!`, `<think>let me see {"x":1}</think>${json}`]) {
    const r = parseReview(raw); assert.ok(r.ok, raw.slice(0, 40)); if (!r.ok) continue;
    assert.equal(r.review.summary, REVIEW.summary); assert.deepEqual(r.review.keep_doing, REVIEW.keep_doing); assert.equal(r.review.improvements[0].how_to_practice, REVIEW.improvements[0].how_to_practice); assert.equal(r.review.partial, undefined);
  }
  // Cut off inside the spoken text: everything before it survives, marked partial.
  const cut = parseReview(json.slice(0, json.indexOf('Next game,')));
  assert.ok(cut.ok); if (cut.ok) { assert.equal(cut.review.partial, true); assert.equal(cut.review.next_game_focus, REVIEW.next_game_focus); assert.match(cut.review.spoken, /^Great game\. You died only three times\./); }
  // Cut off inside an improvement: the earlier fields survive, the spoken text is composed from them.
  const cut2 = parseReview(json.slice(0, json.indexOf('Each death')));
  assert.ok(cut2.ok); if (cut2.ok) { assert.equal(cut2.review.went_well[0], REVIEW.went_well[0]); assert.match(cut2.review.spoken, /A much cleaner game/); }
  assert.equal(parseReview('Sorry, I cannot review this match.').ok, false);
  assert.equal(parseReview('{"foo": 1}').ok, false);
  assert.deepEqual(repairJson('{"a": "x", "b": ["y", "z'), { a: 'x', b: ['y', 'z'] });
  assert.equal(closeJson('{"a": 1, "b":'), '{"a": 1}');
  // Markdown and lists never reach the voice; long sentences are split for sequential speech.
  assert.equal(speakable('**Great** game.\n- Spend souls.\n1. Group up. K/D/A was fine.'), 'Great game. Spend souls. Group up. kills, deaths and assists was fine.');
  const long = Array.from({ length: 40 }, (_, i) => `word${i}`).join(' ').replace('word19', 'word19,') + '.';
  assert.ok(sentencesOf(`Short one. ${long} End!`).length >= 4);
});

test('review storage: newest first, capped, one per match, failures can be retried, export', () => {
  const kv = memory(); const store = new ReviewStore(kv);
  const [m] = ended(match('hero_orion', 30, 900, [450], { postGame: true }));
  const d = buildDigest({ match: m });
  store.put(storedReview(d, m, { error: 'HTTP 500' }));
  assert.equal(store.has(d.key), false, 'a failed review does not block a retry');
  store.put(storedReview(d, m, { status: 'ok', review: (parseReview(JSON.stringify(REVIEW)) as { ok: true; review: never }).review, raw: '', model: 'm', ms: 1, error: null }));
  assert.equal(store.has(d.key), true); assert.equal(store.reviews.length, 1);
  for (let i = 0; i < MAX_REVIEWS + 5; i++) store.put({ ...store.reviews[0], id: `k${i}` });
  assert.equal(store.reviews.length, MAX_REVIEWS); assert.equal(store.reviews[0].id, `k${MAX_REVIEWS + 4}`);
  const again = new ReviewStore(kv); assert.equal(again.reviews.length, MAX_REVIEWS); assert.equal(again.reviews[0].review?.summary, REVIEW.summary);
  assert.equal(JSON.parse(again.exportJSON('2026-10-29T00:00:00Z')).reviews.length, MAX_REVIEWS);
  kv.setItem('deadlock-coach-match-reviews-v1', '{corrupt'); assert.equal(new ReviewStore(kv).reviews.length, 0);
});

test('requestReview: budget, a cut-off reply retried with 8,192 tokens, raw text when not JSON', async () => {
  clearAiExchanges();
  const cfg: AiConfig = { provider: 'openai', baseUrl: 'https://gateway.test/v1', model: 'm', key: 'k', maxTokens: 1024, headers: '', reasoning: 'off' };
  const [m] = ended(match('hero_orion', 30, 900, [450], { postGame: true }));
  const prompt = buildReviewPrompt(buildDigest({ match: m }), 1024);
  const sent: number[] = []; const json = JSON.stringify(REVIEW);
  const reply = (content: string, finish = 'stop') => new Response(JSON.stringify({ model: 'm', choices: [{ message: { content }, finish_reason: finish }] }), { status: 200 });
  let n = 0;
  const fetchImpl = (async (_u: string, init: RequestInit) => { sent.push(JSON.parse(String(init.body)).max_tokens); return n++ === 0 ? reply(json.slice(0, 200), 'length') : reply(json); }) as typeof fetch;
  const r = await requestReview(cfg, prompt, { fetchImpl });
  assert.deepEqual(sent, [3072, 8192]); assert.equal(r.status, 'ok'); assert.equal(r.review?.summary, REVIEW.summary);
  const raw = await requestReview(cfg, prompt, { fetchImpl: (async () => reply('I think you played well overall.')) as typeof fetch });
  assert.equal(raw.status, 'raw'); assert.equal(raw.raw, 'I think you played well overall.');
});

// The recorded matches (local, not in the repository): rec12's first frames are the previous Violet
// match; the digest covers only the Grey Talon match and reads as clearly improved over rec8 + rec11.
test('digest on the recorded matches: rec12 is only the last match and reads as improved; rec11 has its 7 deaths', async () => {
  const root = 'C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
  const files = ['rec8/replay2', 'rec11/replay1', 'rec12/replay1'].map(f => `${root}/${f}.jsonl`);
  if (!files.every(f => existsSync(f))) return;
  const { profileOf, heroNames } = await import('../scripts/player-profile-replay.ts');
  const names = heroNames(); const name = (id: string) => names[id] || id;
  const load = (f: string) => readFileSync(f, 'utf8').split(/\r?\n/).filter(l => l.trim()).map(l => JSON.parse(l) as { snapshot: Snapshot; cues?: Cue[] });
  const m12 = lastMatchOf(load(files[2]))!;
  assert.equal(m12.record.hero, 'hero_orion'); assert.equal(m12.record.deaths.length, 3);
  assert.ok(m12.timeline.points.every(p => p.t == null || p.t < 1900), 'no Violet frames');
  const d12 = buildDigest({ match: m12, previous: profileOf([files[0], files[1]]).matches, name });
  assert.ok(d12.chars <= DIGEST_MAX_CHARS); assert.equal(d12.text, buildDigest({ match: m12, previous: profileOf([files[0], files[1]]).matches, name }).text);
  assert.match(d12.text, /Hero: Grey Talon/); assert.match(d12.text, /DEATHS \(3\)/); assert.match(d12.text, /Rule coach said:/);
  for (const id of ['dead', 'per10', 'unspent']) assert.equal(d12.comparisons.find(c => c.id === id)?.verdict, 'improved', id);
  assert.match(d12.text, /Dying too often \(improved\)/);
  // What went well, with evidence, leads the digest (before ECONOMY and DEATHS) and the prompt asks to lead with it.
  assert.ok(d12.goodGame); assert.ok(d12.text.indexOf('STRENGTHS') < d12.text.indexOf('ECONOMY') && d12.text.indexOf('STRENGTHS') < d12.text.indexOf('DEATHS ('));
  for (const id of ['kda', 'dead', 'deathless', 'windows', 'urn', 'shop', 'lead', 'patron']) assert.ok(d12.strengths.some(s => s.id === id), `strength ${id}: ${d12.strengths.map(s => s.id)}`);
  for (const re of [/K\/D\/A 12\/3\/11: 23 kills and assists for 3 deaths/, /Dead only 7% of the game/, /Deathless stretches: 0:\d\d-9:\d\d \(9\.\d min\)/, /Enemy-dead windows turned into pushes: 13:\d\d \(\d enemies dead, you in the enemy base\)[^\n]*27:\d\d/,
    /Delivered the Soul Urn: 16:\d\d \(\+2,\d{3} souls\)/, /Spent on the death screen: [^\n]*after the 14:50 death about 5\.9k spent/, /Team souls lead from \d+:\d\d to the end/, /Pressure on the enemy Patron \(ARCHMOTHER\)/, /Verdict: a good game \(likely win\)/])
    assert.match(d12.text, re);
  assert.match(buildReviewPrompt(d12).user, /^Review this match for the player\. It went well: open with the strengths and why they worked/);
  const d11s = buildDigest({ match: lastMatchOf(load(files[1]))!, previous: profileOf([files[0]]).matches, name });
  assert.equal(d11s.goodGame, false); assert.ok(d11s.strengths.length < d12.strengths.length);
  const m11 = lastMatchOf(load(files[1]))!;
  const d11 = buildDigest({ match: m11, previous: profileOf([files[0]]).matches, name });
  assert.equal(m11.record.deaths.length, 7); assert.match(d11.text, /Hero: Violet/); assert.match(d11.text, /DEATHS \(7\)/); assert.ok(d11.chars <= DIGEST_MAX_CHARS);
  assert.notEqual(d11.key, d12.key);
});

// ---- Match end without a result screen (MATCH_END in player-profile.ts) ----
type F = { snapshot: Snapshot; cues: Cue[]; wall: number };
const BASE = 1_800_000_000_000;
/** After `match()` (gameplay to `to`, wall = BASE + clock): append frames `ms` after the last one. */
function tail(frames: F[], ms: number, s: Snapshot) { frames.push({ snapshot: s, cues: [], wall: frames.at(-1)!.wall + ms }); return frames; }
function feed(frames: F[]) {
  const tr = new ReviewTracker('live', () => BASE); const out: Array<{ i: number; m: EndedMatch }> = [];
  frames.forEach((f, i) => { const m = tr.add(f.snapshot, f.wall, f.cues); if (m) out.push({ i, m }); });
  return { tr, out };
}
/** rec13: dies at 25:00 (respawn 71 -> 1), "Defeat" read as Dead / Scoreboard, a black frame, the
 *  MVP screens (Unknown), then the post-game summary read as Shop with an extrapolated clock. */
function rec13Like() {
  const f = match('hero_orion', 30, 1500, [400, 900]);
  for (let c = 1505, r = 71; r >= 1; c += 7, r -= 7) tail(f, 7000, { game_clock_secs: c, screen: 'Dead', dead: true, hero: 'hero_orion', respawn_secs: Math.max(1, r) });
  const board = tail(f, 5000, { game_clock_secs: 1580, screen: 'Scoreboard', dead: false, hero: 'hero_orion' }).length - 1;
  tail(f, 5000, { screen: 'Loading' });
  for (let i = 0; i < 4; i++) tail(f, 2500, { screen: 'Unknown' });
  for (let i = 0; i < 3; i++) tail(f, 4000, { game_clock_secs: 1598 + i * 4, screen: 'Shop', dead: false, hero: 'hero_orion', souls: null });
  return { f, board };
}
test('match end: rec13-like (dead -> scoreboard -> loading -> MVP -> summary as Shop) ends the match once, 20 s after the last live frame', () => {
  const { f, board } = rec13Like();
  const { tr, out } = feed(f);
  assert.equal(out.length, 1); const { i, m } = out[0];
  assert.equal(m.endedBy, 'idle'); assert.equal(m.byPostGame, false); assert.ok(autoReviewable(m));
  assert.ok(f[i].wall - f[board].wall >= 20_000 && f[i].wall - f[board].wall < 25_000, `ended ${(f[i].wall - f[board].wall) / 1000} s after the scoreboard`);
  assert.equal(m.record.endClock, 1580); assert.equal(m.record.deaths.length, 3);
  // The summary frames are not game time.
  assert.ok(m.timeline.points.every(p => p.t == null || p.t <= 1585), String(m.timeline.points.at(-1)?.t));
  // Nothing more: the rest of the summary and the end-of-game camera at the final clock start no match.
  for (const s of [{ game_clock_secs: 1610, screen: 'Shop', hero: 'hero_orion' }, { game_clock_secs: 1582, screen: 'Gameplay', hero: 'hero_orion' }, { screen: 'Unknown' }] as Snapshot[]) assert.equal(tr.add(s, f.at(-1)!.wall + 3000), null);
  assert.equal(tr.current(), null); assert.equal(tr.finish(), null);
  // The same frames without wall time (a replay): the match also ends at the loading screens.
  const replay = new ReviewTracker('replay', () => BASE); let ended: EndedMatch | null = null;
  for (const x of f) ended ??= replay.add(x.snapshot, null, x.cues);
  assert.equal(ended?.endedBy, 'idle'); assert.equal(ended?.record.endClock, 1580);
  // With the vision fix the banner over the death screen is a PostGame frame: ended at once, a loss.
  const pg = match('hero_orion', 30, 1500, [400]); tail(pg, 7000, { game_clock_secs: 1507, screen: 'Dead', dead: true, respawn_secs: 1 }); tail(pg, 5000, { screen: 'PostGame' });
  const r = feed(pg).out; assert.equal(r.length, 1); assert.equal(r[0].i, pg.length - 1); assert.equal(r[0].m.endedBy, 'postgame'); assert.ok(r[0].m.byPostGame);
});
test('match end: never during gameplay: long death screens, alt-tab while dead, pauses, menus, short matches', () => {
  // A 90 s respawn shown in full, with the shop open while dead (no souls read on some frames).
  const longDeath = match('hero_orion', 30, 1500, []);
  for (let c = 1505, r = 90; r > 0; c += 5, r -= 5) tail(longDeath, 5000, c % 15 ? { game_clock_secs: c, screen: 'Dead', dead: true, respawn_secs: r } : { game_clock_secs: c, screen: 'Shop', dead: true, respawn_secs: r, souls: null });
  for (let c = 1600; c <= 1700; c += 5) tail(longDeath, 5000, { game_clock_secs: c, screen: 'Gameplay', hero: 'hero_orion', souls: 900 });
  assert.equal(feed(longDeath).out.length, 0);
  // Dead with an 80 s timer, then alt-tab (a black frame, other windows) for 60 s, then back.
  const altTab = match('hero_orion', 30, 1500, []);
  tail(altTab, 5000, { game_clock_secs: 1505, screen: 'Dead', dead: true, respawn_secs: 80 }); tail(altTab, 3000, { screen: 'Loading' });
  for (let i = 0; i < 12; i++) tail(altTab, 5000, { screen: 'Unknown' });
  tail(altTab, 5000, { game_clock_secs: 1573, screen: 'Dead', dead: true, respawn_secs: 12 });
  for (let c = 1590; c <= 1650; c += 5) tail(altTab, 5000, { game_clock_secs: c, screen: 'Gameplay', hero: 'hero_orion' });
  const a = feed(altTab); assert.equal(a.out.length, 0); assert.equal(a.tr.current()?.record.endClock, 1650);
  // A death screen without any timer read waits 90 s.
  const noTimer = match('hero_orion', 30, 1500, []);
  tail(noTimer, 5000, { game_clock_secs: 1505, screen: 'Dead', dead: true }); tail(noTimer, 3000, { screen: 'Loading' });
  for (let i = 0; i < 10; i++) tail(noTimer, 5000, { screen: 'Unknown' });
  assert.equal(feed(noTimer).out.length, 0);
  // A pause: the HUD with a frozen clock and the Esc menu for 3 minutes, then play resumes.
  const pause = match('hero_orion', 30, 1500, []);
  for (let i = 0; i < 36; i++) tail(pause, 5000, i % 3 ? { game_clock_secs: 1500, screen: 'Gameplay', hero: 'hero_orion' } : { screen: 'Menu' });
  for (let c = 1505; c <= 1600; c += 5) tail(pause, 5000, { game_clock_secs: c, screen: 'Gameplay', hero: 'hero_orion' });
  assert.equal(feed(pause).out.length, 0);
  // Unknown frames alone (alt-tab, a vision miss) never end a match, however long.
  const unknown = match('hero_orion', 30, 1500, []);
  for (let i = 0; i < 40; i++) tail(unknown, 5000, { screen: 'Unknown' });
  for (let c = 1700; c <= 1750; c += 5) tail(unknown, 5000, { game_clock_secs: c, screen: 'Gameplay', hero: 'hero_orion' });
  assert.equal(feed(unknown).out.length, 0);
  // A short match (under 10 minutes observed) is not ended by loading screens.
  const short = match('hero_orion', 30, 400, [200]); tail(short, 5000, { screen: 'Loading' });
  for (let i = 0; i < 10; i++) tail(short, 4000, { screen: 'Unknown' });
  const s = feed(short); assert.equal(s.out.length, 0); assert.ok(s.tr.current());
  // A black frame mid-match followed by live play within 20 s changes nothing.
  const blink = match('hero_orion', 30, 1500, []); tail(blink, 5000, { screen: 'Loading' }); tail(blink, 4000, { screen: 'Unknown' }); tail(blink, 4000, { screen: 'Unknown' });
  for (let c = 1515; c <= 1600; c += 5) tail(blink, 5000, { game_clock_secs: c, screen: 'Gameplay', hero: 'hero_orion' });
  assert.equal(feed(blink).out.length, 0);
});
test('match end on Stop: a match observed for 10+ minutes is closed and reviewable; a shorter one stays open', () => {
  const long = feed(match('hero_orion', 30, 1500, [400])).tr;
  const m = long.stop(); assert.equal(m?.endedBy, 'stop'); assert.ok(m && autoReviewable(m)); assert.equal(m!.record.endClock, 1500);
  assert.equal(long.current(), null); assert.equal(long.stop(), null);
  const short = feed(match('hero_orion', 30, 500, [200])).tr;
  assert.equal(short.stop(), null); assert.ok(short.current()); assert.ok(short.openSecs > 400 && short.openSecs < 600);
  // "Review my last match" closes it explicitly.
  assert.equal(short.finish()?.endedBy, 'finish');
});
test('digest deaths (coach-review-8 D12): top bar at the death, the walk-out from base, an ignored hold call; position unknown is "unknown", never "no"', () => {
  // rec8 42:05: three teammates dead, a "hold in base" call, a walk-out from base 21 s before the death.
  const frames = match('hero_orion', 30, 900, [600]);
  for (const f of frames) {
    const t = f.snapshot.game_clock_secs!, s = f.snapshot;
    s.hero_status = Array.from({ length: 12 }, (_, i) => ({ ally: i < 6, slot: i % 6, hero: i === 0 ? 'hero_orion' : null, dead: i >= 1 && i <= 3 && t >= 520 && t <= 640, respawn_secs: 20,
      networth_observations: 1, observations: 1, position_observations: 0 })) as Snapshot['hero_status'];
    if (s.minimap) {
      s.minimap.you = t >= 540 && t < 575 ? { x: 0.45, y: 0.92 } : t >= 575 && t < 600 ? { x: 0.6, y: 0.72 } : { x: 0.5, y: 0.7 };
      s.minimap.heroes = [{ id: 1, ally: true, pos: { x: 0.65, y: 0.76 }, unseen_ms: 0, hits: 9 }, { id: 2, ally: true, pos: { x: s.minimap.you.x + 0.01, y: s.minimap.you.y }, unseen_ms: 0, hits: 9 }];
    }
    if (t === 550) f.cues.push({ key: 'active:hold_base', text: "Hold in base: three teammates are dead, the next back in 23 seconds. Don't walk out alone.", priority: 'High', cooldown_secs: 0 });
  }
  const [m] = ended(frames);
  assert.equal(m.timeline.holds?.length, 1);
  const p = m.timeline.points.find(x => x.t === 580)!;
  assert.equal(p.alliesDead, 3); assert.equal(p.y, 0.72);
  assert.equal(p.allyDist, 0.06, 'a teammate marker on top of yours is your own icon (ignored)');
  const d = buildDigest({ match: m }).text;
  assert.match(d, /1\. 10:00: [^\n]*top bar at the death: 3 of 5 teammates dead, 0 of 6 enemies dead; in the enemy half no; alone no \(nearest teammate on the minimap 0\.06 map away\)/);
  assert.match(d, /left our base at 9:35 with 3 teammates dead \(0 enemies dead\), 25 s before the death; rule call 50 s before: "Hold in base: three teammates are dead[^"]*", then you left base anyway with 3 teammates still dead \(an ignored call\)/);
  // A death in their half with no marker read near it: alone / enemy half / forward are unknown, not "no".
  const blind = match('hero_orion', 30, 900, [600]);
  for (const f of blind) { const t = f.snapshot.game_clock_secs!; if (f.snapshot.minimap) f.snapshot.minimap.you = t >= 585 ? null : { x: 0.5, y: 0.3 }; }
  const b = buildDigest({ match: ended(blind)[0] }).text;
  assert.match(b, /1\. 10:00: [^\n]*not read: your position in the last 12 s \(alone, enemy half and forward are unknown\)/);
  assert.doesNotMatch(b, /1\. 10:00: [^\n]*(alone no|forward with 3\+ enemies unseen no)/);
  // Deep in their base: said so; and a stored timeline without the new fields keeps the old wording.
  const deep = match('hero_orion', 30, 900, [600]);
  for (const f of deep) if (f.snapshot.minimap) f.snapshot.minimap.you = { x: 0.3, y: 0.25 };
  assert.match(buildDigest({ match: ended(deep)[0] }).text, /in the enemy half yes \(deep, near their base\)/);
  const old = ended(match('hero_orion', 30, 900, [600]))[0];
  for (const x of old.timeline.points) { delete x.alliesDead; delete x.y; delete x.allyDist; }
  assert.doesNotMatch(buildDigest({ match: old }).text, /teammates dead|your position in the last 12 s/);
  // A Rejuvenator revive read by the vision is listed with the objectives.
  const rev = match('hero_orion', 30, 900, []);
  for (const f of rev) { const t = f.snapshot.game_clock_secs!; if (t === 500) f.snapshot.midboss_taken_by = 'ally'; if (t === 560) (f.snapshot as Snapshot & { revived?: boolean }).revived = true; }
  assert.match(buildDigest({ match: ended(rev)[0] }).text, /Rejuvenator revives in place read on your screen: 9:20 \(each spends one team credit\)\./);
});
