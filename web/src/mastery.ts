// Skills and mastery (docs/player-profile.md "Skills and mastery"). DOM-free.
//
//   SKILLS / skillOfKey / skillOfCue  the taxonomy of data/coaching/skills.json (the Rust engine tags
//                                    its cues with the same file; old or AI cues are mapped here)
//   SkillMastery.observe()           behaviour evidence from analyzed snapshots and the engine's cues:
//                                    a skill PERFORMED without a reminder (unprompted success), or failed
//   SkillMastery.gate()              the suppression decision for one cue (main.ts deliver())
//   SkillMastery.level()             the player's estimated level from mastery and profile metrics
//   SkillMastery.promptText()        mastered skills + level for the AI prompts
// Persisted in localStorage next to the player profile (versioned, exported / imported with it).
import skillsData from '../../data/coaching/skills.json' with { type: 'json' };
import { PlayerProfile, LIMITS, REVIEW, deathSpend, spendTarget, clockText, type DeathRecord, type KeyValueStore, type MatchRecord, type PlayerSummary } from './player-profile.ts';
import type { Cue, Snapshot } from './types';

export type SkillLevel = 'basic' | 'intermediate' | 'advanced';
export const LEVELS: SkillLevel[] = ['basic', 'intermediate', 'advanced'];
export const LEVEL_LABEL: Record<SkillLevel, string> = { basic: 'Basic', intermediate: 'Intermediate', advanced: 'Advanced' };
export interface SkillDef {
  id: string; label: string; level: SkillLevel; description: string;
  mastered_after: { observations: number; window: number; max_failures: number };
  cues?: string[]; cards?: string[];
  /** A failure of this skill also allows one reminder of these skills this match. */
  allows?: string[];
}
const data = skillsData as unknown as { skills: SkillDef[]; mastery: { unmaster_failures: number; match_unmaster_failures: number; prompted_secs: number; assume_basic_from: SkillLevel; history_matches: number; quiet_high: string[] } };
export const SKILLS: SkillDef[] = data.skills;
export const MASTERY = data.mastery;
const byId = new Map(SKILLS.map(s => [s.id, s]));
export const skillDef = (id: string | null | undefined) => id ? byId.get(id) ?? null : null;

// ---------------------------------------------------------------------------------------------
// Cue -> skill
// ---------------------------------------------------------------------------------------------
/** Same matcher as crates/engine/src/skills.rs for_key: exact, or a prefix ending in '*'; the
 *  longest pattern wins (an exact one over a prefix of the same length). */
export function skillOfKey(key: string): string | null {
  let best: string | null = null, rank = -1;
  for (const s of SKILLS) for (const p of s.cues || []) {
    const ok = p.endsWith('*') ? key.startsWith(p.slice(0, -1)) : key === p;
    const r = p.length * 2 + (p.endsWith('*') ? 0 : 1);
    if (ok && r > rank) { rank = r; best = s.id; }
  }
  return best;
}
/** Match focus goal / profile focus ids (player-profile.ts GOALS, focus lines) -> skill. */
const FOCUS_SKILLS: Record<string, string> = {
  unspent_souls: 'dont_die_holding_souls', won_fight_low: 'low_hp_after_win', close_range: 'positioning_vs_divers', respawn_deaths: 'walk_out_with_team',
  alone_deaths: 'walk_out_with_team', weak_entry: 'heal_before_leaving', forward_unseen: 'respect_missing_enemies', unused_windows: 'use_enemy_dead_windows', deaths: 'back_off_low_hp',
};
/** AI advice (key ai:<trigger>:<focus>): the text first, then the model's focus tag. */
const AI_TEXT: Array<[RegExp, string]> = [
  [/\b(dead|death screen|respawn(ing)? in)\b.*\b(buy|spend|souls)\b|\b(buy|spend)\b.*\bbefore you respawn/i, 'shop_on_death'],
  [/\b(in|at|back to) (our |the )?base\b.*\b(buy|spend|souls|shop)\b|\b(buy|spend|shop)\b.*\b(before you (walk|head|go) out|in base)\b/i, 'shop_in_base'],
  [/\bheal\b.*\b(before|first)\b|\bat \d+% health\b.*\b(out|leav)/i, 'heal_before_leaving'],
  [/\b(spend|unspent|holding|bank)\b.*\bsouls\b|\bsouls\b.*\b(spend|unspent)\b/i, 'dont_die_holding_souls'],
  [/\burn\b/i, 'urn_play'],
  [/\b(walk|head|go|move) out (with|together)|\brespawn\w*\b.*\balone\b|\balone\b.*\brespawn|\bwait for (him|her|them|a teammate)\b.*\bout\b|\bgroup up before you leave/i, 'walk_out_with_team'],
  [/\b(dead|down)\b.*\b(push|take|siege|midboss|objective)\b/i, 'use_enemy_dead_windows'],
  [/\bmidboss|guardian|walker|shrine|objective\b/i, 'objective_timing'],
  [/\b(missing|unseen)\b/i, 'respect_missing_enemies'],
  [/\b(wave|troopers?)\b.*\b(catch|clear|shove|push)\b|\b(catch|clear|shove)\b.*\bwave\b/i, 'catch_waves'],
  [/\bcamps?\b/i, 'farm_camps'],
  [/\b(buy|build|item)\b/i, 'counter_items'],
  [/\b(back off|get out|disengage|low health|heal up)\b/i, 'back_off_low_hp'],
  [/\b(lead|ahead|behind|throw)\b/i, 'throw_awareness'],
];
const AI_FOCUS: Record<string, string> = { healing: 'heal_before_leaving', team: 'join_fights', objective: 'objective_timing', economy: 'dont_die_holding_souls', map: 'respect_missing_enemies', itemization: 'counter_items', wave: 'catch_waves', positioning: 'respect_missing_enemies' };
export function skillOfAdvice(key: string, text: string): string | null { return skillsOfAdvice(key, text)[0] ?? null; }
/** Every skill an AI advice text touches (text order of AI_TEXT), else its focus tag's skill. */
export function skillsOfAdvice(key: string, text: string): string[] {
  const focus = key.split(':')[2] || '';
  const found = [...new Set(AI_TEXT.filter(([re]) => re.test(text)).map(([, s]) => s))];
  return found.length ? found : AI_FOCUS[focus] ? [AI_FOCUS[focus]] : [];
}
/** The skill of any cue: the engine's tag, else the key (cues recorded before the tag, the
 *  shared active:cc key by its text), the profile focus id, or the AI advice mapping. */
export function skillOfCue(cue: Pick<Cue, 'key' | 'text'> & { skill?: string | null }): string | null {
  if (cue.skill && byId.has(cue.skill)) return cue.skill;
  const k = cue.key;
  if (k.startsWith('ai:')) return skillOfAdvice(k, cue.text);
  if (k.startsWith('player:focus')) { const id = k.split(':').at(-1) || ''; return FOCUS_SKILLS[id] ?? null; }
  if (k === 'active:cc') return /zipline/i.test(cue.text) ? 'zipline_discipline' : 'escape_after_cc';
  return skillOfKey(k);
}
const isPraise = (key: string) => key.startsWith('praise:') || key === 'pace:ahead' || key === 'pace:on-track';

// ---------------------------------------------------------------------------------------------
// Stored state
// ---------------------------------------------------------------------------------------------
export const MASTERY_KEY = 'deadlock-coach-skill-mastery-v1';
export const MASTERY_SCHEMA = 1;
/** One match's evidence for one skill: unprompted successes, prompted successes, failures. */
export interface MatchTally { id: string; ok: number; prompted: number; fail: number }
export interface SkillRecord {
  performed: number; prompted: number; failed: number; lastSeen: number | null;
  mastered: boolean; masteredAt: number | null;
  /** Set on the Player page: 'mastered' quiets the coach, 'learning' keeps it coaching. */
  manual: 'mastered' | 'learning' | null;
  /** Newest first, one entry per match with evidence (MASTERY.history_matches at most). */
  history: MatchTally[];
}
export interface StoredMastery { schema: number; skills: Record<string, SkillRecord>; matchIds: string[] }
export type SkillState = 'mastered' | 'assumed' | 'working' | 'unseen';
export interface SkillEvent { skill: string; kind: 'success' | 'failure' | 'mastered' | 'unmastered'; prompted: boolean; clock: number | null; detail: string }
export interface LevelMetrics { deadPct: number | null; deathsPer10: number | null; kda: number | null; matches: number }
export interface LevelEstimate { level: SkillLevel; skillLevel: SkillLevel | null; metricLevel: SkillLevel | null; basis: string }
export interface GateResult { speak: boolean; reason: string; skill: string | null; state: SkillState | null; text?: string }

const finite = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);
const emptyRecord = (): SkillRecord => ({ performed: 0, prompted: 0, failed: 0, lastSeen: null, mastered: false, masteredAt: null, manual: null, history: [] });
function sanitize(raw: unknown): StoredMastery {
  const out: StoredMastery = { schema: MASTERY_SCHEMA, skills: {}, matchIds: [] };
  if (!raw || typeof raw !== 'object') return out;
  const r = raw as Record<string, any>;
  if (finite(r.schema) && r.schema > MASTERY_SCHEMA) throw new Error(`Skill mastery schema ${r.schema} is newer than this app (${MASTERY_SCHEMA}).`);
  out.matchIds = Array.isArray(r.matchIds) ? r.matchIds.filter((x: unknown) => typeof x === 'string').slice(0, 30).map((x: string) => x.slice(0, 60)) : [];
  const skills = r.skills && typeof r.skills === 'object' ? r.skills as Record<string, any> : {};
  for (const [id, v] of Object.entries(skills)) {
    if (!byId.has(id) || !v || typeof v !== 'object') continue;
    const n = (x: unknown) => finite(x) && x >= 0 ? Math.round(x) : 0;
    out.skills[id] = { performed: n(v.performed), prompted: n(v.prompted), failed: n(v.failed), lastSeen: finite(v.lastSeen) ? v.lastSeen : null,
      mastered: v.mastered === true, masteredAt: finite(v.masteredAt) ? v.masteredAt : null, manual: v.manual === 'mastered' || v.manual === 'learning' ? v.manual : null,
      history: Array.isArray(v.history) ? v.history.filter((h: any) => h && typeof h.id === 'string').slice(0, MASTERY.history_matches).map((h: any) => ({ id: h.id.slice(0, 60), ok: n(h.ok), prompted: n(h.prompted), fail: n(h.fail) })) : [] };
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Level from profile metrics
// ---------------------------------------------------------------------------------------------
/** Profile metrics for the level estimate: time dead, deaths per 10 minutes, (kills + assists) / deaths. */
export function metricsOf(profile: { summary(): PlayerSummary; matches: MatchRecord[] }): LevelMetrics {
  const s = profile.summary();
  const kda = profile.matches.slice(0, 10).map(m => m.kda).filter(k => k && finite(k.kills) && finite(k.deaths) && finite(k.assists))
    .map(k => (k!.kills! + k!.assists!) / Math.max(1, k!.deaths!));
  return { deadPct: s.deadPct, deathsPer10: s.deathsPer10, kda: kda.length ? Math.round(kda.reduce((a, b) => a + b, 0) / kda.length * 10) / 10 : null, matches: s.matches };
}
/** Level from metrics alone (null without enough evidence). Real matches: 7-24% dead. */
export function metricLevel(m: LevelMetrics | null): SkillLevel | null {
  if (!m || (m.deadPct == null && m.deathsPer10 == null)) return null;
  const dead = m.deadPct ?? 100, per10 = m.deathsPer10 ?? 100, kda = m.kda;
  let rank = dead <= 10 && per10 <= 1.5 ? 2 : dead <= 18 && per10 <= 3 ? 1 : 0;
  if (kda != null && kda >= 4 && rank < 2) rank++;
  else if (kda != null && kda < 1 && rank > 0) rank--;
  return LEVELS[rank];
}

// ---------------------------------------------------------------------------------------------
// The tracker
// ---------------------------------------------------------------------------------------------
interface Visit { start: number; fromRespawn: boolean; enterHp: number | null; lastHp: number | null; firstSouls: number | null; maxSouls: number; lastSouls: number | null;
  low: number; purchased: boolean; out: number }
interface MatchState {
  id: string; deathClock: number | null; deathSouls: number | null; dropAt: number | null; respawnAt: number | null; walked: boolean;
  visit: Visit | null; prev: { used: number; unused: number; delivered: number; lost: number; survived: number; died: number; joined: number; missed: number } | null;
  /** Reminders spoken this match (skill, clock). */ reminders: Array<{ skill: string; clock: number }>;
  /** One reminder allowed per failure (of the skill or one it allows), for ALLOWANCE_SECS: skill -> failure clock. */ allowance: Map<string, number>;
  /** Reminders spoken since the first unprompted success of a skill this match. */ sinceSuccess: Map<string, number>;
  /** Evidence cues already counted (key -> clock): both analysis paths offer the same cue. */ cueSeen: Map<string, number>;
  seenDeaths: Set<string>; lastSouls: number | null; lastSoulsAt: number | null;
}
/** A failure allows one reminder within this much game time (later calls are not about it). */
export const ALLOWANCE_SECS = 300;
export interface MasteryOptions { store?: KeyValueStore | null; key?: string; now?: () => number; metrics?: () => LevelMetrics | null }

export class SkillMastery {
  data: StoredMastery = { schema: MASTERY_SCHEMA, skills: {}, matchIds: [] };
  /** Events of the match in progress (newest last, capped). */
  events: SkillEvent[] = [];
  /** Suppressed cues this session (validation / diagnostics). */
  suppressed = 0;
  private profile: PlayerProfile; private m: MatchState | null = null;
  private store: KeyValueStore | null; private key: string; private now: () => number; private metrics: () => LevelMetrics | null;
  private dirty = false; private lastSave = 0; private levelCache: { at: number; v: LevelEstimate } | null = null;
  constructor(opts: MasteryOptions = {}) {
    this.store = opts.store ?? null; this.key = opts.key ?? MASTERY_KEY; this.now = opts.now ?? (() => Date.now()); this.metrics = opts.metrics ?? (() => null);
    this.profile = new PlayerProfile({ store: null, now: this.now });
  }
  load(): this {
    if (!this.store) return this;
    try { const t = this.store.getItem(this.key); if (t) this.data = sanitize(JSON.parse(t)); } catch { this.store = null; /* corrupt or newer: keep it untouched */ }
    return this;
  }
  save(): boolean {
    this.dirty = false; if (!this.store) return false;
    try { this.store.setItem(this.key, JSON.stringify(this.data)); this.lastSave = this.now(); return true; } catch { return false; }
  }
  private maybeSave() { if (this.dirty && this.now() - this.lastSave > 10_000) this.save(); }
  record(id: string): SkillRecord { return this.data.skills[id] ??= emptyRecord(); }
  get matchId(): string | null { return this.m?.id ?? null; }

  // ---- Evidence --------------------------------------------------------------------------
  /** Feed one analyzed full-scan snapshot (in order) and the cues the engine produced for it. */
  observe(s: Snapshot, wallMs: number | null = null, cues: Cue[] = []): SkillEvent[] {
    const before = this.events.length;
    const ev = this.profile.observe(s, wallMs, 'live');
    if (ev.ended) this.closeMatch();
    if (ev.matchStarted || (!this.m && this.profile.current)) this.openMatch();
    const m = this.m, cur = this.profile.current;
    if (!m || !cur) { this.maybeSave(); return []; }
    const clock = cur.clock;
    const dead = cur.isDead;
    const souls = finite(s.souls) && s.souls >= 0 ? s.souls : null;
    const hp = finite(s.hp) && finite(s.max_hp) && s.max_hp >= 100 && s.hp <= s.max_hp ? s.hp / s.max_hp : null;
    // --- Deaths: what you carried in, what the death record says (player-profile.ts startDeath).
    if (ev.died) {
      const d = cur.lastDeath;
      m.visit = null; m.walked = true;
      m.deathClock = d?.clock ?? clock; m.deathSouls = d?.souls ?? m.lastSouls; m.dropAt = null;
      if (d) this.deathEvidence(d.clock, d);
    }
    if (dead && m.deathSouls != null && souls != null && souls <= m.deathSouls - 500 && m.dropAt == null) m.dropAt = clock;
    if (ev.respawned) {
      const d = cur.lastDeath;
      if (d) this.deathShop(d);
      m.respawnAt = clock; m.walked = false; m.deathClock = null; m.deathSouls = null;
    }
    // --- Base visits (alive): shopping, healing, walking out with a teammate.
    if (!dead) this.visitEvidence(s, clock, hp, souls);
    if (souls != null && !dead) { m.lastSouls = souls; m.lastSoulsAt = clock; }
    // --- Match counters of the profile aggregator: windows, Urn, low-health episodes, fights.
    const r = cur.record(this.now(), true);
    const now = { used: r.windows.used, unused: r.windows.unused, delivered: r.urn?.delivered ?? 0, lost: r.urn?.lost ?? 0, survived: r.lowHp.survived, died: r.lowHp.died, joined: r.fights.joined, missed: r.fights.missed };
    const p = m.prev; m.prev = now;
    if (p) {
      const step = (n: number, o: number, skill: string, kind: 'success' | 'failure', detail: string) => { for (let i = o; i < n; i++) this.add(skill, kind, clock, detail); };
      step(now.used, p.used, 'use_enemy_dead_windows', 'success', 'pushed while enemies were dead');
      step(now.unused, p.unused, 'use_enemy_dead_windows', 'failure', 'an enemy-dead window went unused');
      step(now.delivered, p.delivered, 'urn_play', 'success', 'Urn delivered');
      step(now.lost, p.lost, 'urn_play', 'failure', 'died carrying the Urn');
      step(now.survived, p.survived, 'back_off_low_hp', 'success', 'backed off at low health and survived');
      step(now.died, p.died, 'back_off_low_hp', 'failure', 'died at low health');
      step(now.joined, p.joined, 'join_fights', 'success', 'joined a team fight');
      step(now.missed, p.missed, 'join_fights', 'failure', 'a team fight happened without you');
    }
    // --- The engine's cues: praise is a success it saw, some calls are a failure in progress.
    for (const c of cues) this.offered(c, clock);
    this.maybeSave();
    return this.events.slice(before);
  }
  private openMatch() {
    this.closeMatch();
    const id = `m${this.now().toString(36)}-${(this.data.matchIds.length + 1).toString(36)}${Math.random().toString(36).slice(2, 5)}`;
    this.m = { id, deathClock: null, deathSouls: null, dropAt: null, respawnAt: null, walked: true, visit: null, prev: null, reminders: [], allowance: new Map(), sinceSuccess: new Map(), cueSeen: new Map(), seenDeaths: new Set(), lastSouls: null, lastSoulsAt: null };
    this.data.matchIds = [id, ...this.data.matchIds.filter(x => x !== id)].slice(0, 30);
    this.events = []; this.dirty = true;
  }
  private closeMatch() { if (this.m) { this.m = null; this.save(); } }
  /** Capture stopped / the review closed the match: the match is over for the mastery too. */
  finishSession() { this.profile.finishSession(0); this.closeMatch(); }

  private deathEvidence(clock: number, d: DeathRecord | null) {
    if (!d || !this.m) return;
    const once = (skill: string, detail: string) => { const k = `${clock}:${skill}`; if (this.m!.seenDeaths.has(k)) return; this.m!.seenDeaths.add(k); this.add(skill, 'failure', clock, detail); };
    const at = clockText(clock);
    if (d.souls != null) {
      const hoard = Math.max(REVIEW.hoard_souls, spendTarget(clock));
      if (d.souls >= hoard) once('dont_die_holding_souls', `died holding ${d.souls.toLocaleString('en-US')} souls at ${at}`);
      else if (d.souls < deathSpend(clock) && clock >= 480) this.add('dont_die_holding_souls', 'success', clock, `died with only ${d.souls} souls: you had spent`);
    }
    if (d.weakEntry === true) once('heal_before_leaving', `went back in hurt and died at ${at}`);
    if (d.lowAfterWin === true) once('low_hp_after_win', `stayed in low after a won fight and died at ${at}`);
    if (d.alone === true && d.afterRespawnSecs != null && d.afterRespawnSecs <= REVIEW.respawn_window_secs) once('walk_out_with_team', `walked out alone and died ${d.afterRespawnSecs} s after respawning`);
    if (d.forwardUnseen === true) once('respect_missing_enemies', `died in their half with enemies missing at ${at}`);
    if (d.close === true && d.burst === true) once('positioning_vs_divers', `burst down up close at ${at}`);
  }
  /** Shopping on the death screen: judged at the respawn (souls before the death vs. spent). */
  private deathShop(d: DeathRecord) {
    const m = this.m!; if ((d.deadSecs ?? 0) < 8) return; // the grey "Respawn in: 1" screen is no shop window
    const need = deathSpend(d.clock), souls = d.souls ?? m.deathSouls, spent = d.spentDead ?? 0;
    if (souls == null) return;
    const at = m.dropAt ?? m.respawnAt ?? d.clock + (d.deadSecs ?? 0);
    if (spent >= 800 && (souls - spent < need || spent >= LIMITS.death_spend_min)) {
      const prompted = m.reminders.some(r => r.skill === 'shop_on_death' && r.clock >= d.clock && r.clock <= at);
      this.add('shop_on_death', 'success', d.clock, `spent ${spent.toLocaleString('en-US')} souls on the death screen`, prompted);
    } else if (souls >= spendTarget(d.clock) && spent < 500) {
      // The same death already counted as dying with the souls: no second reminder for it.
      const same = m.seenDeaths.has(`:dont_die_holding_souls`);
      this.add('shop_on_death', 'failure', d.clock, `respawned with ${souls.toLocaleString('en-US')} souls unspent`, undefined, !same);
    }
  }
  private visitEvidence(s: Snapshot, clock: number | null, hp: number | null, souls: number | null) {
    const m = this.m!, map = s.minimap, you = map?.you && finite(map.you.x) && finite(map.you.y) ? map.you : null, zone = map?.zone_area ?? null;
    const gameplay = s.screen === 'Gameplay';
    const shop = s.screen === 'Shop' && s.dead !== true;
    const inBase = shop || (gameplay && ((you != null && you.y > 0.86) || zone === 'OwnBase'));
    const outside = gameplay && ((you != null && you.y < 0.8) || zone === 'Lane' || zone === 'Jungle' || zone === 'Mid' || zone === 'EnemyBase');
    if (clock == null) return;
    let v = m.visit;
    if (inBase) {
      if (!v) {
        // Souls held just before walking in count (the base frame may already show the purchase).
        const first = m.lastSouls != null && m.lastSoulsAt != null && clock - m.lastSoulsAt <= 15 ? m.lastSouls : souls;
        v = m.visit = { start: clock, fromRespawn: m.respawnAt != null && clock - m.respawnAt <= 20, enterHp: hp, lastHp: hp, firstSouls: first, maxSouls: first ?? 0, lastSouls: first, low: 0, purchased: false, out: 0 };
      }
      v.out = 0;
      if (hp != null) { v.enterHp ??= hp; v.lastHp = hp; }
    }
    // Souls through the whole visit, also on the scoreboard and the first frames out of base (the
    // purchase often shows only there: rec13 27:53 7,685 in base, 27:57 1,305 in lane).
    if (v && souls != null) {
      v.firstSouls ??= souls;
      // A purchase: two reads in a row (or one confirmed on leaving) 500+ under the visit's peak (one misread is not a purchase).
      if (souls <= v.maxSouls - 500) { v.low++; if (v.low >= 2 && !v.purchased) this.purchase(v, clock); } else v.low = 0;
      v.maxSouls = Math.max(v.maxSouls, souls); v.lastSouls = souls;
    }
    if (inBase) return;
    if (!v) { this.walkOut(s, clock, you); return; }
    if (!outside) return;
    if (++v.out < 2) return;
    // Left base: judge the visit.
    m.visit = null;
    if (!v.purchased && v.low >= 1) this.purchase(v, clock);
    // Walking out with the usual buy in your pocket (less is saving for the next item, as the engine says).
    const need = spendTarget(clock);
    if (!v.purchased && (v.firstSouls ?? 0) >= need && (v.lastSouls ?? 0) >= need) this.add('shop_in_base', 'failure', clock, `walked out of base with ${(v.lastSouls ?? 0).toLocaleString('en-US')} souls`);
    if (!v.fromRespawn && v.enterHp != null && v.enterHp < 0.9 && v.lastHp != null) {
      if (v.lastHp >= 0.9) this.add('heal_before_leaving', 'success', clock, `left base at ${Math.round(v.lastHp * 100)}% health`);
      else if (v.lastHp < LIMITS.weak_entry_ratio) this.add('heal_before_leaving', 'failure', clock, `left base at ${Math.round(v.lastHp * 100)}% health`);
    }
    this.walkOut(s, clock, you);
  }
  private purchase(v: Visit, clock: number) {
    v.purchased = true;
    if ((v.firstSouls ?? 0) < deathSpend(clock) && v.maxSouls < deathSpend(clock)) return; // a small buy: no shop window to judge
    const prompted = this.m!.reminders.some(r => r.skill === 'shop_in_base' && r.clock >= v.start && r.clock <= clock);
    this.add('shop_in_base', 'success', clock, `bought in base (${(v.maxSouls - (v.lastSouls ?? v.maxSouls)).toLocaleString('en-US')}+ souls)`, prompted);
    // Spending a pocket of souls at the shop is also not letting them pile up (prompted by a
    // "you are holding N souls" call of the last 2 minutes).
    const asked = this.m!.reminders.some(r => r.skill === 'dont_die_holding_souls' && r.clock <= clock && clock - r.clock <= 120);
    this.add('dont_die_holding_souls', 'success', clock, 'spent a pocket of souls at the shop', asked);
  }
  /** The first located frame out of base within 60 s of a respawn: a teammate within reach? */
  private walkOut(s: Snapshot, clock: number, you: { x: number; y: number } | null) {
    const m = this.m!;
    if (m.walked || m.respawnAt == null || !you || you.y >= 0.75) return;
    if (clock - m.respawnAt > 60) { m.walked = true; return; }
    const live = (s.minimap?.heroes || []).filter(h => h.ally && h.hits >= 2 && h.unseen_ms <= 3000);
    if (!live.length) return;
    m.walked = true;
    const near = Math.min(...live.map(h => Math.hypot(h.pos.x - you.x, h.pos.y - you.y)));
    if (near <= REVIEW.alone_distance) this.add('walk_out_with_team', 'success', clock, 'walked out with a teammate');
    else if (near > REVIEW.missed_fight_distance) this.add('walk_out_with_team', 'failure', clock, 'walked out alone after respawning');
  }
  /** Engine cues as evidence: praise = a success the engine saw (skills with their own detector
   *  above are skipped), pace = farm, and calls that mean "you are failing it right now". */
  /** A cue the engine offered (either analysis path, before the gate): evidence. Repeats of a key
   *  within 30 s are one offer (the fast and the full scan say the same thing). */
  offered(c: Cue, clock: number | null) {
    const m = this.m; if (!m) return;
    const seen = m.cueSeen.get(c.key);
    if (seen != null && clock != null && clock >= seen && clock - seen < 30) return;
    if (clock != null) m.cueSeen.set(c.key, clock);
    const skill = skillOfCue(c); if (!skill) return;
    const direct = ['shop_on_death', 'use_enemy_dead_windows', 'urn_play', 'back_off_low_hp', 'join_fights'];
    if (isPraise(c.key)) { if (!direct.includes(skill)) this.add(skill, 'success', clock, `praised: ${c.key}`); return; }
    if (c.key === 'pace:behind') { this.add('last_hits', 'failure', clock, 'farm behind the lobby'); return; }
    // A death review names the cause: the skill it failed (once per death).
    if (c.key.startsWith('death:review:') && this.m) {
      const k = `${c.key}:${skill}`; if (this.m.seenDeaths.has(k)) return; this.m.seenDeaths.add(k);
      const death = Number(c.key.split(':')[2]);
      // Not when the death record already counted it (its clock may differ by a few seconds).
      if (![...this.m.seenDeaths].some(x => { const [c, s] = x.split(':'); return s === skill && Math.abs(Number(c) - death) <= 10; })) this.add(skill, 'failure', clock, `death review: ${c.text.slice(0, 80)}`);
      return;
    }
    if (c.key === 'review:respawn-alone') this.add('walk_out_with_team', 'failure', clock, 'alone right after a respawn with enemies missing');
    else if (c.key === 'active:heal') this.add('heal_before_leaving', 'failure', clock, 'heading out of base hurt');
  }

  /** Record one piece of evidence and update the skill's mastery. */
  add(skill: string, kind: 'success' | 'failure', clock: number | null, detail: string, prompted?: boolean, grant = true): void {
    const def = byId.get(skill); if (!def || !this.m) return;
    const m = this.m, r = this.record(skill);
    if (prompted == null) prompted = kind === 'success' && clock != null && m.reminders.some(x => x.skill === skill && x.clock <= clock && clock - x.clock <= MASTERY.prompted_secs);
    let t = r.history.find(h => h.id === m.id);
    if (!t) { t = { id: m.id, ok: 0, prompted: 0, fail: 0 }; r.history = [t, ...r.history].slice(0, MASTERY.history_matches); }
    if (kind === 'success') { if (prompted) { t.prompted++; r.prompted++; } else { t.ok++; r.performed++; if (!m.sinceSuccess.has(skill)) m.sinceSuccess.set(skill, 0); } }
    else {
      t.fail++; r.failed++;
      // One reminder per failure: for this skill or one of the skills it allows (one in total).
      if (grant) m.allowance.set(skill, clock ?? -1);
    }
    r.lastSeen = this.now(); this.dirty = true; this.levelCache = null;
    this.push({ skill, kind, prompted: kind === 'success' && prompted, clock, detail });
    const w = this.window(skill);
    if (!r.mastered && w.ok >= def.mastered_after.observations && w.fail <= def.mastered_after.max_failures) {
      r.mastered = true; r.masteredAt = this.now();
      this.push({ skill, kind: 'mastered', prompted: false, clock, detail: `${w.ok} unprompted in the last ${def.mastered_after.window} matches, ${w.fail} failed` });
    } else if (r.mastered && kind === 'failure' && (w.fail >= MASTERY.unmaster_failures || t.fail >= MASTERY.match_unmaster_failures)) {
      r.mastered = false; r.masteredAt = null;
      this.push({ skill, kind: 'unmastered', prompted: false, clock, detail: `${w.fail} failures in the last ${def.mastered_after.window} matches` });
    }
  }
  private push(e: SkillEvent) { this.events.push(e); if (this.events.length > 300) this.events.shift(); }
  /** Evidence of the skill's mastery window: the last `window` matches (the current one included). */
  window(skill: string): { ok: number; prompted: number; fail: number; matches: number } {
    const def = byId.get(skill)!, r = this.data.skills[skill];
    const ids = new Set(this.data.matchIds.slice(0, def.mastered_after.window));
    const hs = (r?.history || []).filter(h => ids.has(h.id));
    return { ok: hs.reduce((a, h) => a + h.ok, 0), prompted: hs.reduce((a, h) => a + h.prompted, 0), fail: hs.reduce((a, h) => a + h.fail, 0), matches: hs.length };
  }
  /** The failure token that allows one reminder of skill this match (its own, or of a skill that allows it). */
  private token(skill: string, clock: number | null = this.profile.current?.clock ?? null): string | null {
    const a = this.m?.allowance; if (!a) return null;
    const ok = (id: string) => { const at = a.get(id); return at != null && (at < 0 || clock == null || (clock >= at && clock - at <= ALLOWANCE_SECS)); };
    if (ok(skill)) return skill;
    return SKILLS.find(s => ok(s.id) && (s.allows || []).includes(skill))?.id ?? null;
  }
  private tally(skill: string): MatchTally | null { const id = this.m?.id; return id ? this.data.skills[skill]?.history.find(h => h.id === id) ?? null : null; }

  // ---- State, level ----------------------------------------------------------------------
  state(skill: string, level: SkillLevel = this.level().level): SkillState {
    const def = byId.get(skill); if (!def) return 'unseen';
    const r = this.data.skills[skill];
    if (r?.manual === 'mastered') return 'mastered';
    const seen = !!r && (r.performed + r.prompted + r.failed) > 0;
    if (r?.manual === 'learning') return seen ? 'working' : 'unseen';
    if (r?.mastered) return 'mastered';
    // Basics are assumed for an intermediate+ player, unless they fail them.
    if (def.level === 'basic' && LEVELS.indexOf(level) >= LEVELS.indexOf(MASTERY.assume_basic_from) && this.window(skill).fail === 0) return 'assumed';
    return seen ? 'working' : 'unseen';
  }
  /** Skill level from mastered skills (evidence only) and profile metrics. */
  level(): LevelEstimate {
    const c = this.levelCache, now = this.now();
    if (c && now - c.at >= 0 && now - c.at < 30_000) return c.v;
    const v = this.computeLevel(); this.levelCache = { at: now, v }; return v;
  }
  private computeLevel(): LevelEstimate {
    const mastered = (lv: SkillLevel) => SKILLS.filter(s => s.level === lv && (this.data.skills[s.id]?.mastered || this.data.skills[s.id]?.manual === 'mastered')).length;
    const seen = (lv: SkillLevel) => SKILLS.filter(s => s.level === lv && (() => { const r = this.data.skills[s.id]; return !!r && r.performed + r.prompted + r.failed > 0; })()).length;
    const evidence = seen('basic') + seen('intermediate') + seen('advanced');
    let skillLevel: SkillLevel | null = null;
    if (evidence >= 4) {
      const b = mastered('basic'), i = mastered('intermediate');
      skillLevel = b >= 4 && i >= 3 && i * 5 >= seen('intermediate') * 3 ? 'advanced' : b >= 2 ? 'intermediate' : 'basic';
    }
    const metrics = (() => { try { return this.metrics(); } catch { return null; } })();
    const ml = metricLevel(metrics);
    const rank = (l: SkillLevel) => LEVELS.indexOf(l);
    const level = skillLevel && ml ? LEVELS[Math.floor((2 * rank(skillLevel) + rank(ml)) / 3 + 0.5)] : skillLevel ?? ml ?? 'basic';
    const parts = [skillLevel ? `${mastered('basic')} basic and ${mastered('intermediate')} intermediate skills mastered` : 'not enough skill evidence yet',
      metrics && ml ? `time dead ${metrics.deadPct ?? '?'}%, ${metrics.deathsPer10 ?? '?'} deaths per 10 min${metrics.kda != null ? `, KDA ${metrics.kda}` : ''}` : null].filter(Boolean);
    return { level, skillLevel, metricLevel: ml, basis: parts.join('; ') };
  }
  /** Skills by state for the Player page. */
  lists(): Record<SkillState, SkillDef[]> {
    const level = this.level().level;
    const out: Record<SkillState, SkillDef[]> = { mastered: [], assumed: [], working: [], unseen: [] };
    for (const s of SKILLS) out[this.state(s.id, level)].push(s);
    return out;
  }
  setManual(skill: string, manual: 'mastered' | 'learning' | null) { if (!byId.has(skill)) return; this.record(skill).manual = manual; this.levelCache = null; this.save(); }

  // ---- Suppression -----------------------------------------------------------------------
  /** Should this cue be spoken? High safety calls always pass; Normal / Low cues of a mastered (or
   *  assumed) skill are dropped unless the player failed it this match (one reminder per failure);
   *  a skill in progress gets one reminder per match after an unprompted success. */
  gate(cue: Cue, clock: number | null = null): GateResult {
    // AI advice often touches several skills (a death explanation names the cause and the buy):
    // it is judged by the first one that is not quiet.
    let skill = skillOfCue(cue);
    if (cue.key.startsWith('ai:')) { const all = skillsOfAdvice(cue.key, cue.text); skill = all.find(s => { const st = this.state(s); return (st !== 'mastered' && st !== 'assumed') || this.token(s) != null; }) ?? all[0] ?? skill; }
    if (!skill) return { speak: true, reason: 'no skill', skill: null, state: null };
    const st = this.state(skill);
    const quiet = st === 'mastered' || st === 'assumed';
    const allowance = this.token(skill, clock ?? undefined) != null;
    if (cue.priority === 'High' && !MASTERY.quiet_high.includes(skill)) {
      // The death review's "Spend N souls now." tail is the death-screen shop reminder.
      if (cue.key.startsWith('death:review') && this.state('shop_on_death') !== 'working' && this.state('shop_on_death') !== 'unseen' && this.token('shop_on_death') == null) {
        const text = cue.text.replace(/\s*Spend [\d,]+ souls now\.\s*$/, '');
        if (text !== cue.text) return { speak: true, reason: 'safety call; shop tail dropped (mastered)', skill, state: st, text };
      }
      return { speak: true, reason: 'safety call', skill, state: st };
    }
    if (quiet) {
      if (!isPraise(cue.key) && allowance) return { speak: true, reason: 'failed this match: one reminder', skill, state: st };
      return { speak: false, reason: st === 'assumed' ? 'basic skill assumed at this level' : 'mastered', skill, state: st };
    }
    if (st === 'working' && !isPraise(cue.key) && !allowance) {
      const t = this.tally(skill);
      if (t && t.ok > 0 && (this.m?.sinceSuccess.get(skill) ?? 0) >= 1) return { speak: false, reason: 'in progress: one reminder per match after a success', skill, state: st };
    }
    return { speak: true, reason: st, skill, state: st };
  }
  /** A cue was spoken (after gate()): a reminder of its skill (praise is not one). */
  noteSpoken(cue: Cue, clock: number | null) {
    // AI advice reminds every skill its text touches.
    if (cue.key.startsWith('ai:') && !cue.skill) { for (const s of skillsOfAdvice(cue.key, cue.text)) this.noteSpoken({ ...cue, skill: s }, clock); return; }
    const skill = skillOfCue(cue); const m = this.m;
    if (!skill || !m || isPraise(cue.key)) return;
    m.reminders.push({ skill, clock: clock ?? -1 }); if (m.reminders.length > 400) m.reminders.shift();
    if (m.sinceSuccess.has(skill)) m.sinceSuccess.set(skill, m.sinceSuccess.get(skill)! + 1);
    // The death review also reminds the death-screen buy when it says "Spend N souls now".
    if (cue.key.startsWith('death:review') && /\bSpend [\d,]+ souls now\b/.test(cue.text)) m.reminders.push({ skill: 'shop_on_death', clock: clock ?? -1 });
    const st = this.state(skill);
    const tok = this.token(skill, clock ?? undefined);
    if ((st === 'mastered' || st === 'assumed') && tok) m.allowance.delete(tok);
  }

  // ---- AI prompt -------------------------------------------------------------------------
  /** "PLAYER LEVEL" for the AI prompts (<= 600 chars), or null without any evidence. */
  promptText(): string | null {
    const lv = this.level(), l = this.lists();
    const quiet = [...l.mastered, ...l.assumed];
    if (!quiet.length && !l.working.length && !lv.metricLevel && !lv.skillLevel) return null;
    const failed = new Set(this.events.filter(e => e.kind === 'failure').map(e => e.skill));
    const name = (s: SkillDef) => `${s.label}${failed.has(s.id) ? ' (failed this match)' : ''}`;
    const lines = [`Level: ${LEVEL_LABEL[lv.level]} (${lv.basis}).`,
      quiet.length ? `Mastered, do not coach unless failed now: ${quiet.map(name).join('; ')}.` : '',
      l.working.length ? `Working on: ${l.working.slice(0, 6).map(name).join('; ')}.` : '',
      `Pitch advice at the ${lv.level} level${lv.level !== 'basic' ? '; no generic shopping, healing or last-hit reminders' : ''}.`].filter(Boolean);
    return lines.join(' ').slice(0, 600);
  }

  // ---- Persistence -----------------------------------------------------------------------
  toJSON(): StoredMastery { return structuredClone(this.data); }
  /** Merge an export: a player-profile export with `mastery`, or a bare mastery object. Returns the skills changed. */
  importJSON(text: string): number {
    const raw = JSON.parse(text) as Record<string, unknown>;
    const src = raw && typeof raw === 'object' && raw.mastery ? raw.mastery : raw;
    if (!src || typeof src !== 'object' || !(src as Record<string, unknown>).skills) return 0;
    const inc = sanitize(src);
    let n = 0;
    for (const [id, r] of Object.entries(inc.skills)) {
      const mine = this.data.skills[id];
      if (!mine) { this.data.skills[id] = r; n++; continue; }
      const ids = new Set(mine.history.map(h => h.id));
      const added = r.history.filter(h => !ids.has(h.id));
      if (added.length) { for (const h of added) { mine.performed += h.ok; mine.prompted += h.prompted; mine.failed += h.fail; } mine.history = [...mine.history, ...added].slice(0, MASTERY.history_matches); n++; }
      if (r.mastered && !mine.mastered) { mine.mastered = true; mine.masteredAt = r.masteredAt; n++; }
      mine.manual ??= r.manual; mine.lastSeen = Math.max(mine.lastSeen ?? 0, r.lastSeen ?? 0) || null;
    }
    this.data.matchIds = [...this.data.matchIds, ...inc.matchIds.filter(x => !this.data.matchIds.includes(x))].slice(0, 30);
    this.levelCache = null; this.save();
    return n;
  }
  reset() { this.data = { schema: MASTERY_SCHEMA, skills: {}, matchIds: [] }; this.events = []; this.levelCache = null; this.m = null; this.profile = new PlayerProfile({ store: null, now: this.now }); try { this.store?.removeItem(this.key); } catch { /* ignore */ } }
}
