// End-of-game AI review: DOM-free logic (docs/ai-coach.md "End-of-game review").
//
// When a match ends, the optional AI coach writes a longer review of the whole match: what went
// well, the top three improvements by impact (with timestamps), a next-game focus, hero tips and a
// 180-300 word spoken version. This module builds the evidence for it and handles the reply:
//   MatchTimeline   compact per-frame facts for one match (net worth, souls, areas, fights,
//                   damage panels, objectives, rule-coach death reviews / pattern texts)
//   ReviewTracker   match boundaries (a private PlayerProfile) + the timeline: feed live frames or a
//                   replay's snapshots, get the finished match (record + timeline) back
//   buildDigest     deterministic match digest (observed facts only, unknowns marked, <= 8k chars)
//   buildReviewPrompt / parseReview / requestReview   prompt, tolerant JSON parsing, gateway call
//   ReviewStore     localStorage list of reviews (newest first, cap 20, one per match)
// Nothing here touches the DOM; tests/ai-review.unit.ts imports it directly.
import { LIMITS, PlayerProfile, killerFamily, PlayerProfileModel, REVIEW, deadOf, deathSpend, spanOf, spendTarget, resultOf, type KeyValueStore, type MatchRecord } from './player-profile.ts';
import { GatewayError, HERO_ROLES, aiExchanges, heroMatchup, callGateway, clockText, extractText, jsonCandidates, retrieve, sections, type AiConfig, type CallOptions } from './ai-coach.ts';
import type { Cue, Knowledge, Snapshot } from './types';

export const DIGEST_MAX_CHARS = 8000;
export const REVIEW_STORE_KEY = 'deadlock-coach-match-reviews-v1';
export const MAX_REVIEWS = 20;
/** A match the profile closes without a PostGame screen is reviewed automatically only after this much observed game time. */
export const AUTO_MIN_SECS = 600;
/** Max reply tokens for a review: three times the live setting, at least 3,072, at most 8,192 (the reasoning headroom is added by buildRequest as for live calls). */
export const reviewMaxTokens = (aiMaxTokens: number) => Math.min(8192, Math.max(Math.round(aiMaxTokens) * 3, 3072));
export const REVIEW_TIMEOUT_MS = 120_000;

const finite = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);
const k = (n: number) => Math.abs(n) >= 1000 ? `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k` : String(Math.round(n));
const pct = (n: number, d: number) => d > 0 ? Math.round(n * 100 / d) : 0;
const grouped = (n: number) => Math.round(n).toLocaleString('en-US');
const dist = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);
const clip = (s: string, n: number) => s.length > n ? s.slice(0, n - 1).trimEnd() + '…' : s;

// ---------------------------------------------------------------------------------------------
// Timeline: compact facts per frame of one match
// ---------------------------------------------------------------------------------------------
export type TimelineArea = 'lane' | 'jungle' | 'base' | 'enemy_base' | 'dead' | 'shop' | 'alive' | 'unknown';
export interface TimelinePoint {
  t: number | null; wall: number | null; area: TimelineArea; hp: number | null; souls: number | null; nw: number | null; allyNw: number | null; enemyNw: number | null;
  enemiesDead: number | null; midboss: boolean | null; urn: string | null; banner: { name: string; own: boolean | null; hp: number | null } | null; effects: string[] | null;
  /** The zone label under the minimap says the enemy base (also when your marker is read). */
  enemyBase?: boolean;
}
export interface DeathPanel { t: number; rows: Array<{ source: string; damage: number }>; killingBlow: string | null; total: number | null; effectsBefore: string[] }
export interface FightEpisode { t: number; status: 'joined' | 'missed' | 'dead' }
export interface ReviewCueText { clock: number | null; key: string; text: string }
export interface TimelineData {
  points: TimelinePoint[]; panels: DeathPanel[]; fights: FightEpisode[]; cues: ReviewCueText[];
  kda: { kills: number | null; deaths: number | null; assists: number | null; at: number | null } | null;
  /** Hero ids read on the scoreboard / top bar, with how many frames read each (the first frames of a match can still show the previous match's heroes). */
  enemies: Record<string, number>; allies: Record<string, number>;
}
const REVIEW_CUE = /^(death:review|review:pattern)(:|$)/;
const MAX_POINTS = 8000;

export class MatchTimeline {
  private d: TimelineData = MatchTimeline.empty();
  private fightLast: number | null = null;
  private lastClock: number | null = null;
  static empty(): TimelineData { return { points: [], panels: [], fights: [], cues: [], kda: null, enemies: {}, allies: {} }; }
  reset() { this.d = MatchTimeline.empty(); this.fightLast = null; this.lastClock = null; }
  get size() { return this.d.points.length; }
  /** One analyzed frame (in order). `cues` = the rule coach's cues of this frame (death reviews and pattern texts are kept). */
  add(s: Snapshot, wall: number | null, cues: Cue[] = []) {
    const read = finite(s.game_clock_secs) && s.game_clock_secs >= 0 && s.game_clock_secs <= 14400 ? Math.round(s.game_clock_secs) : null;
    // Between clock reads (death screen, shop) the clock moves with wall time when known.
    const prev = this.d.points.at(-1);
    const t = read ?? (prev?.t != null && prev.wall != null && wall != null ? Math.round(prev.t + Math.max(0, wall - prev.wall) / 1000) : prev?.t ?? null);
    if (read != null) this.lastClock = read;
    const map = s.minimap || null; const you = map?.you && finite(map.you.x) && finite(map.you.y) ? map.you : null;
    const dead = s.dead === true || s.screen === 'Dead';
    let area: TimelineArea = 'unknown';
    if (dead) area = 'dead'; else if (s.screen === 'Shop') area = 'shop';
    else if (s.screen === 'Gameplay') {
      // Your minimap marker first (as the profile reads it), else the zone label under the minimap.
      const z = map?.zone_area;
      area = you ? (you.y > 0.86 ? 'base' : map?.your_lane ? 'lane' : 'jungle')
        : z === 'Lane' ? 'lane' : z === 'Jungle' || z === 'Mid' ? 'jungle' : z === 'OwnBase' || z === 'Base' ? 'base' : z === 'EnemyBase' ? 'enemy_base' : 'alive';
    }
    const team = s.hero_status?.length === 12 ? s.hero_status : null;
    const b = s.objective_banner;
    const hp = finite(s.hp) && finite(s.max_hp) && s.max_hp >= 100 && s.hp >= 0 && s.hp <= s.max_hp ? Math.round(s.hp / s.max_hp * 100) / 100 : null;
    const p: TimelinePoint = { t, wall, area, hp, souls: finite(s.souls) && s.souls >= 0 ? s.souls : null, nw: finite(s.own_networth) ? s.own_networth : null,
      allyNw: finite(s.ally_networth) ? s.ally_networth : null, enemyNw: finite(s.enemy_networth) ? s.enemy_networth : null,
      enemiesDead: team ? team.filter(h => !h.ally && h.dead === true).length : null,
      midboss: typeof map?.midboss_up === 'boolean' ? map.midboss_up : null, urn: map?.urn ?? null,
      banner: b && typeof b.name === 'string' ? { name: b.name.slice(0, 30), own: typeof b.own === 'boolean' ? b.own : null, hp: finite(b.health_ratio) ? b.health_ratio : null } : null,
      effects: Array.isArray(s.status_effects) && s.status_effects.length ? s.status_effects.filter(e => typeof e === 'string').slice(0, 6) : null,
      ...(map?.zone_area === 'EnemyBase' && !dead ? { enemyBase: true } : {}) };
    this.d.points.push(p);
    if (this.d.points.length > MAX_POINTS) this.d.points.splice(0, this.d.points.length - MAX_POINTS);
    for (const [ids, votes] of [[s.enemy_heroes, this.d.enemies], [s.ally_heroes, this.d.allies]] as const)
      for (const id of new Set(ids || [])) if (typeof id === 'string' && /^hero_[a-z_]+$/.test(id) && (votes[id] != null || Object.keys(votes).length < 40)) votes[id] = (votes[id] || 0) + 1;
    if (finite(s.kills) || finite(s.deaths) || finite(s.assists)) {
      const o = this.d.kda; this.d.kda = { kills: finite(s.kills) ? s.kills : o?.kills ?? null, deaths: finite(s.deaths) ? s.deaths : o?.deaths ?? null, assists: finite(s.assists) ? s.assists : o?.assists ?? null, at: t };
    }
    if (dead && t != null) this.panel(s, t, prev?.area === 'dead');
    if (t != null) this.fight(s, t, dead, you);
    for (const c of cues) if (REVIEW_CUE.test(c.key) && !this.d.cues.some(x => x.key === c.key) && this.d.cues.length < 80)
      this.d.cues.push({ clock: t, key: c.key, text: c.text.replace(/\s+/g, ' ').trim().slice(0, 600) });
  }
  /** The damage panel of a death (the latest, fullest reading wins). */
  private panel(s: Snapshot, t: number, continuing: boolean) {
    const rows = (Array.isArray(s.death_damage) ? s.death_damage : []).filter(r => r && typeof r.source === 'string' && finite(r.damage))
      .map(r => ({ source: r.source.slice(0, 40), damage: Math.round(r.damage) })).sort((a, b) => b.damage - a.damage).slice(0, 4);
    let last = this.d.panels.at(-1);
    if (!last || !continuing && t - last.t > 20 || t - last.t > 90) {
      const before = this.d.points.filter(p => p.t != null && p.area !== 'dead' && p.t <= t && p.t >= t - 12).flatMap(p => p.effects || []);
      last = { t, rows: [], killingBlow: null, total: null, effectsBefore: [...new Set(before)].slice(0, 6) };
      this.d.panels.push(last); if (this.d.panels.length > 40) this.d.panels.shift();
    }
    if (rows.length >= last.rows.length && rows.length) last.rows = rows;
    if (typeof s.death_killing_blow === 'string' && s.death_killing_blow.trim()) last.killingBlow = s.death_killing_blow.trim().slice(0, 40);
    if (finite(s.death_total_damage) && s.death_total_damage > 0) last.total = Math.round(s.death_total_damage);
    if (!last.killingBlow && typeof s.death_cause === 'string' && s.death_cause.trim()) last.killingBlow = s.death_cause.trim().slice(0, 40);
  }
  /** Fight episodes as the profile counts them (review.rs pattern()): a cluster of >= 2 allies and
   *  >= 2 enemies near an ally; joined when you were within a third of the map at its start. */
  private fight(s: Snapshot, t: number, dead: boolean, you: { x: number; y: number } | null) {
    const live = (s.minimap?.heroes || []).filter(h => h.hits >= 2 && h.unseen_ms <= 3000);
    let at: number | null = null;
    for (const a of live.filter(h => h.ally)) {
      const allies = live.filter(o => o.ally && dist(o.pos, a.pos) <= REVIEW.fight_radius).length;
      const enemies = live.filter(e => !e.ally && dist(e.pos, a.pos) <= REVIEW.fight_radius).length;
      if (allies >= 2 && enemies >= 2 && you) { const d = dist(a.pos, you); if (at == null || d < at) at = d; }
    }
    if (at == null) return;
    if (this.fightLast == null || t - this.fightLast > LIMITS.fight_gap_secs) {
      this.d.fights.push({ t, status: dead ? 'dead' : at > REVIEW.missed_fight_distance ? 'missed' : 'joined' });
      if (this.d.fights.length > 120) this.d.fights.shift();
    }
    this.fightLast = t;
  }
  data(): TimelineData { return structuredClone(this.d); }
}

// ---------------------------------------------------------------------------------------------
// Match boundaries: a private profile segments matches like the Player profile does
// ---------------------------------------------------------------------------------------------
export interface EndedMatch { record: MatchRecord; timeline: TimelineData; byPostGame: boolean; source: MatchRecord['source'] }
export class ReviewTracker {
  private profile: PlayerProfile; private timeline = new MatchTimeline(); private source: MatchRecord['source'];
  constructor(source: MatchRecord['source'] = 'live', now: () => number = () => Date.now()) { this.source = source; this.profile = new PlayerProfile({ store: null, now }); }
  /** Feed one analyzed frame; returns the match that this frame ended, if any. */
  add(s: Snapshot, wall: number | null = null, cues: Cue[] = []): EndedMatch | null {
    if (!s || typeof s !== 'object') return null;
    const ev = this.profile.observe(s, wall, this.source);
    let ended: EndedMatch | null = null;
    // The frame that ended a match is not part of it: the timeline still holds the old match here.
    if (ev.ended) ended = { record: ev.ended, timeline: this.timeline.data(), byPostGame: resultOf(s.screen) != null, source: this.source };
    if (ev.ended || ev.matchStarted) this.timeline.reset();
    if (this.profile.current) this.timeline.add(s, wall, cues);
    return ended;
  }
  /** The match in progress so far (null without evidence). */
  current(): EndedMatch | null {
    const cur = this.profile.current; if (!cur?.hasEvidence) return null;
    return { record: cur.record(Date.now(), true), timeline: this.timeline.data(), byPostGame: false, source: this.source };
  }
  /** Close the match in progress (end of a replay). */
  finish(): EndedMatch | null {
    const timeline = this.timeline.data(); const rec = this.profile.endMatch(); this.timeline.reset();
    return rec ? { record: rec, timeline, byPostGame: false, source: this.source } : null;
  }
}
/** The last match of a replay (its snapshots in order, wall time unknown). */
export function lastMatchOf(frames: Array<{ snapshot: Snapshot; cues?: Cue[]; wall?: number | null }>, now: () => number = () => Date.now()): EndedMatch | null {
  const tr = new ReviewTracker('replay', now); let last: EndedMatch | null = null;
  for (const f of frames) { const e = tr.add(f.snapshot, f.wall ?? null, f.cues || []); if (e) last = e; }
  return tr.finish() ?? last;
}
/** Whether a finished match is reviewed automatically: a PostGame screen, or >= 10 min observed. */
export const autoReviewable = (m: EndedMatch) => m.record.deaths.length + spanOf(m.record) / 60 >= 3 && (m.byPostGame || spanOf(m.record) >= AUTO_MIN_SECS);
/** One review per match: the match id when read, otherwise the clock span, deaths and hero. */
// The "match id" read from the HUD repeats across matches (rec11 and rec12 both read 127139499),
// so it never identifies a match alone: hero, end minute and deaths go with it.
export function reviewKey(rec: MatchRecord): string {
  return `${rec.matchId ?? '?'}:${rec.hero ?? '?'}:${rec.endClock == null ? '?' : Math.round(rec.endClock / 60)}:${rec.deaths.length}`;
}
/** Is `a` the same match as `b` (a live match also stored in the profile, a replay added to it)? */
export function sameMatch(a: MatchRecord, b: MatchRecord) {
  if (a.id === b.id) return true;
  if (a.hero !== b.hero || a.startClock == null || b.startClock == null || a.endClock == null || b.endClock == null) return false;
  return Math.abs(a.startClock - b.startClock) <= 60 && Math.abs(a.endClock - b.endClock) <= 60 && Math.abs(a.deaths.length - b.deaths.length) <= 1;
}

// ---------------------------------------------------------------------------------------------
// Comparison with earlier matches
// ---------------------------------------------------------------------------------------------
interface Ratio { num: number; den: number }
interface Metric { id: string; label: string; traits: string[]; unit: '%' | '/10'; minAbs: number; f: (m: MatchRecord) => Ratio | null }
const alive = (m: MatchRecord) => m.time.lane + m.time.jungle + m.time.base;
export const METRICS: Metric[] = [
  { id: 'dead', label: 'Time dead', traits: ['dying'], unit: '%', minAbs: 0.03, f: m => spanOf(m) >= 300 ? { num: deadOf(m), den: spanOf(m) } : null },
  { id: 'per10', label: 'Deaths per 10 min', traits: ['dying'], unit: '/10', minAbs: 0.3, f: m => spanOf(m) >= 300 ? { num: m.deaths.length, den: spanOf(m) / 600 } : null },
  { id: 'unspent', label: 'Deaths holding half an item tier+ unspent', traits: ['unspent_souls'], unit: '%', minAbs: 0.1, f: m => { const d = m.deaths.filter(x => x.souls != null); return d.length ? { num: d.filter(x => x.souls! >= deathSpend(x.clock)).length, den: d.length } : null; } },
  { id: 'hoard', label: 'Alive time holding the usual buy or more', traits: ['unspent_souls'], unit: '%', minAbs: 0.05, f: m => m.souls.knownSecs >= 120 ? { num: m.souls.hoardSecs, den: m.souls.knownSecs } : null },
  { id: 'close', label: 'Deaths to close-range damage', traits: ['close_range'], unit: '%', minAbs: 0.1, f: m => m.deaths.length ? { num: m.deaths.filter(d => d.close === true).length, den: m.deaths.length } : null },
  { id: 'alone', label: 'Deaths with no ally nearby', traits: ['alone_deaths'], unit: '%', minAbs: 0.1, f: m => { const d = m.deaths.filter(x => x.alone != null); return d.length ? { num: d.filter(x => x.alone).length, den: d.length } : null; } },
  { id: 'respawn', label: `Deaths within ${REVIEW.respawn_window_secs} s of respawning`, traits: ['respawn_deaths'], unit: '%', minAbs: 0.1, f: m => m.deaths.length ? { num: m.deaths.filter(d => d.afterRespawnSecs != null && d.afterRespawnSecs <= REVIEW.respawn_window_secs).length, den: m.deaths.length } : null },
  { id: 'weak', label: 'Deaths after going back in under 60% health', traits: ['weak_entry'], unit: '%', minAbs: 0.1, f: m => { const d = m.deaths.filter(x => x.weakEntry != null); return d.length ? { num: d.filter(x => x.weakEntry).length, den: d.length } : null; } },
  { id: 'fights', label: 'Team fights missed', traits: ['missed_fights'], unit: '%', minAbs: 0.1, f: m => m.fights.joined + m.fights.missed ? { num: m.fights.missed, den: m.fights.joined + m.fights.missed } : null },
  { id: 'windows', label: 'Enemy-dead windows unused', traits: ['unused_windows'], unit: '%', minAbs: 0.15, f: m => m.windows.used + m.windows.unused ? { num: m.windows.unused, den: m.windows.used + m.windows.unused } : null },
  { id: 'jungle', label: 'Alive time in the jungle', traits: ['jungle_heavy'], unit: '%', minAbs: 0.05, f: m => alive(m) >= 300 ? { num: m.time.jungle, den: alive(m) } : null },
  { id: 'lowhp', label: 'Low-health moments that ended in a death', traits: ['low_hp'], unit: '%', minAbs: 0.1, f: m => m.lowHp.died + m.lowHp.survived ? { num: m.lowHp.died, den: m.lowHp.died + m.lowHp.survived } : null },
];
export type Verdict = 'improved' | 'worse' | 'same' | 'new';
export interface Comparison { id: string; label: string; now: string; before: string | null; verdict: Verdict; traits: string[]; value: number; previous: number | null }
const fmt = (m: Metric, v: number) => m.unit === '%' ? `${Math.round(v * 100)}%` : v.toFixed(1);
/** This match against the pooled earlier matches (newest 10). Lower is better for every metric. */
export function compare(rec: MatchRecord, previous: MatchRecord[], current: Partial<Record<string, Ratio>> = {}): Comparison[] {
  const prev = previous.filter(m => !sameMatch(m, rec)).slice(0, 10);
  const out: Comparison[] = [];
  for (const m of METRICS) {
    const r = current[m.id] ?? m.f(rec); if (!r || r.den <= 0) continue;
    const v = r.num / r.den;
    let num = 0, den = 0, n = 0; for (const p of prev) { const q = m.f(p); if (q && q.den > 0) { num += q.num; den += q.den; n++; } }
    const pv = den > 0 ? num / den : null;
    let verdict: Verdict = 'new';
    if (pv != null) { const d = v - pv; verdict = Math.abs(d) < m.minAbs || Math.abs(d) < 0.15 * Math.max(pv, m.unit === '%' ? 0.05 : 0.5) ? 'same' : d < 0 ? 'improved' : 'worse'; }
    const seconds = m.id === 'dead' || m.id === 'hoard' || m.id === 'jungle';
    out.push({ id: m.id, label: m.label, now: `${fmt(m, v)}${m.unit === '%' ? ` (${Math.round(r.num)}${seconds ? ' of ' : '/'}${Math.round(r.den)}${seconds ? ' s' : ''})` : ''}`,
      before: pv == null ? null : `${fmt(m, pv)} over ${n} earlier match${n === 1 ? '' : 'es'}`, verdict, traits: m.traits, value: v, previous: pv });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Digest
// ---------------------------------------------------------------------------------------------
export interface DigestInput {
  match: EndedMatch; previous?: MatchRecord[]; name?: (id: string) => string; docs?: Knowledge[];
  /** Rule-coach texts from the live cue log (death reviews, pattern summaries) when the timeline did not see them. */
  cues?: ReviewCueText[];
}
export interface Digest { text: string; chars: number; hero: string | null; heroName: string; result: string; minutes: number; deaths: number; comparisons: Comparison[]; key: string;
  /** What went well (the STRENGTHS section) and whether the match reads as a good game. */
  strengths: Strength[]; goodGame: boolean }
const defaultName = (id: string) => new PlayerProfileModel().heroName(id);
interface Section { title: string; lines: string[]; keep: number }

/** Result as observed; when the end screen did not say, the last Patron banner hints at it. */
function resultText(m: EndedMatch): { text: string; short: string } {
  if (m.record.result) return { text: m.record.result, short: m.record.result };
  const last = [...m.timeline.points].reverse().find(p => p.banner);
  const end = m.timeline.points.at(-1)?.t ?? null;
  if (last?.banner && last.t != null && end != null && end - last.t <= 120 && /king|mother|patron/i.test(last.banner.name) && last.banner.own != null) {
    const r = last.banner.own ? 'loss' : 'win';
    return { text: `not read (${last.banner.own ? "your team's" : "the enemy team's"} Patron health banner was up at ${clockText(last.t)}, near the end: likely a ${r})`, short: `likely ${r}` };
  }
  return { text: 'not read', short: 'unknown' };
}
function nearest(points: TimelinePoint[], t: number, f: (p: TimelinePoint) => boolean, within = 45) {
  let best: TimelinePoint | null = null;
  for (const p of points) if (p.t != null && Math.abs(p.t - t) <= within && f(p) && (!best || Math.abs(p.t - t) < Math.abs(best.t! - t))) best = p;
  return best;
}
/** Seconds per area, each frame standing for the gap to the next (<= 10 s), split by phase. */
function areaShares(points: TimelinePoint[]) {
  const phases = [{ name: 'laning (0-8 min)', lo: 0, hi: 480 }, { name: 'mid game (8-20 min)', lo: 480, hi: 1200 }, { name: 'late game (20+ min)', lo: 1200, hi: Infinity }];
  const acc = phases.map(() => ({ lane: 0, jungle: 0, base: 0, enemy_base: 0, dead: 0, shop: 0, alive: 0, unknown: 0 } as Record<TimelineArea, number>));
  for (let i = 0; i + 1 < points.length; i++) {
    const a = points[i], b = points[i + 1]; if (a.t == null) continue;
    const dt = Math.max(0, Math.min(LIMITS.sample_cap_secs, a.wall != null && b.wall != null ? (b.wall - a.wall) / 1000 : b.t != null ? b.t - a.t : 0));
    const ph = phases.findIndex(p => a.t! >= p.lo && a.t! < p.hi); if (ph >= 0) acc[ph][a.area] += dt;
  }
  return phases.map((p, i) => ({ name: p.name, ...acc[i] }));
}
/** Alive HP reads in the window before a death (the death screen's clock). */
function hpBefore(points: TimelinePoint[], clock: number, secs: number) {
  const end = [...points].reverse().find(p => p.t != null && p.t <= clock && p.area !== 'dead' && p.area !== 'unknown' && p.area !== 'shop' && p.t >= clock - 30)?.t;
  if (end == null) return [];
  return points.filter(p => p.t != null && p.hp != null && p.area !== 'dead' && p.t <= end && p.t >= end - secs) as Array<TimelinePoint & { t: number; hp: number }>;
}
/** From the healthiest read (>= 30%) in the 15 s before a death to the death screen. */
function hpFall(points: TimelinePoint[], clock: number): { from: number; secs: number } | null {
  const xs = hpBefore(points, clock, REVIEW.death_window_secs); if (!xs.length) return null;
  const peak = Math.max(...xs.map(p => p.hp)); const h = xs.find(p => p.hp >= LIMITS.burst_from && p.hp >= peak - 0.03);
  return h ? { from: Math.round(h.hp * 100), secs: Math.max(2, Math.round(clock - h.t)) } : null;
}
function hpLowBefore(points: TimelinePoint[], clock: number): string | null {
  const xs = hpBefore(points, clock, LIMITS.weak_entry_secs);
  return xs.length >= 3 && xs.every(p => p.hp < LIMITS.weak_entry_ratio) ? `under ${LIMITS.weak_entry_ratio * 100}% health for the ${LIMITS.weak_entry_secs} s before` : null;
}
/** Most-read heroes (stale reads from the previous match's first frames drop out). */
function topHeroes(votes: Record<string, number>): string[] {
  const xs = Object.entries(votes).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])); const max = xs[0]?.[1] ?? 0;
  return xs.filter(([, n]) => n >= Math.max(2, max * 0.2)).slice(0, 6).map(([id]) => id);
}
/** Runs of one value lasting >= `minPts` frames. */
function runs<T>(points: TimelinePoint[], f: (p: TimelinePoint) => T | null, minPts = 2): Array<{ v: T; from: number; to: number }> {
  const out: Array<{ v: T; from: number; to: number; n: number }> = [];
  for (const p of points) {
    if (p.t == null) continue; const v = f(p); if (v == null) continue;
    const last = out.at(-1);
    if (last && last.v === v && p.t - last.to <= 30) { last.to = p.t; last.n++; } else out.push({ v, from: p.t, to: p.t, n: 1 });
  }
  return out.filter(r => r.n >= minPts);
}

/** A strength of the match with its evidence (and its measurable effect when there is one). */
export interface Strength { id: string; text: string }
/** What went well, from the same signals the live coach reinforces (ai-coach.ts GoodPlay) plus
 *  the whole-match facts: K/D/A, time dead, deathless stretches, enemy-dead windows used to push,
 *  the urn delivered, death-screen shopping, the team souls lead, Patron pressure, low-health
 *  resets, fights joined and the metrics that improved on earlier matches. Observed facts only. */
export function strengthsOf(match: EndedMatch, comparisons: Comparison[] = []): Strength[] {
  const rec = match.record, pts = match.timeline.points, out: Strength[] = [];
  const span = spanOf(rec), dead = deadOf(rec), deadPct = span > 0 ? dead / span : null;
  const first = pts.find(p => p.t != null)?.t ?? rec.startClock ?? null, end = rec.endClock ?? [...pts].reverse().find(p => p.t != null)?.t ?? null;
  const kda = match.timeline.kda;
  if (kda?.kills != null && kda.deaths != null && kda.assists != null) {
    const ratio = (kda.kills + kda.assists) / Math.max(1, kda.deaths);
    if (ratio >= 3) out.push({ id: 'kda', text: `K/D/A ${kda.kills}/${kda.deaths}/${kda.assists}: ${kda.kills + kda.assists} kills and assists for ${kda.deaths} death${kda.deaths === 1 ? '' : 's'} (ratio ${ratio.toFixed(1)}): big fight impact while handing the enemy few kill bounties.` });
  }
  const deadCmp = comparisons.find(c => c.id === 'dead');
  if (deadPct != null && span >= 600 && (deadPct <= 0.12 || deadCmp?.verdict === 'improved'))
    out.push({ id: 'dead', text: `Dead only ${pct(dead, span)}% of the game (${Math.round(dead)} s of ${Math.round(span)} s, ${rec.deaths.length} death${rec.deaths.length === 1 ? '' : 's'})${deadCmp?.before ? `, against ${deadCmp.before}` : ''}: more time on the map for farm, waves and fights.` });
  // Deathless stretches of 5+ minutes (first frame → death, respawn → death, respawn → end).
  if (first != null && end != null) {
    const runs: Array<[number, number]> = []; let from = first;
    for (const d of rec.deaths) { if (d.clock - from >= 300) runs.push([from, d.clock]); from = d.clock + (d.deadSecs ?? 20); }
    if (end - from >= 300) runs.push([from, end]);
    if (runs.length) out.push({ id: 'deathless', text: `Deathless stretches: ${runs.map(([a, b]) => `${clockText(a)}-${clockText(b)} (${Math.round((b - a) / 6) / 10} min)`).join(', ')}.` });
  }
  // Enemy-dead windows used to push: 2+ enemies dead while you were alive in the enemy base or at an enemy objective.
  {
    const pushes: string[] = []; let i = 0, lastPush = -Infinity;
    while (i < pts.length) {
      if ((pts[i].enemiesDead ?? 0) < 2 || pts[i].t == null) { i++; continue; }
      let j = i, max = 0, at: TimelinePoint | null = null;
      while (j < pts.length && ((pts[j].enemiesDead ?? 2) >= 2 || pts[j].enemiesDead == null && j - i < 3)) {
        const p = pts[j]; max = Math.max(max, p.enemiesDead ?? 0);
        if (!at && p.area !== 'dead' && (p.enemyBase || p.area === 'enemy_base' || p.banner?.own === false)) at = p;
        j++;
      }
      if (at?.t != null && at.t - lastPush >= 60) {
        lastPush = at.t;
        const before = [...pts.slice(0, i + 1)].reverse().find(p => p.allyNw != null && p.enemyNw != null);
        const after = pts.slice(j).find(p => p.allyNw != null && p.enemyNw != null && p.t != null && p.t >= (pts[j - 1].t ?? 0));
        const gain = before && after ? (after.allyNw! - after.enemyNw!) - (before.allyNw! - before.enemyNw!) : null;
        const label = `${clockText(at.t)} (${max} enemies dead${at.banner?.own === false ? `, ${at.banner.name} under attack` : ', you in the enemy base'})${gain != null && gain >= 1000 ? `, team souls lead +${k(gain)} across the window` : ''}`;
        pushes.push(label);
      }
      i = Math.max(j, i + 1);
    }
    if (pushes.length) out.push({ id: 'windows', text: `Enemy-dead windows turned into pushes: ${pushes.slice(0, 5).join('; ')}: dead enemies cannot defend, so a structure or base push is free value.` });
  }
  // The Soul Urn delivered: carried by you, then a souls jump of 1,000+ within 25 s.
  {
    const delivered: string[] = [];
    for (let i = 0; i + 1 < pts.length; i++) {
      if (pts[i].urn !== 'CarriedByYou' || pts[i + 1].urn === 'CarriedByYou' || pts[i].t == null) continue;
      const base = pts[i].souls; if (base == null) continue;
      const jump = pts.slice(i + 1).filter(p => p.t != null && p.t - pts[i].t! <= 25 && p.souls != null && p.area !== 'dead').map(p => p.souls! - base);
      const best = jump.length ? Math.max(...jump) : 0;
      if (best >= 1000) delivered.push(`${clockText(pts[i].t)} (+${grouped(best)} souls)`);
    }
    if (delivered.length) out.push({ id: 'urn', text: `Delivered the Soul Urn: ${delivered.join(', ')}: team souls and a secured objective.` });
  }
  // Shopping on the death screen (souls at death vs the first read after respawning).
  {
    const shops: string[] = [];
    for (const d of rec.deaths) {
      const deadPts = pts.filter(p => p.t != null && p.t >= d.clock - 2 && p.t <= d.clock + (d.deadSecs ?? 30) + 5);
      const peak = Math.max(d.souls ?? 0, ...pts.filter(p => p.t != null && p.t <= d.clock && p.t >= d.clock - 10 && p.souls != null).map(p => p.souls!), ...deadPts.filter(p => p.area === 'dead' && p.souls != null).slice(0, 2).map(p => p.souls!));
      const afterRespawn = pts.find(p => p.t != null && p.t > d.clock + 3 && p.area !== 'dead' && p.area !== 'unknown' && p.area !== 'shop' && p.souls != null);
      const low = Math.min(...deadPts.filter(p => p.souls != null).map(p => p.souls!), afterRespawn?.souls ?? Infinity);
      const spent = Number.isFinite(low) ? peak - low : 0;
      if (spent >= Math.max(800, deathSpend(d.clock)) && afterRespawn?.souls != null && afterRespawn.souls <= peak - Math.max(800, deathSpend(d.clock)) + 400)
        shops.push(`after the ${clockText(d.clock)} death about ${k(Math.round(spent / 100) * 100)} spent`);
    }
    if (shops.length) out.push({ id: 'shop', text: `Spent on the death screen: ${shops.join('; ')}: you walked back out stronger instead of carrying the souls.` });
  }
  // Team souls lead: since when it stayed positive, its peak and the last read.
  {
    const nw = pts.filter(p => p.t != null && p.allyNw != null && p.enemyNw != null).map(p => ({ t: p.t!, d: p.allyNw! - p.enemyNw! }));
    if (nw.length >= 10 && nw.at(-1)!.d > 0) {
      let since = nw.length - 1; while (since > 0 && nw[since - 1].d > 0) since--;
      const peak = nw.slice(since).reduce((a, b) => b.d > a.d ? b : a);
      if (nw.at(-1)!.t - nw[since].t >= 300) out.push({ id: 'lead', text: `Team souls lead from ${clockText(nw[since].t)} to the end: +${k(nw[since].d)} then, ${peak.d - nw.at(-1)!.d >= 1000 ? `peak +${k(peak.d)} at ${clockText(peak.t)}, ` : ''}+${k(nw.at(-1)!.d)} at the last read ${clockText(nw.at(-1)!.t)}: the team kept converting its fights into souls.` });
    }
  }
  // Pressure on the enemy Patron.
  {
    const patron = pts.filter(p => p.banner && p.banner.own === false && /king|mother|patron/i.test(p.banner.name) && p.t != null);
    if (patron.length >= 2) out.push({ id: 'patron', text: `Pressure on the enemy Patron (${patron[0].banner!.name}) ${clockText(patron[0].t)}-${clockText(patron.at(-1)!.t)}: the game-ending objective.` });
  }
  if (rec.lowHp.episodes >= 2 && rec.lowHp.survived >= rec.lowHp.died * 2 && rec.lowHp.survived >= 2)
    out.push({ id: 'lowhp', text: `Survived ${rec.lowHp.survived} of ${rec.lowHp.episodes} moments under 30% health: you got out and reset instead of dying.` });
  if (rec.fights.joined >= 1 && rec.fights.missed === 0) out.push({ id: 'fights', text: `Joined every team fight observed (${rec.fights.joined}, none missed).` });
  const improved = comparisons.filter(c => c.verdict === 'improved' && c.before && !['dead', 'close'].includes(c.id));
  if (improved.length) out.push({ id: 'improved', text: `Better than your earlier matches: ${improved.map(c => `${c.label.toLowerCase()} ${c.now.replace(/ \(.*\)$/, '')} vs ${c.before!.replace(/ over .*/, '')}`).join('; ')}.` });
  return out;
}

export function buildDigest(input: DigestInput, maxChars = DIGEST_MAX_CHARS): Digest {
  const { match } = input; const rec = match.record; const tl = match.timeline; const pts = tl.points;
  const name = input.name || defaultName;
  const heroName = rec.hero ? name(rec.hero) : 'unknown hero';
  const span = spanOf(rec), deadS = deadOf(rec);
  const res = resultText(match);
  const secs: Section[] = [];
  const first = pts.find(p => p.t != null)?.t ?? rec.startClock, last = rec.endClock ?? [...pts].reverse().find(p => p.t != null)?.t ?? null;

  // Header
  const head: string[] = [];
  head.push(`Hero: ${heroName}${rec.hero ? ` (${rec.hero})` : ''}. Result: ${res.text}. Game clock ${clockText(first)} to ${clockText(last)}, ${Math.round(span / 6) / 10} min observed (${rec.source}${rec.partial ? ', partial' : ''}).`);
  if (tl.kda) head.push(`K/D/A (scoreboard${tl.kda.at != null ? ` read at ${clockText(tl.kda.at)}` : ''}): ${tl.kda.kills ?? '?'}/${tl.kda.deaths ?? '?'}/${tl.kda.assists ?? '?'}.`);
  else head.push('K/D/A: not read.');
  if (rec.hero) head.push(`Role: ${HERO_ROLES[rec.hero] ?? "not in the coach's table (do not guess the kit)"}.`);
  const allies = topHeroes(tl.allies), enemies = topHeroes(tl.enemies);
  if (allies.length || enemies.length) head.push(`Allies: ${allies.map(name).join(', ') || '?'}. Enemies: ${enemies.map(name).join(', ') || '?'}.`);
  secs.push({ title: 'MATCH', lines: head, keep: 99 });

  // Strengths first: the review leads with what went well and why it worked.
  const previous = (input.previous || []).filter(m => !sameMatch(m, rec));
  const comparisons = compare(rec, previous);
  const strengths = strengthsOf(match, comparisons);
  const goodGame = /win/.test(res.short) || (span >= 600 && deadS / span <= 0.12 && rec.deaths.length <= 3);
  secs.push({ title: `STRENGTHS (${strengths.length}: what went well, with evidence)`, keep: 99, lines: strengths.length
    ? [...strengths.map((s, i) => `- ${s.text}`), ...(goodGame ? [`Verdict: a good game (${res.short === 'unknown' ? `dead ${pct(deadS, span)}%, ${rec.deaths.length} deaths` : res.short}). Lead with these strengths and why each worked; give at least as many went_well items as improvements.`] : [])]
    : ['No clear strength observed: do not invent praise.'] });

  // Economy
  const eco: string[] = [];
  const marks: string[] = [];
  if (last != null) for (let t = 300; t <= last + 30; t += 300) {
    const p = nearest(pts, t, q => q.allyNw != null && q.enemyNw != null);
    if (p) marks.push(`${clockText(t)} ${k(p.allyNw!)} vs ${k(p.enemyNw!)} (${p.allyNw! >= p.enemyNw! ? '+' : '-'}${k(Math.abs(p.allyNw! - p.enemyNw!))})`);
  }
  const lastNw = [...pts].reverse().find(q => q.allyNw != null && q.enemyNw != null);
  if (lastNw?.t != null && !marks.some(m => m.startsWith(clockText(Math.round(lastNw.t! / 300) * 300)) && Math.abs(lastNw.t! - Math.round(lastNw.t! / 300) * 300) < 45))
    marks.push(`last read ${clockText(lastNw.t)} ${k(lastNw.allyNw!)} vs ${k(lastNw.enemyNw!)} (${lastNw.allyNw! >= lastNw.enemyNw! ? '+' : '-'}${k(Math.abs(lastNw.allyNw! - lastNw.enemyNw!))})`);
  eco.push(marks.length ? `Team souls, your team vs enemy: ${marks.join('; ')}.` : 'Team souls: not read.');
  const own = [600, 1200, 1800].map(t => { const p = nearest(pts, t, q => q.nw != null, 60); return p ? `${clockText(t)} ${k(p.nw!)}` : null; }).filter(Boolean);
  if (own.length) eco.push(`Your net worth: ${own.join(', ')}${rec.farm.perMin != null ? `; about ${grouped(rec.farm.perMin)} souls/min` : ''}.`);
  // Souls held vs the usual buy for the game time.
  const tiers = [{ lo: 0, hi: 600, tier: 1600 }, { lo: 600, hi: 1200, tier: 3200 }, { lo: 1200, hi: Infinity, tier: 6400 }].map(x => {
    let peak = 0, over = 0, peakAt: number | null = null, known = 0;
    for (let i = 0; i + 1 < pts.length; i++) {
      const a = pts[i], b = pts[i + 1];
      if (a.t == null || a.t < x.lo || a.t >= x.hi || a.souls == null || a.area === 'dead' || a.area === 'unknown') continue; // alive frames only
      const dt = Math.max(0, Math.min(10, a.wall != null && b.wall != null ? (b.wall - a.wall) / 1000 : b.t != null ? b.t - a.t : 0));
      known += dt; if (a.souls >= spendTarget(a.t)) over += dt; if (a.souls > peak) { peak = a.souls; peakAt = a.t; }
    }
    return known >= 30 ? `${x.hi === Infinity ? '20:00+' : `${clockText(x.lo)}-${clockText(x.hi)}`} (usual buy ${grouped(x.tier)}): peak unspent ${grouped(peak)}${peakAt != null ? ` at ${clockText(peakAt)}` : ''}, ${Math.round(over)} s at or over the usual buy` : null;
  }).filter(Boolean);
  eco.push(tiers.length ? `Unspent souls vs the usual item tier: ${tiers.join('; ')}.` : 'Unspent souls: not read.');
  secs.push({ title: 'ECONOMY', lines: eco, keep: 3 });

  // Time
  const time: string[] = [];
  time.push(`Dead ${pct(deadS, span)}% of the observed game (${Math.round(deadS)} s of ${Math.round(span)} s) over ${rec.deaths.length} death${rec.deaths.length === 1 ? '' : 's'}.`);
  const shares = areaShares(pts).map(p => {
    const tot = p.lane + p.jungle + p.base + p.enemy_base + p.dead + p.shop + p.alive + p.unknown; if (tot < 60) return null;
    const parts = ([['lane', p.lane], ['jungle', p.jungle], ['own base', p.base], ['enemy base', p.enemy_base], ['shop', p.shop], ['dead', p.dead], ['alive, place not read', p.alive], ['menus or unread', p.unknown]] as Array<[string, number]>)
      .filter(([, v]) => pct(v, tot) >= 1).map(([n, v]) => `${n} ${pct(v, tot)}%`);
    return `${p.name}: ${parts.join(', ')}`;
  }).filter(Boolean);
  if (shares.length) time.push(`Time by phase: ${shares.join('; ')}.`);
  if (rec.deepAwaySecs >= 30) time.push(`A wave sat deep on your side for ${rec.deepAwaySecs} s while you were in the jungle.`);
  if (rec.lowHp.episodes) time.push(`Under 30% health ${rec.lowHp.episodes} times: survived ${rec.lowHp.survived}, died ${rec.lowHp.died}.`);
  secs.push({ title: 'TIME', lines: time, keep: 2 });

  // Deaths
  const deaths: string[] = [];
  const cues = [...tl.cues, ...(input.cues || []).filter(c => !tl.cues.some(x => x.key === c.key))];
  let closeDeaths = 0;
  rec.deaths.forEach((d, i) => {
    const panel = tl.panels.filter(p => Math.abs(p.t - d.clock) <= 30).sort((a, b) => Math.abs(a.t - d.clock) - Math.abs(b.t - d.clock))[0];
    const parts: string[] = [];
    const unread: string[] = [];
    const yn = (label: string, v: boolean | null) => { if (v == null) unread.push(label); else parts.push(`${label} ${v ? 'yes' : 'no'}`); };
    parts.push(`cause ${d.cause ? `"${d.cause}"` : '?'}${d.family && d.family !== d.cause && !d.cause?.toLowerCase().startsWith(d.family.toLowerCase()) ? ` (${d.family}${d.killer ? `, ${name(d.killer)}` : ''})` : ''}`);
    if (panel?.rows.length) parts.push(`damage panel: ${panel.rows.map(r => `${r.source} ${grouped(r.damage)}`).join(', ')}${panel.total ? ` (total ${grouped(panel.total)})` : ''}`);
    if (panel?.killingBlow && panel.killingBlow !== d.cause) parts.push(`killing blow ${panel.killingBlow}`);
    // Melee / Rend / Aura on the panel mean close range even when the top line read is "Gun Damage".
    const sum = panel?.rows.reduce((a, r) => a + r.damage, 0) ?? 0;
    const close = (panel?.rows || []).filter(r => killerFamily(r.source)?.close === true);
    const closeByPanel = sum > 0 && close.reduce((a, r) => a + r.damage, 0) >= 0.25 * sum;
    if (d.close === true || closeByPanel) closeDeaths++;
    if (closeByPanel) parts.push(`close-range damage on the panel: ${pct(close.reduce((a, r) => a + r.damage, 0), sum)}% (${[...new Set(close.map(r => r.source))].join(', ')})`);
    const fx = [...new Set([...(d.effects || []), ...(panel?.effectsBefore || [])])];
    parts.push(`CC/status before: ${fx.length ? fx.join(', ') : 'none read'}`);
    const fall = d.fromPct != null ? { from: d.fromPct, secs: d.fallSecs ?? 0 } : hpFall(pts, d.clock);
    if (fall) parts.push(`HP ${fall.from}% to dead in ${fall.secs} s${fall.secs <= LIMITS.burst_secs ? ' (burst)' : ''}`); else unread.push('HP fall');
    const lowBefore = hpLowBefore(pts, d.clock); if (lowBefore) parts.push(lowBefore);
    yn('alone', d.alone); yn('forward with 3+ enemies unseen', d.forwardUnseen);
    parts.push(d.afterRespawnSecs != null ? `${d.afterRespawnSecs} s after respawning` : 'first life or respawn unseen');
    if (d.weakEntry) parts.push('went back in under 60% health');
    if (d.teammatesDied) parts.push(`${d.teammatesDied} teammate${d.teammatesDied === 1 ? '' : 's'} died just before`);
    if (d.souls == null) unread.push('souls held'); else parts.push( `held ${grouped(d.souls)} souls (usual buy now ${grouped(spendTarget(d.clock))}${d.souls >= deathSpend(d.clock) ? ', half a tier or more unspent' : ''})`);
    if (d.deadSecs != null) parts.push(`dead ${d.deadSecs} s`);
    if (unread.length) parts.push(`not read: ${unread.join(', ')}`);
    deaths.push(`${i + 1}. ${clockText(d.clock)}: ${parts.join('; ')}.`);
    const review = cues.find(c => /^death:review:/.test(c.key) && Math.abs(Number(c.key.split(':')[2]) - d.clock) <= 25);
    if (review) deaths.push(`   Rule coach said: "${clip(review.text.replace(/^Why you died:\s*/i, ''), Math.max(160, Math.min(330, Math.floor(1400 / rec.deaths.length))))}"`);
  });
  if (!deaths.length) deaths.push('No deaths observed.');
  secs.push({ title: `DEATHS (${rec.deaths.length})`, lines: deaths, keep: 99 });

  // Fights and windows
  const fl: string[] = [];
  const by = (s: FightEpisode['status']) => tl.fights.filter(f => f.status === s).map(f => clockText(f.t));
  const list = (xs: string[]) => xs.length ? `${xs.slice(0, 12).join(', ')}${xs.length > 12 ? ` and ${xs.length - 12} more` : ''}` : 'none';
  fl.push(`Team fights (2+ allies vs 2+ enemies on the minimap): joined ${rec.fights.joined}, missed ${rec.fights.missed} (more than a third of the map away), while dead ${rec.fights.whileDead}.`);
  if (tl.fights.length) fl.push(`Joined at: ${list(by('joined'))}. Missed at: ${list(by('missed'))}. While dead: ${list(by('dead'))}.`);
  fl.push(`Enemy-dead windows (2+ enemies dead for 20+ s while you were alive): used ${rec.windows.used}, unused ${rec.windows.unused}${rec.windows.unusedAt.length ? ` (unused at ${rec.windows.unusedAt.map(c => clockText(c)).join(', ')})` : ''}.`);
  secs.push({ title: 'FIGHTS AND WINDOWS', lines: fl, keep: 3 });

  // Objectives
  const ob: string[] = [];
  const mb = runs(pts, p => p.midboss, 3);
  const gone = mb.filter((r, i) => i > 0 && r.v === false && mb[i - 1].v === true && r.to - r.from >= 20).map(r => clockText(r.from));
  if (mb.length) ob.push(gone.length ? `Midboss icon disappeared (taken; by which team not read) at ${gone.join(', ')}.` : 'Midboss: no take observed.');
  const urn = runs(pts, p => /^Carried/.test(p.urn || '') ? p.urn : null, 2).map(r => `${r.v === 'CarriedByYou' ? 'you' : r.v === 'CarriedByAlly' ? 'an ally' : 'an enemy'} ${clockText(r.from)}-${clockText(r.to)}`);
  ob.push(urn.length ? `Soul Urn carried by: ${urn.slice(0, 8).join('; ')}.` : 'Soul Urn: no carry observed.');
  const banners = runs(pts, p => p.banner ? `${p.banner.name}|${p.banner.own}` : null, 2).map(r => {
    const [bn, ownS] = r.v.split('|'); const hp = pts.filter(p => p.t != null && p.t >= r.from && p.t <= r.to && p.banner?.hp != null).map(p => p.banner!.hp!);
    const lo = hp.length ? Math.min(...hp.slice().sort((a, b) => a - b).slice(Math.min(1, hp.length - 1))) : null; // skip one misread minimum
    return `${bn} (${ownS === 'true' ? 'your' : ownS === 'false' ? 'enemy' : '?'} objective) ${clockText(r.from)}-${clockText(r.to)}${lo != null ? `, down to ${Math.round(lo * 100)}%` : ''}`;
  });
  if (banners.length) ob.push(`Boss health banner (a Patron under attack): ${banners.slice(-4).join('; ')}.`);
  secs.push({ title: 'OBJECTIVES SEEN', lines: ob, keep: 3 });

  // Rule coach patterns
  const pat = cues.filter(c => /^review:pattern/.test(c.key)).map(c => `[${clockText(c.clock)}] ${clip(c.text, 220)}`);
  // Repeated "Last 5 minutes: you died ..." lines say the same: keep the first and the last.
  const dedup = pat.filter((p, i) => !/^\[[^\]]*\] Last 5 minutes: you died/.test(p) || i === pat.findIndex(x => /^\[[^\]]*\] Last 5 minutes: you died/.test(x)) || i === pat.length - 1 - [...pat].reverse().findIndex(x => /^\[[^\]]*\] Last 5 minutes: you died/.test(x)));
  if (dedup.length) secs.push({ title: 'RULE COACH PATTERN CALLS', lines: dedup.slice(-8), keep: 2 });

  // Comparison
  const recordClose = rec.deaths.filter(d => d.close === true).length, closeUnclear = closeDeaths !== recordClose;
  const cmp: string[] = [];
  if (!previous.length) cmp.push('No earlier matches in the profile: nothing to compare.');
  else {
    const sameHero = previous.filter(m => m.hero === rec.hero).length;
    cmp.push(`${previous.length} earlier match${previous.length === 1 ? '' : 'es'} in the profile (${sameHero} on ${heroName}); newest 10 pooled. Lower is better for every line.`);
    for (const c of comparisons) if (!(c.value === 0 && c.previous === 0)) cmp.push(`- ${c.label}: ${c.now}${c.before ? ` vs ${c.before}` : ''} -> ${c.verdict === 'new' ? 'no earlier data' : c.id === 'close' && closeUnclear ? 'not comparable (see note)' : c.verdict}`);
    if (closeUnclear) cmp.push(`  Note: counted from the killing cause, as for earlier matches, ${recordClose} of ${rec.deaths.length} deaths were close range; the damage panels show close-range damage (melee, Rend, auras) in ${closeDeaths} of ${rec.deaths.length}.`);
    const model = new PlayerProfileModel(); if (input.name) model.heroName = (id: string | null | undefined) => id ? name(id) : 'Unknown hero';
    const before = model.summary(previous).weaknesses.slice(0, 5);
    if (before.length) cmp.push(`Weaknesses the profile knew before this match: ${before.map(t => {
      const c = comparisons.filter(x => x.traits.includes(t.id) && x.verdict !== 'new' && !(x.id === 'close' && closeUnclear));
      const v = c.length ? (c.every(x => x.verdict === 'improved') ? 'improved' : c.some(x => x.verdict === 'worse') ? (c.some(x => x.verdict === 'improved') ? 'mixed' : 'worse') : c.some(x => x.verdict === 'improved') ? 'improved' : 'same') : 'not measurable this match';
      return `${t.label} (${v})`;
    }).join('; ')}.`);
    const improved = comparisons.filter(c => c.verdict === 'improved').length, worse = comparisons.filter(c => c.verdict === 'worse').length;
    cmp.push(`Overall vs earlier matches: ${improved} improved, ${worse} worse, ${comparisons.length - improved - worse} same or new.`);
  }
  secs.push({ title: 'COMPARED WITH EARLIER MATCHES', lines: cmp, keep: 99 });

  // Knowledge
  if (input.docs?.length) {
    const terms: string[] = [];
    const worst = comparisons.filter(c => c.value >= 0.3 || c.verdict === 'worse').map(c => c.id);
    if (worst.includes('alone') || worst.includes('respawn')) terms.push('alone', 'regroup', 'respawn');
    if (worst.includes('unspent') || worst.includes('hoard')) terms.push('unspent', 'spend', 'shop');
    if (worst.includes('windows')) terms.push('objective', 'walker', 'push');
    if (worst.includes('jungle')) terms.push('jungle', 'camp', 'wave');
    if (worst.includes('close')) terms.push('range', 'dive', 'positioning');
    if (worst.includes('fights')) terms.push('rotations', 'group');
    if (rec.deaths.length >= 4 || pct(deadS, span) >= 12) terms.push('dying', 'dying', 'death');
    const ex: string[] = [];
    const matchup = heroMatchup(input.docs, rec.hero, enemies, name);
    if (matchup) ex.push(clip(matchup, 300));
    if (terms.length) for (const s of retrieve(sections(input.docs), terms, 3 - ex.length)) ex.push(`[${s.title} › ${s.heading}] ${clip(s.text, 320)}`);
    if (ex.length) secs.push({ title: 'REFERENCE EXCERPTS (knowledge base)', lines: ex.slice(0, 3), keep: 0 });
  }

  // Render within budget: drop lines from the end of the lowest-priority sections first.
  const render = () => ['MATCH DIGEST (read from the screen by the coach; "?" or "not read" = unknown, never guess it)', ...secs.filter(s => s.lines.length).map(s => `\n${s.title}\n${s.lines.join('\n')}`)].join('\n');
  let text = render();
  const order = [...secs].sort((a, b) => a.keep - b.keep);
  for (const s of order) { while (text.length > maxChars && s.lines.length > Math.min(s.keep, s.lines.length) ) { s.lines.pop(); text = render(); } }
  for (const s of order) { while (text.length > maxChars && s.lines.length > 1) { s.lines.pop(); text = render(); } }
  if (text.length > maxChars) text = text.slice(0, maxChars - 1) + '…';
  return { text, chars: text.length, hero: rec.hero, heroName, result: res.short, minutes: Math.round(span / 6) / 10, deaths: rec.deaths.length, comparisons, key: reviewKey(rec), strengths, goodGame };
}

// ---------------------------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------------------------
export const REVIEW_SYSTEM_PROMPT = `You are an elite Deadlock coach (Valve's 6v6 hero-shooter MOBA: three lanes with Guardians, Walkers, Shrines and the Patron; souls economy and item tiers of 800 / 1,600 / 3,200 / 6,400; jungle camps; the Midboss and its Rejuvenator; the Soul Urn; respawn timers that grow with game time). The player just finished a match and wants an honest, high-level review: what they did well and why it worked, and how to improve overall.

You get a MATCH DIGEST read from the screen by an automatic coach, not a video. Rules:
- Use only facts in the digest. Cite game-clock timestamps (m:ss) as evidence. Never invent kills, items, abilities, teammates' actions or events. Values marked "?" or "not read" are unknown: say so instead of guessing.
- Lead with what went well (STRENGTHS, and "improved" lines in COMPARED WITH EARLIER MATCHES). For each strength say WHY it worked: the cause and its effect on the team's result (the shape, not this match: "you pushed into their base while five were dead, and the souls lead grew right after: dead enemies cannot defend, so that push was free"). Reinforcing the habits that won the game is as important as fixing mistakes.
- Balance: after a good game (the digest's Verdict line, a win, or few deaths) give at least as many went_well items as improvements. Never invent praise: if STRENGTHS says none, say so honestly.
- Rank improvements by impact on winning: deaths and time dead first (every death is souls and map time for the enemy), then souls spending, then fight and objective presence (enemy-dead windows, Walkers, Midboss, Urn), then map and wave habits.
- Be specific to Deadlock and to the player's hero and the enemy heroes named in the digest. Prefer concrete habits ("after a respawn, walk out with the next teammate or catch the closest safe wave") over generic advice ("play safer").
- Praise real progress: when the comparison with earlier matches says improved, say what improved and by how much.
- Speak to the player as "you", direct and warm, like a coach after a game.

Reply with one JSON object only (no markdown, no code fences):
{"summary": "2-3 sentences: the verdict on this match, opening with what went well when it did",
 "went_well": ["what you did, with its evidence and timestamps, then why it worked: its effect on the team's result"],
 "improvements": [{"title": "short habit to fix", "evidence": "what happened, with timestamps", "why_it_matters": "the cost in Deadlock terms", "how_to_practice": "a concrete drill or rule for the next games"}],
 "keep_doing": ["a habit from this match to repeat every game"],
 "next_game_focus": "one sentence: the single thing to focus on next game",
 "hero_tips": ["tip for this hero in situations like the ones in the digest"],
 "spoken": "180-300 words of natural speech for text-to-speech"}
went_well: 2-5 items, strongest first. improvements: 1-3 by impact, most important first (after a good game, no more than went_well). keep_doing: 1-3 items. hero_tips: 1-3 items.
"spoken": plain sentences only, no lists, no markdown, no symbols or abbreviations like K/D/A; say times as "around fifteen minutes in" or "at 15:20"; round numbers. When the game went well, open with genuine, specific praise (what you did and why it mattered), never a generic "good game"; then the top fixes with one reason each; end with what to keep doing and the next-game focus.`;

export interface ReviewPrompt { system: string; user: string; chars: number; maxTokens: number }
export function buildReviewPrompt(digest: Digest | string, aiMaxTokens = 1024): ReviewPrompt {
  const text = typeof digest === 'string' ? digest : digest.text;
  const good = typeof digest !== 'string' && digest.goodGame;
  const user = [good ? 'Review this match for the player. It went well: open with the strengths and why they worked, then the fixes, then what to keep doing and the next focus.' : 'Review this match for the player: what went well and why, the fixes by impact, what to keep doing and the next focus.', '', text, '', 'Reply with the JSON object only.'].join('\n');
  return { system: REVIEW_SYSTEM_PROMPT, user, chars: REVIEW_SYSTEM_PROMPT.length + user.length, maxTokens: reviewMaxTokens(aiMaxTokens) };
}

// ---------------------------------------------------------------------------------------------
// Reply parsing (fenced, prefixed, or cut off)
// ---------------------------------------------------------------------------------------------
export interface Improvement { title: string; evidence: string; why_it_matters: string; how_to_practice: string }
export interface ReviewContent { summary: string; went_well: string[]; improvements: Improvement[];
  /** Habits from this match to repeat (older stored reviews have none). */
  keep_doing: string[]; next_game_focus: string; hero_tips: string[]; spoken: string; partial?: boolean }
export type ReviewParse = { ok: true; review: ReviewContent } | { ok: false; error: string };

/** Close an unfinished JSON text: open string, dangling key / comma, open brackets. */
export function closeJson(prefix: string): string {
  const stack: string[] = []; let inString = false, escaped = false;
  for (const c of prefix) {
    if (inString) { if (escaped) escaped = false; else if (c === '\\') escaped = true; else if (c === '"') inString = false; continue; }
    if (c === '"') inString = true; else if (c === '{' || c === '[') stack.push(c); else if (c === '}' || c === ']') stack.pop();
  }
  let s = prefix;
  if (inString) s = (escaped ? s.slice(0, -1) : s) + '"';
  for (let i = 0; i < 3; i++) {
    s = s.replace(/\s+$/, '');
    if (/,$/.test(s)) s = s.slice(0, -1);
    else if (/:$/.test(s)) s = s.replace(/,?\s*"(?:[^"\\]|\\.)*"\s*:$/, '');
    else if (stack.at(-1) === '{' && /[{,]\s*"(?:[^"\\]|\\.)*"$/.test(s)) s = s.replace(/,?\s*"(?:[^"\\]|\\.)*"$/, '');
    else break;
  }
  return s + stack.reverse().map(c => c === '{' ? '}' : ']').join('');
}
/** Parse a cut-off JSON object: the whole prefix closed, else cut back to earlier commas. */
export function repairJson(text: string): Record<string, unknown> | null {
  const start = text.indexOf('{'); if (start < 0) return null;
  let body = text.slice(start);
  for (let i = 0; i < 40; i++) {
    try { const v = JSON.parse(closeJson(body)); if (v && typeof v === 'object' && !Array.isArray(v)) return v; } catch { /* cut back */ }
    const cut = body.lastIndexOf(',', body.length - 2); if (cut <= 0) return null;
    body = body.slice(0, cut);
  }
  return null;
}
const str = (v: unknown, n = 600) => typeof v === 'string' ? v.replace(/\s+/g, ' ').trim().slice(0, n) : '';
const strs = (v: unknown, max: number, n = 400) => (Array.isArray(v) ? v : typeof v === 'string' && v.trim() ? [v] : [])
  .map(x => typeof x === 'string' ? str(x, n) : x && typeof x === 'object' ? str(Object.values(x as Record<string, unknown>).filter(y => typeof y === 'string').join(': '), n) : '').filter(Boolean).slice(0, max);
/** Speakable text: markdown, bullets and symbols removed; at most 420 words. */
export function speakable(text: string, maxWords = 420): string {
  const clean = text.replace(/```[\s\S]*?```/g, ' ').replace(/^\s*(?:[-*•]|\d+[.)])\s+/gm, '').replace(/[*_#`~>|]/g, '').replace(/\bK\/D\/A\b/g, 'kills, deaths and assists').replace(/\s+/g, ' ').trim();
  const words = clean.split(' ');
  if (words.length <= maxWords) return clean;
  const cut = words.slice(0, maxWords).join(' '); const end = Math.max(cut.lastIndexOf('. '), cut.lastIndexOf('! '), cut.lastIndexOf('? '));
  return end > cut.length / 2 ? cut.slice(0, end + 1) : cut + '.';
}
/** Sentences for sequential speech (long sentences split at commas/semicolons after ~28 words). */
export function sentencesOf(text: string): string[] {
  const out: string[] = [];
  for (const s of text.match(/[^.!?]+(?:[.!?]+(?=\s|$)|$)/g)?.map(x => x.trim()).filter(Boolean) || []) {
    if (s.split(' ').length <= 32) { out.push(s); continue; }
    let cur = '';
    for (const part of s.split(/(?<=[,;:])\s+/)) { if (cur && (cur + ' ' + part).split(' ').length > 28) { out.push(cur); cur = part; } else cur = cur ? `${cur} ${part}` : part; }
    if (cur) out.push(cur);
  }
  return out;
}
export function parseReview(raw: string): ReviewParse {
  const text = raw.replace(/<think>[\s\S]*?(<\/think>|$)/gi, '').replace(/```(?:json)?/gi, '').trim();
  const wanted = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v) && ['summary', 'spoken', 'improvements', 'went_well'].some(key => key in (v as object));
  let obj: Record<string, unknown> | null = null, partial = false;
  for (const c of jsonCandidates(text)) { try { const v = JSON.parse(c); if (wanted(v)) { obj = v; break; } } catch { /* next */ } }
  if (!obj) { const v = repairJson(text); if (wanted(v)) { obj = v; partial = true; } }
  if (!obj) return { ok: false, error: `Reply was not the requested JSON: "${text.replace(/\s+/g, ' ').slice(0, 120)}"` };
  const improvements: Improvement[] = (Array.isArray(obj.improvements) ? obj.improvements : []).slice(0, 3).map((x: unknown) => {
    if (typeof x === 'string') return { title: str(x, 160), evidence: '', why_it_matters: '', how_to_practice: '' };
    const o = x && typeof x === 'object' ? x as Record<string, unknown> : {};
    return { title: str(o.title, 160), evidence: str(o.evidence), why_it_matters: str(o.why_it_matters ?? o.why), how_to_practice: str(o.how_to_practice ?? o.practice ?? o.how) };
  }).filter(i => i.title || i.evidence);
  const r: ReviewContent = { summary: str(obj.summary, 900), went_well: strs(obj.went_well, 6), improvements, keep_doing: strs(obj.keep_doing ?? obj.keep, 4, 300), next_game_focus: str(obj.next_game_focus ?? obj.focus, 300),
    hero_tips: strs(obj.hero_tips, 4), spoken: speakable(typeof obj.spoken === 'string' ? obj.spoken : '') };
  if (!r.spoken) r.spoken = speakable([r.summary, r.went_well.length ? `What went well: ${r.went_well.join('. ')}.` : '', ...r.improvements.map((i, n) => `${['First', 'Second', 'Third'][n]}, ${i.title}. ${i.how_to_practice}`), r.keep_doing.length ? `Keep doing this: ${r.keep_doing.join('. ')}.` : '', r.next_game_focus ? `Next game: ${r.next_game_focus}` : ''].filter(Boolean).join(' '));
  if (!r.summary && !r.spoken && !r.improvements.length) return { ok: false, error: 'The review JSON had no summary, improvements or spoken text.' };
  if (partial) r.partial = true;
  return { ok: true, review: r };
}

// ---------------------------------------------------------------------------------------------
// Gateway call
// ---------------------------------------------------------------------------------------------
export interface ReviewResult { status: 'ok' | 'raw'; review: ReviewContent | null; raw: string; model: string; ms: number; error: string | null }
/** Ask the gateway for a review. A cut-off reply is retried once with 8,192 tokens; a reply that
 *  is still not usable JSON comes back as raw text (status 'raw'). Network/HTTP errors throw. */
export async function requestReview(cfg: AiConfig, prompt: ReviewPrompt, opts: Omit<CallOptions, 'maxTokens'> = {}): Promise<ReviewResult> {
  const started = Date.now();
  let tokens = prompt.maxTokens; let raw = ''; let model = cfg.model;
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const r = await callGateway(cfg, prompt.system, prompt.user, { timeoutMs: REVIEW_TIMEOUT_MS, trigger: 'review', ...opts, maxTokens: tokens });
      raw = r.text; model = r.model;
      const p = parseReview(raw);
      return p.ok ? { status: 'ok', review: p.review, raw, model, ms: Date.now() - started, error: null } : { status: 'raw', review: null, raw, model, ms: Date.now() - started, error: p.error };
    } catch (e) {
      if (!(e instanceof GatewayError && e.kind === 'truncated')) throw e;
      raw = lastReplyText(cfg) || raw;
      if (tokens < 8192) { tokens = 8192; continue; }
      const p = parseReview(raw);
      return p.ok ? { status: 'ok', review: p.review, raw, model, ms: Date.now() - started, error: e.message } : { status: 'raw', review: null, raw, model, ms: Date.now() - started, error: e.message };
    }
  }
  return { status: 'raw', review: null, raw, model, ms: Date.now() - started, error: 'No usable reply.' };
}
function lastReplyText(cfg: AiConfig): string {
  const x = aiExchanges().at(-1); if (!x?.reply) return '';
  try { return extractText(cfg.provider, JSON.parse(x.reply)); } catch { return x.reply; }
}

// ---------------------------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------------------------
export interface StoredReview {
  id: string; createdAt: number; date: number; hero: string | null; heroName: string; result: string; minutes: number; deaths: number; source: MatchRecord['source'];
  status: 'ok' | 'raw' | 'error'; review: ReviewContent | null; raw: string | null; error: string | null; model: string | null; digestChars: number; digest?: string;
}
function sanitizeReview(v: unknown): StoredReview | null {
  if (!v || typeof v !== 'object') return null; const o = v as Record<string, unknown>;
  if (typeof o.id !== 'string' || !o.id) return null;
  const review = o.review && typeof o.review === 'object' ? parseReview(JSON.stringify(o.review)) : null;
  return { id: o.id.slice(0, 120), createdAt: finite(o.createdAt) ? o.createdAt : 0, date: finite(o.date) ? o.date : 0, hero: typeof o.hero === 'string' ? o.hero.slice(0, 40) : null,
    heroName: str(o.heroName, 60) || 'Unknown hero', result: str(o.result, 20) || 'unknown', minutes: finite(o.minutes) ? o.minutes : 0, deaths: finite(o.deaths) ? o.deaths : 0,
    source: o.source === 'replay' || o.source === 'import' ? o.source : 'live', status: o.status === 'ok' || o.status === 'raw' ? o.status : 'error',
    review: review?.ok ? { ...review.review, ...((o.review as Record<string, unknown>).partial ? { partial: true } : {}) } : null,
    raw: typeof o.raw === 'string' ? o.raw.slice(0, 20000) : null, error: typeof o.error === 'string' ? o.error.slice(0, 500) : null, model: typeof o.model === 'string' ? o.model.slice(0, 120) : null,
    digestChars: finite(o.digestChars) ? o.digestChars : 0, ...(typeof o.digest === 'string' ? { digest: o.digest.slice(0, DIGEST_MAX_CHARS + 200) } : {}) };
}
export class ReviewStore {
  reviews: StoredReview[] = [];
  private store: KeyValueStore | null; private key: string;
  constructor(store: KeyValueStore | null = null, key = REVIEW_STORE_KEY) { this.store = store; this.key = key; this.load(); }
  private load() {
    try { const t = this.store?.getItem(this.key); if (!t) return; const v = JSON.parse(t); this.reviews = (Array.isArray(v?.reviews) ? v.reviews : []).map(sanitizeReview).filter((r: StoredReview | null): r is StoredReview => !!r).slice(0, MAX_REVIEWS); }
    catch { this.reviews = []; }
  }
  private save() { try { this.store?.setItem(this.key, JSON.stringify({ schema: 1, reviews: this.reviews })); } catch { /* quota: keep in memory */ } }
  has(id: string) { return this.reviews.some(r => r.id === id && r.status !== 'error'); }
  get(id: string) { return this.reviews.find(r => r.id === id) ?? null; }
  put(r: StoredReview) { this.reviews = [r, ...this.reviews.filter(x => x.id !== r.id)].slice(0, MAX_REVIEWS); this.save(); }
  remove(id: string) { this.reviews = this.reviews.filter(r => r.id !== id); this.save(); }
  clear() { this.reviews = []; try { this.store?.removeItem(this.key); } catch { /* ignore */ } }
  exportJSON(exported = new Date().toISOString()) { return JSON.stringify({ schema: 1, exported, reviews: this.reviews }, null, 1); }
}
/** The stored entry for a finished review. */
export function storedReview(d: Digest, m: EndedMatch, r: ReviewResult | { error: string }, now = Date.now()): StoredReview {
  const ok = 'status' in r;
  return { id: d.key, createdAt: now, date: m.record.startedAt, hero: d.hero, heroName: d.heroName, result: d.result, minutes: d.minutes, deaths: d.deaths, source: m.source,
    status: ok ? r.status : 'error', review: ok ? r.review : null, raw: ok && r.status === 'raw' ? r.raw.slice(0, 20000) : null, error: r.error ?? null,
    model: ok ? r.model : null, digestChars: d.chars, digest: d.text };
}
