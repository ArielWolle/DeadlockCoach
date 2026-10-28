/**
 * Player profile: learns the player's strengths and weaknesses from the coach's per-frame
 * snapshots, across matches, and keeps them in this browser only (localStorage).
 *
 * Pure and DOM-free (runs under `node --experimental-strip-types`). The per-sample reductions,
 * death reasons, fight / enemy-dead-window / hoarding definitions and their thresholds mirror
 * crates/engine/src/review.rs and data/coaching/review.json so the profile and the spoken death
 * reviews agree. Trait thresholds are coach heuristics documented in docs/player-profile.md
 * (calibrated on the reviewed recordings in docs/recording-analysis-4.md and -5.md); they are not
 * game rules.
 *
 * Evidence rule (as in review.rs): an unread value is never evidence. Every per-death reason is
 * `true | false | null` (null = not observable), and every rate has its own denominator of frames
 * or deaths where the value was actually read.
 */
import type { Point, Snapshot } from './types';

export const PLAYER_PROFILE_KEY = 'deadlock-coach-player-profile-v1';
export const PLAYER_PROFILE_SCHEMA = 1;
export const MAX_MATCHES = 30;

/** data/coaching/review.json `thresholds` (keep in sync). */
export const REVIEW = {
  death_window_secs: 15, alone_distance: 0.25, unseen_enemies: 3, souls_at_death: 1200,
  fight_radius: 0.13, missed_fight_distance: 0.3, hoard_souls: 3000, respawn_window_secs: 40,
} as const;
/** Profile-only constants. */
export const LIMITS = {
  /** review.rs: each sample stands for the gap to the next one, capped at 10 s. */
  sample_cap_secs: 10,
  /** review.rs death_review: "soon after respawn" is within 90 s. */
  soon_after_respawn_secs: 90,
  /** review.rs: every HP read of the last 30 s under 60% = went back in weak. */
  weak_entry_secs: 30, weak_entry_ratio: 0.6,
  /** review.rs: in the enemy half = minimap y < 0.48 (our base is at the bottom). */
  enemy_half_y: 0.48,
  /** review.rs pattern(): an enemy-dead window is >= 2 enemies dead for >= 20 s. */
  window_enemies: 2, window_secs: 20,
  /** A lane front that moves this much toward the enemy during a window counts as using it. */
  window_push_gain: 0.15,
  /** review.rs: burst = from the healthiest reading (>= 30%) to dead within 15 s. */
  burst_secs: 15, burst_from: 0.3,
  /** Low health = under 30% while alive (the engine's critical band). */
  low_hp: 0.3, low_hp_safe: 0.5,
  /** A fight cluster that disappears for less than this is the same fight (minimap flicker). */
  fight_gap_secs: 10,
  /** Match boundaries. */
  wall_gap_new_match_ms: 15 * 60_000, wall_gap_check_ms: 3 * 60_000,
  /** A match is kept only with this much readable gameplay (or at least one death). */
  min_match_secs: 120,
} as const;

export type Area = 'lane' | 'jungle' | 'base' | 'dead' | 'unknown';
export interface DeathRecord {
  clock: number; cause: string | null; family: string | null; killer: string | null; close: boolean | null;
  /** False = nothing readable was recorded in the window before the death. */
  evidence: boolean;
  fallSecs: number | null; fromPct: number | null; burst: boolean | null;
  souls: number | null; alone: boolean | null; forwardUnseen: boolean | null;
  teammatesDied: number | null; afterRespawnSecs: number | null; weakEntry: boolean | null;
  effects?: string[];
}
export interface MatchRecord {
  v: 1; id: string; source: 'live' | 'replay' | 'import'; startedAt: number; endedAt: number;
  hero: string | null; matchId: number | null; startClock: number | null; endClock: number | null;
  observedSecs: number; result: 'win' | 'loss' | null; partial?: boolean; fingerprint?: string;
  time: Record<Area, number>;
  deaths: DeathRecord[];
  fights: { joined: number; missed: number; whileDead: number };
  windows: { used: number; unused: number; unusedAt: number[] };
  souls: { knownSecs: number; hoardSecs: number; peak: number };
  lowHp: { knownSecs: number; lowSecs: number; episodes: number; survived: number; died: number };
  deepAwaySecs: number;
  farm: { perMin: number | null };
  kda: { kills: number | null; deaths: number | null; assists: number | null } | null;
}

interface Sample {
  t: number | null; wall: number | null; area: Area; hp: number | null; you: Point | null; nearestAlly: number | null;
  unseen: number | null; alliesDead: number | null; enemiesDead: number | null; souls: number | null;
  fightAt: number | null; deepWave: boolean; push: [number | null, number | null, number | null]; lane: 0 | 1 | 2 | null;
}

// ---------------------------------------------------------------------------------------------
// Killer families
// ---------------------------------------------------------------------------------------------
/** Damage-panel causes the coach has seen, with the hero that owns them and whether the damage
 *  comes from melee range. Sources: docs/recording-analysis-4.md, -5.md (Victor's Aura of
 *  Suffering, Drifter's Stalker's Mark, Bebop's Hyper Beam, melee divers), crates/vision/src/scan.rs
 *  (OCR variants "Mele", "Melec", "Stalkers MarkX"). `close: null` = range not implied. */
const KILLERS: Array<{ re: RegExp; family: string; hero?: string; close: boolean | null }> = [
  { re: /aura\s*of\s*suff|still\s*life/i, family: "Victor's Aura of Suffering", hero: 'hero_frank', close: true },
  { re: /stalker/i, family: "Drifter's Stalker's Mark", hero: 'hero_drifter', close: true },
  { re: /mele|bashdown/i, family: 'melee hits', close: true },
  { re: /hyper\s*beam|sticky\s*bomb/i, family: "Bebop's Hyper Beam", hero: 'hero_bebop', close: false },
  { re: /trooper/i, family: 'troopers', close: null },
  { re: /guardian|walker|patron|shrine/i, family: 'structures', close: false },
  { re: /midboss|mid\s*boss/i, family: 'the Midboss', close: null },
  { re: /gun\s*damage|bullet/i, family: 'gun damage', close: null },
];
export function killerFamily(cause: string | null | undefined): { family: string; hero: string | null; close: boolean | null } | null {
  if (!cause || !cause.trim()) return null;
  const k = KILLERS.find(k => k.re.test(cause));
  if (k) return { family: k.family, hero: k.hero ?? null, close: k.close };
  // Unknown ability: its first word, so OCR variants ("Chimera MKI" / "Chimera Mkl") group.
  const word = cause.toLowerCase().replace(/[^a-z\s]/g, ' ').trim().split(/\s+/)[0] || '';
  return word.length >= 3 ? { family: word[0].toUpperCase() + word.slice(1), hero: null, close: null } : null;
}

/** Hero archetypes for practice tips (docs/heroes/*). Only heroes whose range is unambiguous. */
const LONG_RANGE = new Set(['hero_orion', 'hero_hornet']);
const FRONTLINE = new Set(['hero_atlas', 'hero_krill', 'hero_viscous', 'hero_warden', 'hero_yamato', 'hero_shiv', 'hero_frank', 'hero_drifter', 'hero_werewolf']);
const FALLBACK_NAMES: Record<string, string> = { hero_orion: 'Grey Talon', hero_hornet: 'Vindicta', hero_frank: 'Victor', hero_drifter: 'Drifter', hero_bebop: 'Bebop' };

// ---------------------------------------------------------------------------------------------
// Per-match aggregation
// ---------------------------------------------------------------------------------------------
const finite = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);
const dist = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);
function digitDiff(a: number, b: number) {
  const x = String(a), y = String(b); let d = Math.abs(x.length - y.length);
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) d++;
  return d;
}
export function resultOf(screen: string | null | undefined): 'win' | 'loss' | 'unknown' | null {
  if (!screen) return null;
  if (/victory/i.test(screen)) return 'win';
  if (/defeat/i.test(screen)) return 'loss';
  if (/post\s*game|postgame|match\s*end/i.test(screen)) return 'unknown';
  return null;
}

/** Reduce one snapshot like review.rs `sample()`. */
function reduce(s: Snapshot, t: number | null, wall: number | null): Sample {
  const map = s.minimap || null; const you = map?.you && finite(map.you.x) && finite(map.you.y) ? map.you : null;
  const screen = s.screen || null;
  let area: Area;
  if (s.dead === true || screen === 'Dead') area = 'dead';
  else if (screen !== 'Gameplay') area = 'unknown';
  else if (you && you.y > 0.86) area = 'base';
  else if (you && map?.your_lane) area = 'lane';
  else if (you) area = 'jungle';
  else area = 'unknown';
  const live = (map?.heroes || []).filter(h => h.hits >= 2 && h.unseen_ms <= 3000);
  const nearestAlly = you ? live.filter(h => h.ally).reduce<number | null>((m, h) => { const d = dist(h.pos, you); return m == null || d < m ? d : m; }, null) : null;
  let fightAt: number | null = null;
  if (you) for (const a of live.filter(h => h.ally)) {
    const allies = live.filter(o => o.ally && dist(o.pos, a.pos) <= REVIEW.fight_radius).length;
    const enemies = live.filter(e => !e.ally && dist(e.pos, a.pos) <= REVIEW.fight_radius).length;
    if (allies >= 2 && enemies >= 2) { const d = dist(a.pos, you); if (fightAt == null || d < fightAt) fightAt = d; }
  }
  const team = s.hero_status?.length === 12 ? s.hero_status : null;
  let unseen: number | null = null, alliesDead: number | null = null, enemiesDead: number | null = null;
  if (team) {
    alliesDead = team.filter(h => h.ally && h.dead === true).length;
    enemiesDead = team.filter(h => !h.ally && h.dead === true).length;
    unseen = team.filter(h => !h.ally && h.dead !== true && h.bar_visible === false).length;
  }
  const hp = finite(s.hp) && finite(s.max_hp) && s.max_hp >= 100 && s.hp <= s.max_hp && s.hp >= 0 ? s.hp / s.max_hp : null;
  const lanes = map ? [map.green, map.blue, map.yellow] : [];
  const deepWave = lanes.some(l => l && finite(l.push) && l.push < 0.3 && l.enemy_troopers >= 2);
  const push = (map ? lanes.map(l => l && finite(l.push) ? l.push : null) : [null, null, null]) as Sample['push'];
  const souls = finite(s.souls) && s.souls >= 0 ? s.souls : null;
  const idx = ['Green', 'Blue', 'Yellow'].indexOf(map?.your_lane || '');
  return { t, wall, area, hp, you, nearestAlly, unseen, alliesDead, enemiesDead, souls, fightAt, deepWave, push, lane: idx >= 0 ? idx as 0 | 1 | 2 : null };
}

interface OpenDeath { rec: DeathRecord; startT: number }
/** Aggregates one match. Keeps only a bounded window of recent samples plus running totals. */
export class MatchAggregator {
  readonly startedAt: number; source: MatchRecord['source'];
  private samples: Sample[] = [];
  private heroVotes = new Map<string, number>();
  matchId: number | null = null;
  clock: number | null = null; lastWall: number | null = null; private rejects = 0;
  startClock: number | null = null; private lastRead: number | null = null; result: MatchRecord['result'] = null;
  private time: Record<Area, number> = { lane: 0, jungle: 0, base: 0, dead: 0, unknown: 0 };
  private deaths: DeathRecord[] = []; private open: OpenDeath | null = null;
  private dead = false; private lastDeadT: number | null = null; private respawnedAt: number | null = null; private prevRespawn: number | null = null;
  private fights = { joined: 0, missed: 0, whileDead: 0 }; private fight: { lastSeen: number } | null = null;
  private windows = { used: 0, unused: 0, unusedAt: [] as number[] };
  private win: { start: number; secs: number; used: boolean; push: Sample['push'] } | null = null;
  private souls = { knownSecs: 0, hoardSecs: 0, peak: 0 };
  private lowHp = { knownSecs: 0, lowSecs: 0, episodes: 0, survived: 0, died: 0 }; private lowOpen: number | null = null;
  private deepAwaySecs = 0;
  private worth: { first?: { t: number; v: number }; last?: { t: number; v: number } } = {};
  private kda: MatchRecord['kda'] = null;
  private prev: Sample | null = null;
  constructor(startedAt: number, source: MatchRecord['source'] = 'live') { this.startedAt = startedAt; this.source = source; }

  /** Clock smoothing: a read inconsistent with the wall-clock prediction (or, without wall time,
   *  going backwards) is OCR noise (rec4 read 22:22 after 22:42) unless it repeats three times. */
  private effectiveClock(read: number | null, wall: number | null): number | null {
    if (this.clock == null) return read;
    const predicted = wall != null && this.lastWall != null ? this.clock + Math.max(0, wall - this.lastWall) / 1000 : null;
    if (read == null) return predicted;
    const ok = predicted != null ? Math.abs(read - predicted) <= 20 : read >= this.clock - 3 && read <= this.clock + 900;
    if (ok || ++this.rejects >= 3) { this.rejects = 0; return read; }
    return predicted;
  }
  /** True when this read (raw) is consistent with the running clock. */
  continuous(read: number | null, wall: number | null): boolean {
    if (read == null || this.clock == null) return true;
    const predicted = wall != null && this.lastWall != null ? this.clock + Math.max(0, wall - this.lastWall) / 1000 : this.clock;
    return read >= this.clock - 60 && Math.abs(read - predicted) <= Math.max(60, predicted - this.clock + 60);
  }

  get observedSecs() { return Object.values(this.time).reduce((a, b) => a + b, 0); }
  get hasEvidence() { return this.time.lane + this.time.jungle + this.time.base + this.time.dead >= LIMITS.min_match_secs || this.deaths.length > 0; }
  get isDead() { return this.dead; }

  add(s: Snapshot, wall: number | null): { died: boolean; respawned: boolean } {
    const read = finite(s.game_clock_secs) && s.game_clock_secs >= 0 && s.game_clock_secs <= 14400 ? Math.round(s.game_clock_secs) : null;
    const t = this.effectiveClock(read, wall);
    if (t != null) { this.clock = t; if (this.startClock == null) this.startClock = t; if (t === read) this.lastRead = read; }
    if (wall != null) this.lastWall = wall;
    if (finite(s.match_id) && this.matchId == null) this.matchId = s.match_id;
    if (s.hero && /^hero_/.test(s.hero)) this.heroVotes.set(s.hero, (this.heroVotes.get(s.hero) || 0) + 1);
    const ext = s as Snapshot & { kills?: number | null; deaths?: number | null; assists?: number | null };
    if (finite(ext.kills) || finite(ext.deaths) || finite(ext.assists)) this.kda = { kills: finite(ext.kills) ? ext.kills : this.kda?.kills ?? null, deaths: finite(ext.deaths) ? ext.deaths : this.kda?.deaths ?? null, assists: finite(ext.assists) ? ext.assists : this.kda?.assists ?? null };
    if (finite(s.own_networth) && t != null) { const p = { t, v: s.own_networth }; this.worth.first ??= p; if (s.own_networth >= (this.worth.last?.v ?? 0)) this.worth.last = p; }
    const x = reduce(s, t, wall);
    // Souls barely change between two reads: a frame without a souls read keeps the last one for
    // up to 10 s (rec4 read the souls box on about half of the frames).
    const p = this.prev;
    if (x.souls == null && p && p.souls != null && p.area !== 'dead' && x.area !== 'dead' && p.t != null && x.t != null && x.t - p.t <= 10) x.souls = p.souls;
    this.integrate(x);
    const events = { died: false, respawned: false };
    // Deaths (review.rs evaluate): entering the death screen, or a death screen > 60 s after the
    // previous one (rec5 merged a death across a 7-minute recording gap). Unknown frames between
    // death screens do not split or end a death.
    if (x.area === 'dead' && x.t != null) {
      // A single gameplay frame inside a death screen is a misread, not a respawn (rec4 26:37
      // between death screens at 26:32 and 26:40): reopen that death.
      const last = this.deaths[this.deaths.length - 1];
      if (!this.dead && last && this.respawnedAt != null && x.t - this.respawnedAt <= 8 && this.lastDeadT != null && this.respawnedAt - this.lastDeadT <= 10) {
        this.deaths.pop(); this.open = { rec: last, startT: last.clock }; this.respawnedAt = this.prevRespawn; this.dead = true;
      }
      if (!this.dead || (this.lastDeadT != null && x.t > this.lastDeadT + 60)) { this.closeDeath(); this.startDeath(x, s); events.died = true; }
      this.dead = true; this.lastDeadT = x.t;
      const o = this.open;
      if (o) {
        // The damage panel fills in over a few seconds: the latest reading wins.
        const dd = (s as Snapshot & { death_damage?: Array<{ source?: string; damage?: number }> | null; death_killing_blow?: string | null });
        const top = Array.isArray(dd.death_damage) ? [...dd.death_damage].filter(d => d && typeof d.source === 'string').sort((a, b) => (b.damage || 0) - (a.damage || 0))[0]?.source : undefined;
        const cause = s.death_cause || top || dd.death_killing_blow || null;
        if (cause) { o.rec.cause = String(cause).slice(0, 60); const k = killerFamily(o.rec.cause); o.rec.family = k?.family ?? null; o.rec.killer = k?.hero ?? null; o.rec.close = k?.close ?? null; }
        if (o.rec.souls == null && x.souls != null) o.rec.souls = x.souls;
      }
    } else if (s.screen === 'Gameplay' && x.area !== 'dead' && this.dead) {
      this.dead = false; this.closeDeath(); this.prevRespawn = this.respawnedAt; if (x.t != null) this.respawnedAt = x.t; events.respawned = true;
    }
    this.samples.push(x);
    const keep = (x.t ?? 0) - 120;
    while (this.samples.length > 400 || (this.samples[0]?.t != null && x.t != null && this.samples[0].t! < keep)) this.samples.shift();
    return events;
  }

  private integrate(b: Sample) {
    const a = this.prev; this.prev = b;
    if (!a) return;
    let dt = 0;
    if (a.wall != null && b.wall != null) dt = (b.wall - a.wall) / 1000;
    else if (a.t != null && b.t != null) dt = b.t - a.t;
    dt = Math.max(0, Math.min(LIMITS.sample_cap_secs, dt));
    if (!dt) return;
    this.time[a.area] += dt;
    const alive = a.area === 'lane' || a.area === 'jungle' || a.area === 'base';
    if (alive && a.souls != null) { this.souls.knownSecs += dt; if (a.souls >= REVIEW.hoard_souls) { this.souls.hoardSecs += dt; this.souls.peak = Math.max(this.souls.peak, a.souls); } }
    if (a.deepWave && a.area === 'jungle') this.deepAwaySecs += dt;
    if (alive && a.hp != null) {
      this.lowHp.knownSecs += dt;
      if (a.hp < LIMITS.low_hp) this.lowHp.lowSecs += dt;
    }
    // Low-health episodes: entering < 30% alive; survived = back to >= 50% or reached base.
    if (alive && b.hp != null && b.hp < LIMITS.low_hp && this.lowOpen == null && b.area !== 'dead') { this.lowOpen = b.t ?? 0; this.lowHp.episodes++; }
    else if (this.lowOpen != null && ((b.hp != null && b.hp >= LIMITS.low_hp_safe) || b.area === 'base')) { this.lowHp.survived++; this.lowOpen = null; }
    else if (this.lowOpen != null && b.area === 'dead') { this.lowHp.died++; this.lowOpen = null; }
    // Fight episodes (review.rs pattern()): a cluster of >= 2 allies and >= 2 enemies; joined when
    // you were within a third of the map. A cluster absent < 10 s is the same fight.
    const tb = b.t ?? a.t;
    if (a.fightAt != null) {
      if (!this.fight || (tb != null && tb - this.fight.lastSeen > LIMITS.fight_gap_secs)) {
        if (a.area === 'dead') this.fights.whileDead++;
        else if (a.fightAt > REVIEW.missed_fight_distance) this.fights.missed++; else this.fights.joined++;
      }
      this.fight = { lastSeen: a.t ?? tb ?? 0 };
    }
    // Enemy-dead windows (review.rs pattern()): >= 2 enemies dead while you are alive. Used when
    // you were in their half or a lane front moved toward them. Unknown frames keep a window
    // open without adding time (no evidence); your death or fewer than 2 dead closes it.
    const nDead = a.enemiesDead;
    if (alive && nDead != null && nDead >= LIMITS.window_enemies) {
      if (!this.win) this.win = { start: a.t ?? 0, secs: 0, used: false, push: [...a.push] };
      const w = this.win; w.secs += dt;
      if (a.you && a.you.y < 0.5) w.used = true;
      // Only the lane you are on counts: other fronts drift on their own (rec4 32:07, a front
      // moved 0.2 while you stood in base).
      a.push.forEach((p, i) => { if (w.push[i] == null && p != null) w.push[i] = p; });
      if (a.area === 'lane' && a.lane != null) { const p = a.push[a.lane], p0 = w.push[a.lane]; if (p != null && p0 != null && p - p0 >= LIMITS.window_push_gain) w.used = true; }
    } else if (a.area === 'dead' || (nDead != null && nDead < LIMITS.window_enemies)) this.closeWindow();
  }
  private closeWindow() {
    const w = this.win; this.win = null;
    if (!w || w.secs < LIMITS.window_secs) return;
    if (w.used) this.windows.used++; else { this.windows.unused++; this.windows.unusedAt.push(Math.round(w.start)); }
  }

  /** The reasons review.rs death_review() states, as booleans (null = unobservable). */
  private startDeath(x: Sample, s: Snapshot) {
    const clock = x.t!;
    const rec: DeathRecord = { clock, cause: null, family: null, killer: null, close: null, evidence: false, fallSecs: null, fromPct: null, burst: null,
      souls: null, alone: null, forwardUnseen: null, teammatesDied: null, afterRespawnSecs: null, weakEntry: null };
    const hist = this.samples.filter(p => p.t != null);
    const last = [...hist].reverse().find(p => p.area !== 'dead' && p.area !== 'unknown' && p.t! <= clock && p.t! + 30 >= clock);
    const end = last?.t ?? clock;
    const before = hist.filter(p => p.area !== 'dead' && p.t! + REVIEW.death_window_secs >= end && p.t! <= end);
    if (before.length && !before.every(p => p.area === 'unknown')) {
      rec.evidence = true;
      const known = before.filter(p => p.hp != null);
      if (known.length) {
        const peak = Math.max(...known.map(p => p.hp!));
        const h = known.find(p => p.hp! >= LIMITS.burst_from && p.hp! >= peak - 0.03);
        if (h) {
          const secs = Math.max(2, (h.t === end ? clock : end) - h.t!);
          rec.fallSecs = Math.round(secs); rec.fromPct = Math.round(h.hp! * 100); rec.burst = secs <= LIMITS.burst_secs;
        } else rec.burst = false;
      }
      const first = before.find(p => p.alliesDead != null)?.alliesDead, lastDead = [...before].reverse().find(p => p.alliesDead != null)?.alliesDead;
      if (first != null && lastDead != null) rec.teammatesDied = Math.max(0, lastDead - first);
      const recent = [...before].reverse().filter(p => p.nearestAlly != null).slice(0, 4);
      if (recent.length) rec.alone = recent.every(p => p.nearestAlly! > REVIEW.alone_distance);
      const located = before.filter(p => p.you && p.unseen != null);
      if (located.length) rec.forwardUnseen = located.some(p => p.you!.y < LIMITS.enemy_half_y && p.unseen! >= REVIEW.unseen_enemies);
      const minute = hist.filter(p => p.t! + LIMITS.weak_entry_secs >= end && p.t! <= end && p.hp != null).map(p => p.hp!);
      if (minute.length >= 3) rec.weakEntry = minute.every(h => h < LIMITS.weak_entry_ratio);
      rec.souls = [...before].reverse().find(p => p.souls != null)?.souls ?? null;
      const effects = (s as Snapshot & { status_effects?: string[] }).status_effects;
      if (Array.isArray(effects) && effects.length) rec.effects = effects.filter(e => typeof e === 'string').slice(0, 6);
    }
    if (this.respawnedAt != null && end >= this.respawnedAt) rec.afterRespawnSecs = Math.round(end - this.respawnedAt);
    if (rec.souls == null && x.souls != null) rec.souls = x.souls;
    this.open = { rec, startT: clock };
    if (this.lowOpen != null) { this.lowHp.died++; this.lowOpen = null; }
  }
  private closeDeath() { if (this.open) { this.deaths.push(this.open.rec); this.open = null; } }

  /** The match so far (does not close it). */
  record(endedAt: number, partial = false): MatchRecord {
    const hero = [...this.heroVotes.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
    const deaths = [...this.deaths, ...(this.open ? [this.open.rec] : [])].map(d => ({ ...d }));
    const windows = { ...this.windows, unusedAt: [...this.windows.unusedAt] };
    if (this.win && this.win.secs >= LIMITS.window_secs) { if (this.win.used) windows.used++; else { windows.unused++; windows.unusedAt.push(Math.round(this.win.start)); } }
    const w = this.worth; const span = w.first && w.last ? w.last.t - w.first.t : 0;
    const round = (r: Record<string, number>) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, Math.round(v)]));
    return { v: 1, id: `${this.startedAt.toString(36)}-${(this.startClock ?? 0).toString(36)}`, source: this.source, startedAt: this.startedAt, endedAt,
      hero, matchId: this.matchId, startClock: this.startClock, endClock: this.lastRead ?? (this.clock == null ? null : Math.round(this.clock)), observedSecs: Math.round(this.observedSecs), result: this.result,
      ...(partial ? { partial: true } : {}),
      time: round(this.time) as Record<Area, number>, deaths, fights: { ...this.fights }, windows,
      souls: round(this.souls) as MatchRecord['souls'], lowHp: round(this.lowHp) as MatchRecord['lowHp'], deepAwaySecs: Math.round(this.deepAwaySecs),
      farm: { perMin: span >= 120 ? Math.round((w.last!.v - w.first!.v) * 60 / span) : null }, kda: this.kda };
  }
}

// ---------------------------------------------------------------------------------------------
// Classifier
// ---------------------------------------------------------------------------------------------
export type Trend = 'improving' | 'worse' | 'same' | 'new';
export interface Trait {
  id: string; kind: 'strength' | 'weakness'; label: string; evidence: string; severity: 1 | 2 | 3;
  confidence: number; confidenceLabel: 'low' | 'medium' | 'high'; trend: Trend; tip: string; focus: string;
  value: number; threshold: number; matches: number;
}
export interface MatchRow {
  id: string; date: number; hero: string | null; heroName: string; source: MatchRecord['source']; result: MatchRecord['result'];
  minutes: number; deaths: number; deadPct: number | null; fightsJoined: number; fightsMissed: number;
  unspentAtDeath: string; topKiller: string | null; partial: boolean;
}
export interface PlayerSummary {
  matches: number; hero: string | null; heroName: string | null; heroes: Array<{ hero: string; name: string; matches: number }>;
  observedMinutes: number; deathsPer10: number | null; deadPct: number | null; farmPerMin: number | null;
  strengths: Trait[]; weaknesses: Trait[]; rows: MatchRow[];
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
const pct = (n: number, d: number) => d > 0 ? Math.round(n * 100 / d) : 0;
const known = (m: MatchRecord) => m.time.lane + m.time.jungle + m.time.base + m.time.dead;
const alive = (m: MatchRecord) => m.time.lane + m.time.jungle + m.time.base;
const k = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k` : String(Math.round(n));
/** Recency weight: the newest match counts 1, each older one 0.85 times the next. */
const weight = (age: number) => Math.pow(0.85, age);

interface Ratio { num: number; den: number }
/** Weighted ratio over matches (newest first); null when no match had evidence. */
function ratio(ms: MatchRecord[], f: (m: MatchRecord) => Ratio | null): (Ratio & { value: number; n: number; raw: Ratio }) | null {
  let num = 0, den = 0, n = 0, rn = 0, rd = 0;
  ms.forEach((m, age) => { const r = f(m); if (!r || r.den <= 0) return; const w = weight(age); num += w * r.num; den += w * r.den; rn += r.num; rd += r.den; n++; });
  return den > 0 ? { num, den, value: num / den, n, raw: { num: rn, den: rd } } : null;
}
/** Newest three matches against the older ones. */
function trend(ms: MatchRecord[], f: (m: MatchRecord) => Ratio | null, lowerBetter: boolean): Trend {
  const usable = ms.filter(m => { const r = f(m); return r && r.den > 0; });
  if (usable.length < 2) return 'new';
  const cut = Math.min(3, Math.ceil(usable.length / 2));
  const a = ratio(usable.slice(0, cut), f), b = ratio(usable.slice(cut, 10), f);
  if (!a || !b) return 'new';
  const base = Math.max(Math.abs(b.value), 0.05);
  const delta = (a.value - b.value) / base;
  if (Math.abs(delta) < 0.15) return 'same';
  return (delta < 0) === lowerBetter ? 'improving' : 'worse';
}
/** Two trends of one trait: kept only when they do not contradict each other. */
function agree(a: Trend | null, b: Trend | null): Trend {
  if (a == null || a === 'new') return b ?? 'new';
  if (b == null || b === 'new' || a === b) return a;
  return a === 'same' ? b : b === 'same' ? a : 'same';
}
function confidence(events: number, needed: number, matches: number): { confidence: number; confidenceLabel: Trait['confidenceLabel'] } {
  const c = Math.min(1, events / needed) * (matches >= 3 ? 1 : matches === 2 ? 0.85 : 0.7);
  const v = Math.round(c * 100) / 100;
  return { confidence: v, confidenceLabel: v >= 0.75 ? 'high' : v >= 0.45 ? 'medium' : 'low' };
}
const severity = (value: number, threshold: number): 1 | 2 | 3 => { const r = threshold > 0 ? value / threshold : 1; return r >= 1.6 ? 3 : r >= 1.25 ? 2 : 1; };

export class PlayerProfileModel {
  names: Record<string, string> = {};
  heroName(id: string | null | undefined) { return id ? this.names[id] || FALLBACK_NAMES[id] || id.replace(/^hero_/, '').replaceAll('_', ' ') : 'Unknown hero'; }

  /** Traits for matches (newest first). */
  traits(all: MatchRecord[]): { strengths: Trait[]; weaknesses: Trait[] } {
    const ms = all.slice(0, 10);
    const out: Trait[] = [];
    if (!ms.length) return { strengths: [], weaknesses: [] };
    const hero = mode(ms.map(m => m.hero));
    const heroName = this.heroName(hero);
    const longRange = hero != null && LONG_RANGE.has(hero), frontline = hero != null && FRONTLINE.has(hero);
    const deathsAll = ms.flatMap(m => m.deaths);
    const push = (t: Omit<Trait, 'confidence' | 'confidenceLabel'> & { events: number; needed: number }) => {
      const { events, needed, ...rest } = t; out.push({ ...rest, ...confidence(events, needed, t.matches) });
    };

    // 1. Dying: share of readable time spent dead (review.json pattern_deaths: >= 60 s dead in a
    //    5-minute window; rec4 ~25% / rec5 17-21% were judged too much). Weak >= 15%, strong < 7%.
    const deadF = (m: MatchRecord): Ratio | null => known(m) >= 300 ? { num: m.time.dead, den: known(m) } : null;
    const dead = ratio(ms, deadF);
    const per10 = ratio(ms, m => known(m) >= 300 ? { num: m.deaths.length, den: known(m) / 600 } : null);
    if (dead && per10) {
      const v = dead.value * 100, d10 = per10.value;
      const evidence = `${Math.round(v)}% of readable time dead; ${d10.toFixed(1)} deaths per 10 min (${per10.raw.num} deaths in ${Math.round(per10.raw.den * 10)} min, ${dead.n} match${dead.n === 1 ? '' : 'es'}).`;
      if (v >= 15 || d10 >= 2) push({ id: 'dying', kind: 'weakness', label: 'Dying too often', evidence, severity: severity(Math.max(v / 15, d10 / 2), 1), value: Math.round(v), threshold: 15, matches: dead.n,
        trend: trend(ms, deadF, true), events: per10.raw.num, needed: 6,
        tip: `Play for zero deaths: fight from behind your front line and leave on the first big hit.${longRange ? ` ${heroName} is long-range: stay at max range and let the front line take space.` : ''}`,
        focus: longRange ? `Profile focus: you spend ${Math.round(v)}% of the game dead. Stay at max range behind your front line.` : `Profile focus: you spend ${Math.round(v)}% of the game dead. Fight from behind your front line.` });
      else if (v < 7 && d10 < 1 && per10.raw.den >= 1.5) push({ id: 'survival', kind: 'strength', label: 'Stays alive', evidence, severity: 1, value: Math.round(v), threshold: 7, matches: dead.n,
        trend: trend(ms, deadF, true), events: per10.raw.den, needed: 3, tip: 'Keep it up: being alive is what lets you take the next objective.', focus: '' });
    }

    // 2. Unspent souls (common-mistakes.md #6; review.json souls_at_death 1200, hoard_souls 3000,
    //    pattern_hoard = >= 60 s of a 5-minute window, i.e. 20%).
    const atDeathF = (m: MatchRecord): Ratio | null => { const d = m.deaths.filter(x => x.souls != null); return d.length ? { num: d.filter(x => x.souls! >= REVIEW.souls_at_death).length, den: d.length } : null; };
    const hoardF = (m: MatchRecord): Ratio | null => m.souls.knownSecs >= 120 ? { num: m.souls.hoardSecs, den: m.souls.knownSecs } : null;
    const atDeath = ratio(ms, atDeathF), hoard = ratio(ms, hoardF);
    const soulsAt = deathsAll.map(d => d.souls).filter(finite);
    const peak = Math.max(0, ...ms.map(m => m.souls.peak));
    const weakAt = atDeath && atDeath.raw.den >= 2 && atDeath.value >= 0.5, weakHoard = hoard && hoard.value >= 0.2;
    if (weakAt || weakHoard) {
      const parts: string[] = [];
      if (atDeath) parts.push(`died holding ${k(REVIEW.souls_at_death)}+ souls in ${atDeath.raw.num} of ${atDeath.raw.den} deaths${soulsAt.length ? ` (${k(Math.min(...soulsAt))}-${k(Math.max(...soulsAt))})` : ''}`);
      if (hoard) parts.push(`over ${k(REVIEW.hoard_souls)} unspent for ${pct(hoard.num, hoard.den)}% of alive time${peak ? `, peak ${k(peak)}` : ''}`);
      const v = Math.max(atDeath ? atDeath.value / 0.5 : 0, hoard ? hoard.value / 0.2 : 0);
      push({ id: 'unspent_souls', kind: 'weakness', label: 'Unspent souls', evidence: cap(parts.join('; ')) + '.', severity: severity(v, 1), value: Math.round((atDeath?.value ?? hoard!.value) * 100), threshold: 50,
        matches: Math.max(atDeath?.n ?? 0, hoard?.n ?? 0), trend: agree(atDeath ? trend(ms, atDeathF, true) : null, hoard ? trend(ms, hoardF, true) : null), events: (atDeath?.raw.den ?? 0) + (hoard ? hoard.raw.den / 300 : 0), needed: 6,
        tip: 'Spend at 3,000 souls: shop when your wave is safe, and never walk back into a fight with souls in your pocket.', focus: 'Profile focus: spend your souls at 3,000 before you go back in.' });
    } else if (atDeath && hoard && atDeath.raw.den >= 3 && atDeath.value <= 0.15 && hoard.value <= 0.05) {
      push({ id: 'spending', kind: 'strength', label: 'Spends souls on time', evidence: `Only ${atDeath.raw.num} of ${atDeath.raw.den} deaths with ${k(REVIEW.souls_at_death)}+ souls; over ${k(REVIEW.hoard_souls)} for ${pct(hoard.num, hoard.den)}% of alive time.`,
        severity: 1, value: Math.round(atDeath.value * 100), threshold: 15, matches: atDeath.n, trend: trend(ms, atDeathF, true), events: atDeath.raw.den, needed: 5, tip: 'Keep shopping on time.', focus: '' });
    }

    // 3. Death-context reasons (review.rs death_review): each needs >= 2 deaths with the reason.
    const reasons: Array<{ id: string; label: string; f: (d: DeathRecord) => boolean | null; min: number; text: (n: number, of: number) => string; tip: string; focus: string }> = [
      { id: 'alone_deaths', label: 'Dies alone', f: d => d.alone, min: 0.35, text: (n, of) => `${n} of ${of} deaths with no teammate within a quarter of the map.`,
        tip: `Stay within reach of a teammate, especially when enemies are missing (common-mistakes.md #16).${longRange ? ` ${heroName} needs a front line to shoot from behind.` : ''}`, focus: 'Profile focus: your deaths come alone. Stay near a teammate.' },
      { id: 'forward_unseen', label: 'Pushes in with enemies missing', f: d => d.forwardUnseen, min: 0.3, text: (n, of) => `${n} of ${of} deaths in the enemy half with ${REVIEW.unseen_enemies}+ enemies unseen.`,
        tip: "When enemies are missing from the minimap, don't push past your wave (common-mistakes.md #22).", focus: "Profile focus: with enemies missing, don't cross into their half." },
      { id: 'fight_lost', label: 'Stays in lost fights', f: d => d.teammatesDied == null ? null : d.teammatesDied > 0, min: 0.3, text: (n, of) => `${n} of ${of} deaths came right after teammates died in front of you.`,
        tip: 'When teammates fall in front of you the fight is lost: retreat together (common-mistakes.md #19).', focus: 'Profile focus: when teammates fall in front of you, back off together.' },
      { id: 'respawn_deaths', label: 'Dies soon after respawning', f: d => d.afterRespawnSecs == null ? null : d.afterRespawnSecs <= LIMITS.soon_after_respawn_secs, min: 0.35, text: (n, of) => `${n} of ${of} deaths within ${LIMITS.soon_after_respawn_secs} s of respawning.`,
        tip: 'After a respawn, spend your souls and walk out with your team or catch a wave near your guardian.', focus: 'Profile focus: after you respawn, walk out with a teammate.' },
      { id: 'weak_entry', label: 'Goes back in hurt', f: d => d.weakEntry, min: 0.3, text: (n, of) => `${n} of ${of} deaths after 30 s under 60% health.`,
        tip: 'Heal to full before you go back in; shopping does not heal you.', focus: 'Profile focus: heal to full before you go back in.' },
      { id: 'burst', label: 'Gets burst down', f: d => d.burst, min: 0.6, text: (n, of) => `${n} of ${of} deaths fell from healthy to dead in ${LIMITS.burst_secs} s or less.`,
        tip: longRange ? `${heroName} is long-range: keep distance, leave on the first big hit, and stay behind your front line.` : frontline ? 'Save your escape for the burst and add a defensive item.' : 'Leave on the first big hit, not at low health, and stay behind your front line.',
        focus: longRange ? `Profile focus: you get burst. ${heroName} is long-range, keep your distance.` : 'Profile focus: leave on the first big hit.' },
    ];
    for (const r of reasons) {
      const f = (m: MatchRecord): Ratio | null => { const d = m.deaths.map(r.f).filter(v => v != null); return d.length ? { num: d.filter(Boolean).length, den: d.length } : null; };
      const x = ratio(ms, f); if (!x || x.raw.num < 2 || x.value < r.min) continue;
      push({ id: r.id, kind: 'weakness', label: r.label, evidence: r.text(x.raw.num, x.raw.den), severity: severity(x.value, r.min), value: Math.round(x.value * 100), threshold: Math.round(r.min * 100), matches: x.n,
        trend: trend(ms, f, true), events: x.raw.num, needed: 4, tip: r.tip, focus: r.focus });
    }

    // 4. Close-range killers and repeat killers (common-mistakes.md #8, #21; rec5 every death in
    //    melee range). Range is only claimed for causes the killer table knows.
    const closeF = (m: MatchRecord): Ratio | null => { const d = m.deaths.filter(x => x.close != null); return d.length ? { num: d.filter(x => x.close).length, den: d.length } : null; };
    const close = ratio(ms, closeF);
    const counts = new Map<string, { n: number; hero: string | null; inMatch: number }>();
    ms.forEach(m => { const per = new Map<string, number>(); for (const d of m.deaths) if (d.family) per.set(d.family, (per.get(d.family) || 0) + 1);
      for (const [fam, n] of per) { const c = counts.get(fam) || { n: 0, hero: m.deaths.find(d => d.family === fam)?.killer ?? null, inMatch: 0 }; c.n += n; c.inMatch = Math.max(c.inMatch, n); counts.set(fam, c); } });
    const top = [...counts.entries()].sort((a, b) => b[1].n - a[1].n);
    if (close && close.raw.num >= 2 && close.value >= 0.5) {
      const fams = top.filter(([f]) => deathsAll.some(d => d.family === f && d.close)).slice(0, 3).map(([f, c]) => `${f} x${c.n}`).join(', ');
      push({ id: 'close_range', kind: 'weakness', label: 'Killed at close range', evidence: `${close.raw.num} of ${close.raw.den} deaths with a known cause came from melee range${fams ? ` (${fams})` : ''}.`,
        severity: severity(close.value, 0.5), value: Math.round(close.value * 100), threshold: 50, matches: close.n, trend: trend(ms, closeF, true), events: close.raw.num, needed: 4,
        tip: longRange ? `${heroName} is long-range: keep distance from divers and auras, back out to max range when they close in, and save your escape for them.` : 'Keep your distance from melee divers and auras; buy a defensive item against them.',
        focus: longRange ? `Profile focus: you die in melee range. ${heroName} is long-range, keep your distance.` : 'Profile focus: you die in melee range. Keep your distance.' });
    }
    const repeats = top.filter(([, c]) => c.inMatch >= 2 || c.n >= 3);
    if (repeats.length) {
      // The newest match's repeat killer first (most actionable), then the others.
      const latest = repeats.find(([f]) => ms[0].deaths.filter(d => d.family === f).length >= 2) ?? repeats[0];
      const [fam, c] = latest; const who = c.hero ? this.heroName(c.hero) : null;
      const others = repeats.filter(r => r !== latest).slice(0, 2).map(([f, o]) => `${o.hero ? this.heroName(o.hero) : f} x${o.n}`);
      push({ id: 'repeat_killer', kind: 'weakness', label: 'Same killer again', evidence: `${fam} killed you ${c.n} times${c.inMatch >= 2 ? ` (${c.inMatch} in one match)` : ''}${others.length ? `; also ${others.join(', ')}` : ''}.`,
        severity: severity(c.n, 2), value: c.n, threshold: 2, matches: ms.filter(m => m.deaths.some(d => d.family === fam)).length, trend: 'new', events: c.n, needed: 4,
        tip: who ? `Check the damage panel: ${who} keeps killing you. Itemise against ${who} and keep out of reach.` : `Check what is killing you (${fam}) and buy a counter or a defensive item (common-mistakes.md #8).`,
        focus: who ? `Profile focus: ${who} keeps killing you. Keep away and itemise against ${who}.` : `Profile focus: ${fam} keeps killing you. Buy a counter.` });
    }

    // 5. Enemy-dead windows (review.json pattern_window; common-mistakes.md #10).
    const winF = (m: MatchRecord): Ratio | null => m.windows.used + m.windows.unused ? { num: m.windows.unused, den: m.windows.used + m.windows.unused } : null;
    const win = ratio(ms, winF);
    if (win && win.raw.num >= 1 && win.value >= 0.5) {
      const at = ms[0].windows.unusedAt.slice(0, 2).map(c => `${Math.floor(c / 60)}:${String(c % 60).padStart(2, '0')}`).join(', ');
      push({ id: 'unused_windows', kind: 'weakness', label: 'Enemy-dead windows unused', evidence: `${win.raw.num} of ${win.raw.den} windows with ${LIMITS.window_enemies}+ enemies dead for ${LIMITS.window_secs}+ s stayed on your half${at ? ` (latest match: ${at})` : ''}.`,
        severity: severity(win.value, 0.5), value: Math.round(win.value * 100), threshold: 50, matches: win.n, trend: trend(ms, winF, true), events: win.raw.den, needed: 4,
        tip: 'When two or more enemies are dead, push the deepest wave or take an objective (Walker, Midboss) before they respawn.', focus: 'Profile focus: when two enemies are dead, push a lane or take an objective.' });
    } else if (win && win.raw.den - win.raw.num >= 2 && win.value <= 0.34) {
      push({ id: 'used_windows', kind: 'strength', label: 'Uses enemy-dead windows', evidence: `${win.raw.den - win.raw.num} of ${win.raw.den} enemy-dead windows used.`, severity: 1, value: Math.round((1 - win.value) * 100), threshold: 67,
        matches: win.n, trend: trend(ms, winF, true), events: win.raw.den, needed: 4, tip: 'Keep converting kills into objectives.', focus: '' });
    }

    // 6. Fight attendance (review.json pattern_missed: > a third of the map away; #25).
    const fightF = (m: MatchRecord): Ratio | null => m.fights.joined + m.fights.missed ? { num: m.fights.missed, den: m.fights.joined + m.fights.missed } : null;
    const fights = ratio(ms, fightF);
    if (fights && fights.raw.den >= 4 && fights.value > 0.5) {
      push({ id: 'missed_fights', kind: 'weakness', label: 'Misses team fights', evidence: `Joined ${fights.raw.den - fights.raw.num} of ${fights.raw.den} team fights; ${fights.raw.num} were more than a third of the map away.`,
        severity: severity(fights.value, 0.5), value: Math.round(fights.value * 100), threshold: 50, matches: fights.n, trend: trend(ms, fightF, true), events: fights.raw.den, needed: 10,
        tip: 'When your team groups, shove your wave and go help.', focus: 'Profile focus: when your team groups, shove your wave and join them.' });
    } else if (fights && fights.raw.den >= 5 && fights.value <= 0.25) {
      push({ id: 'joins_fights', kind: 'strength', label: 'Shows up for fights', evidence: `Joined ${fights.raw.den - fights.raw.num} of ${fights.raw.den} team fights.`, severity: 1, value: Math.round((1 - fights.value) * 100), threshold: 75,
        matches: fights.n, trend: trend(ms, fightF, true), events: fights.raw.den, needed: 10, tip: 'Good team presence. Keep catching waves between fights.', focus: '' });
    }

    // 7. Jungle vs lanes (review.json pattern_jungle: >= 50% of alive time; jungle-tactics.md:
    //    camps between waves). A low jungle share is not a weakness.
    const jungleF = (m: MatchRecord): Ratio | null => alive(m) >= 300 ? { num: m.time.jungle, den: alive(m) } : null;
    const laneF = (m: MatchRecord): Ratio | null => alive(m) >= 300 ? { num: m.time.lane, den: alive(m) } : null;
    const jungle = ratio(ms, jungleF), lane = ratio(ms, laneF);
    if (jungle && lane && (jungle.value >= 0.5 || (jungle.value > lane.value && jungle.value >= 0.35))) {
      push({ id: 'jungle_heavy', kind: 'weakness', label: 'Too much jungle', evidence: `${pct(jungle.num, jungle.den)}% of alive time in the jungle vs ${pct(lane.num, lane.den)}% in lanes.`, severity: severity(jungle.value, 0.5),
        value: Math.round(jungle.value * 100), threshold: 50, matches: jungle.n, trend: trend(ms, jungleF, true), events: jungle.raw.den / 300, needed: 4,
        tip: 'Take camps between waves, then go back to a lane or your team.', focus: 'Profile focus: take camps between waves, then go back to a lane.' });
    } else if (lane && jungle && lane.value >= 0.6 && lane.raw.den >= 600) {
      push({ id: 'lane_presence', kind: 'strength', label: 'Keeps lanes covered', evidence: `${pct(lane.num, lane.den)}% of alive time on a lane, ${pct(jungle.num, jungle.den)}% in the jungle.`, severity: 1, value: Math.round(lane.value * 100), threshold: 60,
        matches: lane.n, trend: trend(ms, laneF, false), events: lane.raw.den / 300, needed: 4, tip: 'Good wave presence. Take a camp only between waves.', focus: '' });
    }

    // 8. Waves left deep while in the jungle (review.json pattern_waves: >= 30 s per 5 minutes).
    const deepF = (m: MatchRecord): Ratio | null => alive(m) >= 300 ? { num: m.deepAwaySecs, den: alive(m) / 600 } : null;
    const deep = ratio(ms, deepF);
    if (deep && deep.value >= 60 && deep.raw.num >= 60) push({ id: 'deep_waves', kind: 'weakness', label: 'Leaves waves deep', evidence: `A wave sat deep on your side for ${Math.round(deep.value)} s per 10 min while you were in the jungle.`,
      severity: severity(deep.value, 60), value: Math.round(deep.value), threshold: 60, matches: deep.n, trend: trend(ms, deepF, true), events: deep.raw.num / 60, needed: 4,
      tip: 'Catch the wave that is pushing into your base before you take camps.', focus: 'Profile focus: catch deep waves before taking camps.' });

    // 9. Low health: leaving at < 30% (moba-fundamentals.md "retreat when low").
    const lowF = (m: MatchRecord): Ratio | null => m.lowHp.died + m.lowHp.survived ? { num: m.lowHp.died, den: m.lowHp.died + m.lowHp.survived } : null;
    const low = ratio(ms, lowF);
    if (low && low.raw.num >= 2 && low.value >= 0.5) push({ id: 'low_hp', kind: 'weakness', label: 'Stays in at low health', evidence: `${low.raw.num} of ${low.raw.den} times under 30% health ended in a death.`,
      severity: severity(low.value, 0.5), value: Math.round(low.value * 100), threshold: 50, matches: low.n, trend: trend(ms, lowF, true), events: low.raw.den, needed: 6,
      tip: 'Leave on the first big hit, not at low health.', focus: 'Profile focus: back off before you drop under a third of your health.' });
    else if (low && low.raw.den - low.raw.num >= 3 && low.value <= 0.25) push({ id: 'backs_off', kind: 'strength', label: 'Backs off when low', evidence: `Survived ${low.raw.den - low.raw.num} of ${low.raw.den} times under 30% health.`,
      severity: 1, value: Math.round((1 - low.value) * 100), threshold: 75, matches: low.n, trend: trend(ms, lowF, true), events: low.raw.den, needed: 6, tip: 'Good discipline at low health.', focus: '' });

    // Rank: severity x confidence x specificity (a generic symptom such as "most deaths were
    // fast" says less about what to change than time dead or unspent souls).
    const score = (t: Trait) => t.severity * t.confidence * (SPECIFICITY[t.id] ?? 1);
    const weaknesses = out.filter(t => t.kind === 'weakness').sort((a, b) => score(b) - score(a) || PRIORITY.indexOf(a.id) - PRIORITY.indexOf(b.id));
    const strengths = out.filter(t => t.kind === 'strength').sort((a, b) => b.confidence - a.confidence);
    return { strengths, weaknesses };
  }

  summary(all: MatchRecord[], hero?: string | null): PlayerSummary {
    const ms = hero ? all.filter(m => m.hero === hero) : all;
    const recent = ms.slice(0, 10);
    const heroes = [...all.reduce((m, r) => r.hero ? m.set(r.hero, (m.get(r.hero) || 0) + 1) : m, new Map<string, number>())].sort((a, b) => b[1] - a[1]).map(([h, n]) => ({ hero: h, name: this.heroName(h), matches: n }));
    const kn = sum(recent.map(known)), deaths = sum(recent.map(m => m.deaths.length));
    const farm = recent.map(m => m.farm.perMin).filter(finite);
    const { strengths, weaknesses } = this.traits(recent);
    const main = hero ?? mode(recent.map(m => m.hero));
    return { matches: ms.length, hero: main, heroName: main ? this.heroName(main) : null, heroes, observedMinutes: Math.round(sum(recent.map(m => m.observedSecs)) / 60),
      deathsPer10: kn >= 300 ? Math.round(deaths * 6000 / kn) / 10 : null, deadPct: kn >= 300 ? pct(sum(recent.map(m => m.time.dead)), kn) : null,
      farmPerMin: farm.length ? Math.round(sum(farm) / farm.length) : null, strengths, weaknesses, rows: ms.map(m => this.row(m)) };
  }

  row(m: MatchRecord): MatchRow {
    const withSouls = m.deaths.filter(d => d.souls != null);
    const fam = mode(m.deaths.map(d => d.family));
    const killer = fam ? (() => { const h = m.deaths.find(d => d.family === fam)?.killer; const n = m.deaths.filter(d => d.family === fam).length; return `${h ? this.heroName(h) : fam} x${n}`; })() : null;
    return { id: m.id, date: m.startedAt, hero: m.hero, heroName: this.heroName(m.hero), source: m.source, result: m.result, minutes: Math.round(m.observedSecs / 6) / 10,
      deaths: m.deaths.length, deadPct: known(m) >= 60 ? pct(m.time.dead, known(m)) : null, fightsJoined: m.fights.joined, fightsMissed: m.fights.missed,
      unspentAtDeath: withSouls.length ? `${withSouls.filter(d => d.souls! >= REVIEW.souls_at_death).length}/${withSouls.length} · avg ${k(sum(withSouls.map(d => d.souls!)) / withSouls.length)}` : '—',
      topKiller: killer, partial: m.partial === true };
  }

  /** Compact text for an AI prompt (<= 600 characters). */
  promptText(all: MatchRecord[], hero?: string | null): string | null {
    const s = this.summary(all, hero);
    if (!s.matches) return null;
    const head = `Player profile (${s.matches} match${s.matches === 1 ? '' : 'es'}${s.heroName ? `, mostly ${s.heroName}` : ''}${s.deadPct != null ? `, ${s.deadPct}% dead, ${s.deathsPer10}/10min deaths` : ''}).`;
    const weak = s.weaknesses.slice(0, 4).map(t => `${t.label}: ${t.evidence.replace(/\.$/, '')}${t.trend === 'improving' || t.trend === 'worse' ? ` [${t.trend}]` : ''}`);
    const strong = s.strengths.slice(0, 2).map(t => t.label);
    let text = head;
    const add = (part: string) => { if ((text + ' ' + part).length <= 600) text += ' ' + part; };
    if (weak.length) { add('Weak:'); for (const w of weak) add(w.length > 150 ? w.slice(0, 147) + '...;' : w + ';'); }
    if (strong.length) add(`Strong: ${strong.join(', ')}.`);
    if (s.weaknesses[0]) add(`Focus: ${s.weaknesses[0].tip}`.slice(0, 160));
    return text.replace(/;\s*(Strong|Focus|$)/g, '. $1').trim().slice(0, 600);
  }
}
const SPECIFICITY: Record<string, number> = { dying: 1.3, unspent_souls: 1.1, burst: 0.6, low_hp: 0.7, respawn_deaths: 0.8, repeat_killer: 0.9 };
const PRIORITY = ['dying', 'unspent_souls', 'close_range', 'repeat_killer', 'alone_deaths', 'unused_windows', 'forward_unseen', 'fight_lost', 'respawn_deaths', 'weak_entry', 'burst', 'missed_fights', 'low_hp', 'jungle_heavy', 'deep_waves'];
function mode<T>(xs: Array<T | null | undefined>): T | null {
  const m = new Map<T, number>(); for (const x of xs) if (x != null) m.set(x, (m.get(x) || 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}
const cap = (s: string) => s ? s[0].toUpperCase() + s.slice(1) : s;

// ---------------------------------------------------------------------------------------------
// Persistence + live API
// ---------------------------------------------------------------------------------------------
export interface KeyValueStore { getItem(key: string): string | null; setItem(key: string, value: string): void; removeItem(key: string): void }
export interface StoredProfile { schema: number; matches: MatchRecord[]; current?: MatchRecord | null; lastFocusAt?: number | null }

const num = (v: unknown, fallback = 0) => finite(v) ? v : fallback;
const numOrNull = (v: unknown) => finite(v) ? v : null;
const bool = (v: unknown) => typeof v === 'boolean' ? v : null;
const str = (v: unknown, n = 60) => typeof v === 'string' ? v.slice(0, n) : null;
/** Validate one stored/imported match; drops it when the shape is unusable. */
export function sanitizeMatch(raw: unknown): MatchRecord | null {
  if (!raw || typeof raw !== 'object') return null;
  const m = raw as Record<string, any>;
  if (!m.time || typeof m.time !== 'object' || !Array.isArray(m.deaths)) return null;
  const time = { lane: num(m.time.lane), jungle: num(m.time.jungle), base: num(m.time.base), dead: num(m.time.dead), unknown: num(m.time.unknown) };
  const deaths: DeathRecord[] = m.deaths.slice(0, 60).filter((d: unknown) => d && typeof d === 'object').map((d: Record<string, unknown>) => ({
    clock: num(d.clock), cause: str(d.cause), family: str(d.family), killer: str(d.killer, 40), close: bool(d.close), evidence: d.evidence !== false,
    fallSecs: numOrNull(d.fallSecs), fromPct: numOrNull(d.fromPct), burst: bool(d.burst), souls: numOrNull(d.souls), alone: bool(d.alone),
    forwardUnseen: bool(d.forwardUnseen), teammatesDied: numOrNull(d.teammatesDied), afterRespawnSecs: numOrNull(d.afterRespawnSecs), weakEntry: bool(d.weakEntry),
    ...(Array.isArray(d.effects) ? { effects: d.effects.filter((e: unknown) => typeof e === 'string').slice(0, 6) } : {}) }));
  const o = (v: unknown) => v && typeof v === 'object' ? v as Record<string, unknown> : {};
  const f = o(m.fights), w = o(m.windows), s = o(m.souls), l = o(m.lowHp);
  return { v: 1, id: str(m.id, 40) || `import-${Math.random().toString(36).slice(2, 10)}`, source: m.source === 'replay' || m.source === 'import' ? m.source : 'live',
    startedAt: num(m.startedAt, Date.now()), endedAt: num(m.endedAt, num(m.startedAt, Date.now())), hero: typeof m.hero === 'string' && /^hero_[a-z_]+$/.test(m.hero) ? m.hero : null,
    matchId: numOrNull(m.matchId), startClock: numOrNull(m.startClock), endClock: numOrNull(m.endClock), observedSecs: num(m.observedSecs, Object.values(time).reduce((a, b) => a + b, 0)),
    result: m.result === 'win' || m.result === 'loss' ? m.result : null, ...(m.partial === true ? { partial: true } : {}), ...(typeof m.fingerprint === 'string' ? { fingerprint: m.fingerprint.slice(0, 80) } : {}),
    time, deaths, fights: { joined: num(f.joined), missed: num(f.missed), whileDead: num(f.whileDead) },
    windows: { used: num(w.used), unused: num(w.unused), unusedAt: Array.isArray(w.unusedAt) ? w.unusedAt.filter(finite).slice(0, 20) : [] },
    souls: { knownSecs: num(s.knownSecs), hoardSecs: num(s.hoardSecs), peak: num(s.peak) },
    lowHp: { knownSecs: num(l.knownSecs), lowSecs: num(l.lowSecs), episodes: num(l.episodes), survived: num(l.survived), died: num(l.died) },
    deepAwaySecs: num(m.deepAwaySecs), farm: { perMin: numOrNull(o(m.farm).perMin) },
    kda: m.kda && typeof m.kda === 'object' ? { kills: numOrNull(m.kda.kills), deaths: numOrNull(m.kda.deaths), assists: numOrNull(m.kda.assists) } : null };
}
/** Versioned schema. Schema 0 (pre-release) was a bare array of matches. */
export function migrate(raw: unknown): StoredProfile {
  if (Array.isArray(raw)) raw = { schema: 1, matches: raw };
  if (!raw || typeof raw !== 'object') return { schema: PLAYER_PROFILE_SCHEMA, matches: [] };
  const r = raw as Record<string, unknown>;
  if (finite(r.schema) && r.schema > PLAYER_PROFILE_SCHEMA) throw new Error(`Player profile schema ${r.schema} is newer than this app (${PLAYER_PROFILE_SCHEMA}).`);
  const matches = (Array.isArray(r.matches) ? r.matches : []).map(sanitizeMatch).filter((m): m is MatchRecord => !!m);
  const current = r.current ? sanitizeMatch(r.current) : null;
  return { schema: PLAYER_PROFILE_SCHEMA, matches: matches.sort((a, b) => b.startedAt - a.startedAt).slice(0, MAX_MATCHES), current, lastFocusAt: numOrNull(r.lastFocusAt) };
}

export interface ObserveEvents { matchStarted: boolean; respawned: boolean; died: boolean; ended: MatchRecord | null }
export interface FocusCue { key: string; text: string; priority: 'Normal'; cooldown_secs: number }
export const FOCUS_COOLDOWN_MS = 5 * 60_000;

/** The profile: completed matches (newest first) plus the match in progress. */
export class PlayerProfile {
  readonly model = new PlayerProfileModel();
  matches: MatchRecord[] = [];
  current: MatchAggregator | null = null;
  lastFocusAt: number | null = null;
  private store: KeyValueStore | null; private key: string; private now: () => number;
  private lastSave = 0; private resultSeen = false;
  constructor(opts: { store?: KeyValueStore | null; key?: string; now?: () => number } = {}) {
    this.store = opts.store ?? null; this.key = opts.key ?? PLAYER_PROFILE_KEY; this.now = opts.now ?? (() => Date.now());
  }
  setHeroNames(names: Record<string, string>) { this.model.names = names; }

  load(): this {
    if (!this.store) return this;
    try {
      const text = this.store.getItem(this.key); if (!text) return this;
      const data = migrate(JSON.parse(text));
      this.matches = data.matches; this.lastFocusAt = data.lastFocusAt ?? null;
      // A match left open by a closed tab is kept as a partial match.
      if (data.current && (known(data.current) >= LIMITS.min_match_secs || data.current.deaths.length)) this.addMatch({ ...data.current, partial: true }, false);
    } catch { /* corrupt or newer data: keep it untouched in storage, start empty in memory */ this.store = null; }
    return this;
  }
  save(): boolean {
    if (!this.store) return false;
    const data: StoredProfile = { schema: PLAYER_PROFILE_SCHEMA, matches: this.matches.slice(0, MAX_MATCHES), lastFocusAt: this.lastFocusAt,
      current: this.current?.hasEvidence ? this.current.record(this.now(), true) : null };
    try { this.store.setItem(this.key, JSON.stringify(data)); this.lastSave = this.now(); return true; } catch { return false; }
  }

  /** Feed one snapshot. `wallMs` = when the frame was captured (null for replays without it). */
  observe(s: Snapshot, wallMs: number | null = null, source: MatchRecord['source'] = 'live'): ObserveEvents {
    const ev: ObserveEvents = { matchStarted: false, respawned: false, died: false, ended: null };
    if (!s || typeof s !== 'object') return ev;
    const wall = finite(wallMs) ? wallMs : null;
    const result = resultOf(s.screen);
    if (result) {
      if (this.current && !this.resultSeen) { if (result !== 'unknown') this.current.result = result; this.resultSeen = true; ev.ended = this.endMatch(); }
      return ev;
    }
    const read = finite(s.game_clock_secs) ? s.game_clock_secs : null;
    const cur = this.current;
    if (cur) {
      const gap = wall != null && cur.lastWall != null ? wall - cur.lastWall : null;
      const reset = read != null && cur.clock != null && cur.clock > 120 && read < 60;
      const newId = finite(s.match_id) && cur.matchId != null && digitDiff(cur.matchId, s.match_id) >= 3 && !cur.continuous(read, wall);
      const longGap = gap != null && (gap > LIMITS.wall_gap_new_match_ms || (gap > LIMITS.wall_gap_check_ms && read != null && !cur.continuous(read, wall)));
      if (reset || newId || longGap) ev.ended = this.endMatch();
    }
    if (!this.current) {
      // A match starts with readable gameplay evidence (a clock on a gameplay or death screen).
      if (read == null || (s.screen !== 'Gameplay' && s.screen !== 'Dead' && s.dead !== true)) return ev;
      this.current = new MatchAggregator(wall ?? this.now(), source); this.resultSeen = false;
      ev.matchStarted = true;
    }
    const r = this.current.add(s, wall);
    ev.respawned = r.respawned; ev.died = r.died;
    if (source === 'live' && this.now() - this.lastSave > 60_000) this.save();
    return ev;
  }
  /** Close the match in progress; kept when it had enough evidence. */
  endMatch(): MatchRecord | null {
    const cur = this.current; this.current = null;
    if (!cur || !cur.hasEvidence) return null;
    const rec = cur.record(this.now());
    this.addMatch(rec);
    return rec;
  }
  addMatch(rec: MatchRecord, persist = true) {
    if (rec.fingerprint && this.matches.some(m => m.fingerprint === rec.fingerprint)) return false;
    this.matches = [rec, ...this.matches.filter(m => m.id !== rec.id)].sort((a, b) => b.startedAt - a.startedAt).slice(0, MAX_MATCHES);
    if (persist) this.save();
    return true;
  }
  /** Build matches from a finished replay (snapshots in order) and add them. Returns added count. */
  addReplay(snapshots: Snapshot[], _label = ''): number {
    const tmp = new PlayerProfile({ now: this.now });
    for (const s of snapshots) tmp.observe(s, null, 'replay');
    tmp.endMatch();
    let added = 0;
    for (const m of tmp.matches.reverse()) {
      const fp = `replay:${m.matchId ?? ''}:${m.startClock}-${m.endClock}:${m.deaths.length}:${m.observedSecs}`;
      if (this.addMatch({ ...m, source: 'replay', fingerprint: fp }, false)) added++;
    }
    if (added) this.save();
    return added;
  }
  /** Everything to summarize: completed matches plus the current one when it has evidence. */
  private all(): MatchRecord[] {
    const cur = this.current?.hasEvidence ? [this.current.record(this.now(), true)] : [];
    return [...cur, ...this.matches];
  }
  summary(hero?: string | null): PlayerSummary { return this.model.summary(this.all(), hero); }
  promptText(hero?: string | null): string | null { return this.model.promptText(this.all(), hero); }
  /** Top weaknesses as short spoken lines. */
  focusTips(n = 1, hero?: string | null): string[] {
    return this.summary(hero).weaknesses.filter(t => t.confidence >= 0.3).slice(0, Math.max(0, n)).map(t => t.focus);
  }
  /** One focus reminder for a match start or a respawn, at most every 5 minutes. */
  focusCue(): FocusCue | null {
    const now = this.now();
    if (this.lastFocusAt != null && now - this.lastFocusAt < FOCUS_COOLDOWN_MS) return null;
    // Learned from completed matches only (the match in progress is not yet a pattern).
    const w = this.model.summary(this.matches).weaknesses.filter(t => t.confidence >= 0.3)[0];
    if (!w) return null;
    this.lastFocusAt = now;
    return { key: `player:focus:${w.id}`, text: w.focus, priority: 'Normal', cooldown_secs: FOCUS_COOLDOWN_MS / 1000 };
  }
  exportJSON(): string { return JSON.stringify({ schema: PLAYER_PROFILE_SCHEMA, exported: new Date(this.now()).toISOString(), matches: this.matches }, null, 1); }
  /** Merge an export into this profile. Returns the number of matches added. */
  importJSON(text: string): number {
    const data = migrate(JSON.parse(text));
    let added = 0;
    for (const m of data.matches) if (!this.matches.some(x => x.id === m.id)) { this.matches.push(m); added++; }
    this.matches = this.matches.sort((a, b) => b.startedAt - a.startedAt).slice(0, MAX_MATCHES);
    this.save();
    return added;
  }
  reset() { this.matches = []; this.current = null; this.lastFocusAt = null; try { this.store?.removeItem(this.key); } catch { /* ignore */ } }
}
