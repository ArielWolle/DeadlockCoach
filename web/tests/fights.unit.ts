// Team fights in the Player profile (player-profile.ts FightTracker, docs/player-profile.md
// "Fights"): one test per bug found in the rec13 / rec16 / rec17 screenshot review.
import test from 'node:test';
import assert from 'node:assert/strict';
import { FightTracker, PlayerProfile, PlayerProfileModel, fightsLegacy, fightsText, migrate, sanitizeMatch, type MatchRecord } from '../src/player-profile.ts';
import { METRICS, lastMatchOf } from '../src/ai-review.ts';
import type { Snapshot } from '../src/types.ts';

type Hero = [ally: boolean, x: number, y: number];
interface F { you?: [number, number] | null; dead?: boolean; hp?: number | null; heroes?: Hero[]; down?: string[]; screen?: string; team?: boolean }
/** One frame: `down` = top-bar portraits dead ('a0'..'a5' ours, 'e0'..'e5' theirs). */
function snap(t: number, f: F = {}): Snapshot {
  const dead = f.dead === true;
  const team = Array.from({ length: 12 }, (_, i) => {
    const ally = i < 6, slot = i % 6, down = (f.down || []).includes(`${ally ? 'a' : 'e'}${slot}`);
    return { ally, slot, dead: down, respawn_secs: down ? 20 : null, respawn_observations: down ? 1 : 0, bar_visible: true, health_ratio: down ? null : 1, observations: 1, networth_observations: 0, position_observations: 0 };
  });
  const you = f.you === undefined ? [0.5, 0.6] : f.you;
  return { game_clock_secs: t, screen: f.screen ?? (dead ? 'Dead' : 'Gameplay'), dead, hero: 'hero_orion', hp: dead ? null : (f.hp === undefined ? 1000 : f.hp), max_hp: dead ? null : 1000, health_observations: 3,
    hero_status: f.team === false ? [] : team,
    minimap: dead ? null : { observations: 3, you: you ? { x: you[0], y: you[1] } : null, your_lane: null, midboss_up: false, allies: [], enemies: [], enemy_count: 0,
      heroes: (f.heroes || []).map(([ally, x, y], id) => ({ id, ally, pos: { x, y }, hits: 5, unseen_ms: 0 })) } } as unknown as Snapshot;
}
/** Frames every 2 s from t0 to t1 (inclusive) built by `f(t)`. */
const run = (tr: FightTracker, t0: number, t1: number, f: (t: number) => F) => { for (let t = t0; t <= t1; t += 2) tr.add(snap(t, f(t)), t); };
const clashAt = (x: number, y: number): Hero[] => [[true, x, y], [true, x + .03, y], [false, x + .05, y], [false, x + .06, y + .03]];

test('fights: a 2v2 lane clash without a death is no fight; your own icon is not a teammate (rec17 16:42, 28:59; rec13 11:11-12:49)', () => {
  const tr = new FightTracker();
  // 40 s of you + a teammate poking two enemies; the vision also reads your icon as an ally.
  run(tr, 600, 640, () => ({ you: [.2, .5], heroes: [[true, .2, .5], [true, .23, .5], [false, .27, .5], [false, .28, .52]] }));
  assert.deepEqual(tr.episodes(), [], 'posturing is not a team fight (the old count said "joined")');
  // One kill in that clash is a fight (a skirmish with a death), joined.
  run(tr, 642, 650, t => ({ you: [.2, .5], heroes: [[true, .25, .5], [false, .27, .5], [false, .28, .52]], down: t >= 646 ? ['e2'] : [] }));
  run(tr, 652, 680, () => ({ you: [.2, .5], down: ['e2'] }));
  assert.deepEqual(tr.episodes().map(e => [e.s, e.why, e.deaths]), [['joined', 'near on the minimap', 1]]);
});

test('fights: deaths read only on the top bar make a fight; 3 enemies dying while you stand with your team is joined (rec16 17:46)', () => {
  const tr = new FightTracker();
  run(tr, 1060, 1066, () => ({ you: [.44, .67], heroes: [[true, .45, .62], [true, .47, .6]] }));
  run(tr, 1068, 1100, t => ({ you: [.44, .67], heroes: [[true, .45, .62], [true, .47, .6]], down: [...(t >= 1068 ? ['e2'] : []), ...(t >= 1070 ? ['e4'] : []), ...(t >= 1072 ? ['e3'] : [])] }));
  const [e] = tr.episodes();
  assert.equal(e.deaths, 3); assert.equal(e.s, 'joined'); assert.equal(e.why, 'with your team');
  assert.equal(e.t, 1068); assert.equal(e.end, 1072);
});

test('fights: one fight is one episode: unread markers inside it do not split it into joined + missed (rec16 20:22-20:43)', () => {
  const tr = new FightTracker();
  // Four of ours die over 28 s in mid (clash at 0.47,0.6) while you farm the left lane, 0.28 away;
  // your marker is unread for 10 s in the middle.
  const down = (t: number) => [...(t >= 1228 ? ['a2'] : []), ...(t >= 1232 ? ['a5'] : []), ...(t >= 1240 ? ['a1'] : []), ...(t >= 1254 ? ['a0'] : [])];
  run(tr, 1220, 1300, t => ({ you: t >= 1232 && t < 1242 ? null : [.19, .55], heroes: t <= 1254 ? clashAt(.45, .6) : [], down: down(t) }));
  const eps = tr.episodes();
  assert.equal(eps.length, 1, JSON.stringify(eps));
  assert.equal(eps[0].deaths, 4); assert.equal(eps[0].s, 'missed', '0.28 away is another lane, not "within a third of the map"');
});

test('fights: the verdict covers the whole fight, not its first frame: arriving late is joined', () => {
  const tr = new FightTracker();
  run(tr, 900, 940, t => ({ you: t < 916 ? [.15, .5] : [.42, .6], heroes: clashAt(.45, .6), down: [...(t >= 910 ? ['e1'] : []), ...(t >= 920 ? ['a3'] : [])] }));
  run(tr, 942, 970, () => ({ you: [.42, .6], down: ['e1', 'a3'] }));
  assert.deepEqual(tr.episodes().map(e => e.s), ['joined']);
});

test('fights: an unread marker or a shop screen is unknown, never missed; a fight not placed on the minimap is not missed either', () => {
  const tr = new FightTracker();
  run(tr, 1000, 1040, t => ({ you: null, heroes: clashAt(.45, .6), down: t >= 1010 ? ['e1', 'e2'] : [] }));
  run(tr, 1042, 1070, () => ({ you: null, down: ['e1', 'e2'] }));
  // Two allies die somewhere while you stand in base (no clash read anywhere): where is unknown.
  run(tr, 1100, 1140, t => ({ you: [.44, .9], down: t >= 1110 ? ['a1', 'a2'] : [] }));
  run(tr, 1142, 1170, () => ({ you: [.44, .9], down: ['a1', 'a2'] }));
  // Shopping through a fight: no minimap at all.
  run(tr, 1200, 1240, t => ({ screen: 'Shop', you: null, down: t >= 1210 ? ['e3', 'e4'] : [] }));
  run(tr, 1242, 1270, () => ({ down: ['e3', 'e4'] }));
  assert.deepEqual(tr.episodes().map(e => [e.s, e.why]), [['unknown', 'your marker not read'], ['unknown', 'fight not on the minimap'], ['unknown', 'your marker not read']]);
});

test('fights: dead, respawning and healing are not missed (rec16 13:58 at 13%; rec16 33:07 while dead)', () => {
  const tr = new FightTracker();
  // Dead through a fight.
  run(tr, 2000, 2040, t => ({ dead: true, down: [...(t >= 2006 ? ['a2'] : []), ...(t >= 2014 ? ['a3'] : [])] }));
  run(tr, 2042, 2060, () => ({ you: [.45, .9], down: ['a2', 'a3'] }));
  // Respawned 4 s before it began, still in base.
  run(tr, 2100, 2110, () => ({ dead: true }));
  run(tr, 2112, 2150, t => ({ you: [.45, .92], heroes: clashAt(.7, .5), down: t >= 2116 ? ['e1', 'e2'] : [] }));
  run(tr, 2152, 2180, () => ({ you: [.45, .92], down: ['e1', 'e2'] }));
  // Walking back to heal at 13%: healing in base while three of ours die on the right.
  run(tr, 2300, 2350, t => ({ you: t < 2316 ? [.5, .8] : [.44, .92], hp: t < 2318 ? 130 : 700, heroes: [[true, .77, .72], [false, .72, .67], [false, .7, .71]],
    down: [...(t >= 2318 ? ['a1'] : []), ...(t >= 2320 ? ['a2'] : []), ...(t >= 2322 ? ['a3'] : [])] }));
  run(tr, 2352, 2380, () => ({ you: [.44, .92], down: ['a1', 'a2', 'a3'] }));
  assert.deepEqual(tr.episodes().map(e => e.s), ['dead', 'dead', 'healing']);
});

test('fights: a flickering portrait dies once; one dead read alone is a misread; your death screen and portrait are one death (rec16 33:04, 32:33/32:44)', () => {
  const tr = new FightTracker();
  run(tr, 100, 110, () => ({}));
  // a1 dead, alive on one read, dead again: one death. e0 dead on a single read: no death.
  const seq: F[] = [{ down: ['a1'] }, { down: ['a1'] }, {}, { down: ['a1'] }, { down: ['a1'] }, { down: ['e0'] }, {}, {}];
  seq.forEach((f, i) => tr.add(snap(112 + i * 2, f), 112 + i * 2));
  assert.deepEqual(tr.deathLog.map(d => `${d.ally ? 'a' : 'e'}${d.slot}@${d.t}`), ['a1@112']);
  // You die at 200 (death screen); your portrait's countdown is read only at 210; one more ally dies at 206.
  const t2 = new FightTracker();
  run(t2, 180, 198, () => ({ you: [.6, .6] }));
  run(t2, 200, 230, t => ({ dead: true, down: [...(t >= 206 ? ['a1'] : []), ...(t >= 210 ? ['a4'] : [])] }));
  const [e] = t2.episodes();
  assert.equal(e.deaths, 2, 'your death counted once'); assert.equal(e.s, 'joined'); assert.equal(e.why, 'died in it');
});

test('fights: dying alone while others posture across the map is a pick, not a fight you joined', () => {
  const tr = new FightTracker();
  run(tr, 760, 798, () => ({ you: [.3, .6], heroes: clashAt(.8, .2) }));
  run(tr, 800, 840, () => ({ dead: true }));
  assert.deepEqual(tr.episodes(), []);
  // The same death inside that clash is a fight (died in it).
  const t2 = new FightTracker();
  run(t2, 760, 798, () => ({ you: [.8, .22], heroes: clashAt(.8, .2) }));
  run(t2, 800, 840, () => ({ dead: true }));
  assert.deepEqual(t2.episodes().map(e => [e.s, e.why, e.deaths]), [['joined', 'died in it', 1]]);
});

test('fights: deaths more than 15 s apart are separate fights; clashes never chain them (rec16 15:23-17:52 was one 12-death "fight")', () => {
  const tr = new FightTracker();
  run(tr, 900, 1080, t => ({ you: [.45, .6], heroes: clashAt(.45, .6),
    down: [...(t >= 920 ? ['e1'] : []), ...(t >= 960 ? ['e2'] : []), ...(t >= 1000 ? ['e3'] : []), ...(t >= 1040 ? ['e4'] : [])] }));
  assert.equal(tr.episodes().length, 4);
});

test('fights: profile record and review timeline agree; counts, text and per-fight detail in the matches table', () => {
  const frames: Array<{ snapshot: Snapshot }> = [];
  const add = (t0: number, t1: number, f: (t: number) => F) => { for (let t = t0; t <= t1; t += 2) frames.push({ snapshot: snap(t, f(t)) }); };
  add(600, 700, () => ({ you: [.3, .6] }));
  add(702, 740, t => ({ you: [.3, .6], heroes: clashAt(.32, .6), down: t >= 712 ? ['e1', 'e2'] : [] }));          // joined
  add(742, 800, () => ({ you: [.3, .6], down: [] }));
  add(802, 840, t => ({ you: [.1, .5], heroes: clashAt(.6, .6), down: t >= 812 ? ['a1', 'a2'] : [] }));            // missed
  add(842, 900, () => ({ you: [.1, .5] }));
  add(902, 940, t => ({ you: null, heroes: clashAt(.6, .6), down: t >= 912 ? ['e3', 'e4'] : [] }));                 // unknown
  add(942, 1000, () => ({ you: [.3, .6] }));
  const m = lastMatchOf(frames)!;
  assert.deepEqual(m.timeline.fights.map(f => f.status), ['joined', 'missed', 'unknown']);
  assert.deepEqual({ ...m.record.fights, at: undefined }, { v: 2, joined: 1, missed: 1, whileDead: 0, healing: 0, unknown: 1, at: undefined });
  const p = new PlayerProfile({ now: () => 0 });
  for (const f of frames) p.observe(f.snapshot, null, 'replay');
  p.endMatch();
  const row = p.summary().rows[0];
  assert.equal(row.fights, 'joined 1 of 3, missed 1 (1 unknown)');
  assert.equal(row.fightsJoined, 1); assert.equal(row.fightsMissed, 1);
  assert.match(row.fightsDetail, /^11:52 joined \(near on the minimap, 2 deaths\) · 13:32 missed \(alive, elsewhere on the map, 2 deaths\) · 15:12 unknown \(your marker not read, 2 deaths\)$/);
});

test('fights: the table text', () => {
  assert.equal(fightsText({ v: 2, joined: 5, missed: 1, whileDead: 0, healing: 0, unknown: 2 }), 'joined 5 of 8, missed 1 (2 unknown)');
  assert.equal(fightsText({ v: 2, joined: 5, missed: 0, whileDead: 1, healing: 1, unknown: 0 }), 'joined 5 of 7 (1 while dead, 1 healing)');
  assert.equal(fightsText({ v: 2, joined: 0, missed: 0, whileDead: 0 }), 'none seen');
  assert.equal(fightsText({ joined: 13, missed: 4, whileDead: 0 }), '— (older version)');
});

test('fights: stored matches from before v2 still load, show as older and are left out of traits and comparisons; v2 survives export / import', () => {
  const base = sanitizeMatch({ id: 'old-1', startedAt: 1, time: { lane: 900, jungle: 100, base: 100, dead: 100, unknown: 0 }, deaths: [], spanSecs: 1200, deadSecs: 100, hero: 'hero_orion',
    fights: { joined: 1, missed: 9, whileDead: 0 }, windows: { used: 0, unused: 0, unusedAt: [] }, souls: {}, lowHp: {}, farm: {} })!;
  assert.ok(fightsLegacy(base));
  const old = Array.from({ length: 3 }, (_, i) => ({ ...base, id: `old-${i}`, startedAt: 1 + i })) as MatchRecord[];
  const model = new PlayerProfileModel();
  const s = model.summary(old);
  assert.equal(s.rows[0].fights, '— (older version)'); assert.equal(s.rows[0].fightsJoined, null);
  assert.ok(!s.weaknesses.some(t => t.id === 'missed_fights'), '27 old "missed" fights are not evidence');
  assert.equal(METRICS.find(m => m.id === 'fights')!.f(old[0]), null);
  // A v2 record round-trips (episodes validated, unknown statuses dropped).
  const v2 = { ...base, id: 'new-1', fights: { v: 2, joined: 2, missed: 1, whileDead: 0, healing: 0, unknown: 0, at: [{ t: 700, end: 712, deaths: 2, s: 'joined', why: 'died in it' }, { t: 800, end: 812, deaths: 2, s: 'bogus', why: '' }] } };
  const back = migrate({ schema: 1, matches: [v2] }).matches[0];
  assert.equal(back.fights.v, 2); assert.equal(back.fights.at!.length, 1); assert.equal(back.fights.at![0].s, 'joined');
  assert.equal(model.summary([back]).rows[0].fights, 'joined 2 of 3, missed 1');
});
