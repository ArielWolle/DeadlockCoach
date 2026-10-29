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
/** Item tier normally bought at a game clock (crates/engine/src/economy.rs spend_target): holding
 *  less is saving for the next item, not hoarding. */
export const spendTarget = (clock: number | null) => clock == null ? REVIEW.hoard_souls : clock < 600 ? 1600 : clock < 1200 ? 3200 : 6400;
/** Souls worth naming at a death: the tier below (economy.rs death_spend). */
export const deathSpend = (clock: number | null) => Math.max(REVIEW.souls_at_death, Math.floor(spendTarget(clock) / 2));
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
  /** Time dead / observed game time: a frame-to-frame gap longer than this is a recording gap,
   *  removed from both (rec5 recorded nothing for 7 minutes mid-match). */
  recording_gap_secs: 60,
  /** Respawn-timer reads that agree within this many seconds are one estimate (misreads drop a
   *  digit: rec4 read 9 / 5 / 7 for 49 / 45 / 27). */
  timer_cluster_secs: 2,
  /** The grey "Respawn in: 1" screen (no "Shop & Upgrade" line, scoreboard death count
   *  unchanged: rec4 26:32, rec8 39:17, rec11 25:17) is shorter than this; from this game clock
   *  on, a real respawn wait is longer (wiki: ~18 s at 10:00, 38 s at 20:00). */
  downed_secs: 10, downed_after_clock: 600,
  /** Trait eligibility (docs/player-profile.md "Which matches count"): at least 8 minutes of
   *  observed game time and 2 minutes alive in gameplay; a dead share above 50% is a misread
   *  (real matches: 7-24%). */
  trait_min_secs: 480, trait_min_alive_secs: 120, max_dead_share: 0.5,
  /** Frames without a game clock this long after the last clocked frame are menus / loading
   *  between matches, not part of the match (rec11 -> rec12: 9 minutes of shop and loading). */
  stale_unclocked_ms: 150_000,
  /** After a Victory / Defeat / post-game screen, a clocked frame within this many seconds of the
   *  final clock is the end-of-game camera, not a new match (rec10 57:55 PostGame, 58:00 Gameplay). */
  after_result_secs: 120,
  /** Urn carry: absent this long ends a carry; delivered = no death within 10 s after it ends. */
  urn_gap_secs: 8, urn_safe_secs: 10,
  /** Souls spent between the death and the respawn worth praising (one tier-2 item). */
  death_spend_min: 1500,
  /** A deathless stretch worth praising (6 minutes; respawns grow to 40+ s late), and the dead-share target spoken in the focus line. */
  deathless_secs: 360, dead_target_pct: 12,
  /** "Low health after a won fight": 2+ enemies dead in the 20 s before the death, health under 35%. */
  won_fight_secs: 20, won_fight_low_hp: 0.35,
  /** Match focus goals: chosen from this many recent counted matches, a goal needs this many
   *  events in them (or one in each of two matches), at most this many goals per match. */
  goal_matches: 3, goal_events: 2, goal_max: 2,
  /** A focus praise is never spoken this soon after a death. */
  praise_after_death_secs: 60,
} as const;

/** The respawn moment of one death from its timer reads (each = frame clock + timer): the
 *  largest group of reads within 2 s of each other (latest on a tie), its median. A read refuted
 *  by a later death screen (implied respawn before `lastDead`) is dropped: rec4 30:52 read 8, 5,
 *  4, 2, 0 for 68-60 and was still dead at 31:06. Same as review.rs respawn_estimate(). */
export function respawnEstimate(all: number[], lastDead = -Infinity): number | null {
  const ends = all.filter(e => e + 1 >= lastDead);
  if (!ends.length) return null;
  let best = ends[0], n = 0;
  for (const e of ends) { const c = ends.filter(f => Math.abs(f - e) <= LIMITS.timer_cluster_secs).length; if (c > n || (c === n && e > best)) { n = c; best = e; } }
  const group = ends.filter(f => Math.abs(f - best) <= LIMITS.timer_cluster_secs).sort((a, b) => a - b);
  return group[Math.floor(group.length / 2)];
}

export type Area = 'lane' | 'jungle' | 'base' | 'dead' | 'unknown';
export interface DeathRecord {
  clock: number; cause: string | null; family: string | null; killer: string | null; close: boolean | null;
  /** False = nothing readable was recorded in the window before the death. */
  evidence: boolean;
  fallSecs: number | null; fromPct: number | null; burst: boolean | null;
  souls: number | null; alone: boolean | null; forwardUnseen: boolean | null;
  teammatesDied: number | null; afterRespawnSecs: number | null; weakEntry: boolean | null;
  effects?: string[];
  /** Seconds dead: first death screen -> respawn (timer estimate, at most the first alive frame). */
  deadSecs?: number;
  /** Souls spent from just before the death to the respawn (shopping on the death screen). */
  spentDead?: number;
  /** Died under 35% health after a won fight (2+ enemies dead, more than ours, in the 20 s
   *  before; a health read under 35% at least 4 s before the death). null = top bar not read. */
  lowAfterWin?: boolean | null;
}
/** A match focus goal (docs/player-profile.md "Match focus goal"): measurable from the death and
 *  match records, chosen from the last 3 counted matches' weaknesses. */
export interface MatchGoal { id: string; text: string; target?: number }
export interface GoalResult extends MatchGoal { met: boolean | null; detail: string }
export interface MatchRecord {
  v: 1; id: string; source: 'live' | 'replay' | 'import'; startedAt: number; endedAt: number;
  hero: string | null; matchId: number | null; startClock: number | null; endClock: number | null;
  observedSecs: number; result: 'win' | 'loss' | null; partial?: boolean; fingerprint?: string;
  /** Area shares: each sample stands for the gap to the next one, capped at 10 s. */
  time: Record<Area, number>;
  /** The dead-share metric (docs/player-profile.md "Time dead"): observed game seconds (first to
   *  last clocked frame, recording gaps > 60 s removed; menus and unreadable frames count) and
   *  seconds dead in them. Absent on matches stored before it (see spanOf / deadOf). */
  spanSecs?: number; deadSecs?: number;
  /** Alive gameplay seconds whose location was not read (live full scans rarely locate you). */
  fieldSecs?: number;
  deaths: DeathRecord[];
  fights: { joined: number; missed: number; whileDead: number };
  /** `usedAt` / `base` (windows used inside their base) are absent on older records. */
  windows: { used: number; unused: number; unusedAt: number[]; usedAt?: number[]; base?: number };
  /** Soul Urn carries (status effect "carrying_urn"): delivered = carry ended alive. */
  urn?: { carried: number; delivered: number; lost: number; at: Array<[number, number]> };
  /** Observed team net worth (ally - enemy): first clock of the lead that lasted to the end. */
  team?: { reads: number; leadFrom: number | null; maxLead: number; lastLead: number | null };
  souls: { knownSecs: number; hoardSecs: number; peak: number };
  lowHp: { knownSecs: number; lowSecs: number; episodes: number; survived: number; died: number };
  deepAwaySecs: number;
  farm: { perMin: number | null };
  kda: { kills: number | null; deaths: number | null; assists: number | null } | null;
  /** The match focus goals chosen at its start and how they were scored at its end. */
  goals?: GoalResult[];
}

/** 'field' = alive in gameplay, location not read (kept out of the lane / jungle shares). */
type SampleArea = Area | 'field';
interface Sample {
  t: number | null; wall: number | null; area: SampleArea; enemySide: boolean; enemyBase: boolean; urn: boolean; hp: number | null; you: Point | null; nearestAlly: number | null;
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
/** Display names whose class name differs (data/heroes.json; a unit test keeps them in sync), used
 *  until the app has loaded assets/heroes.json and in scripts. Others are the capitalised id. */
export const FALLBACK_NAMES: Record<string, string> = {
  hero_inferno: 'Infernus', hero_gigawatt: 'Seven', hero_hornet: 'Vindicta', hero_ghost: 'Lady Geist', hero_atlas: 'Abrams', hero_forge: 'McGinnis',
  hero_chrono: 'Paradox', hero_astro: 'Holliday', hero_nano: 'Calico', hero_orion: 'Grey Talon', hero_krill: 'Mo & Krill', hero_tengu: 'Ivy', hero_synth: 'Pocket',
  hero_viper: 'Vyper', hero_magician: 'Sinclair', hero_vampirebat: 'Mina', hero_priest: 'Venator', hero_frank: 'Victor', hero_bookworm: 'Paige',
  hero_doorman: 'The Doorman', hero_punkgoat: 'Billy', hero_necro: 'Graves', hero_fencer: 'Apollo', hero_deadpack: 'Deadman Danny', hero_familiar: 'Rem',
  hero_werewolf: 'Silver', hero_unicorn: 'Celeste', hero_ratking: 'Rat King', hero_chessmaster: 'Solomon', hero_artist: 'Violet', hero_nurse: 'Nurse Harrow',
};

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
  const zone = map?.zone_area ?? null;
  let area: SampleArea;
  if (s.dead === true || screen === 'Dead') area = 'dead';
  else if (screen !== 'Gameplay') area = 'unknown';
  else if (you && you.y > 0.86) area = 'base';
  else if (you && map?.your_lane) area = 'lane';
  else if (you) area = 'jungle';
  // Live full scans rarely locate your marker (rec8: 10 of 478 gameplay frames, rec12: 6 of 424),
  // so every alive frame was "unknown" and time dead looked like 90% of the readable time. The
  // zone label under the minimap still says where you are ("Base" alone is ambiguous: rec12 read
  // it between their Yellow lane and their base).
  else if (zone === 'Lane' || zone === 'EnemyBase') area = 'lane';
  else if (zone === 'Jungle' || zone === 'Mid') area = 'jungle';
  else if (zone === 'OwnBase') area = 'base';
  else area = 'field';
  const enemySide = (you != null && you.y < 0.5) || zone === 'EnemyBase';
  const urn = Array.isArray(s.status_effects) && s.status_effects.some(e => typeof e === 'string' && /carrying_urn|soul\s*urn/i.test(e));
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
    // A portrait read as dead with no respawn countdown ever read is usually a misread: rec12 had
    // Infernus "dead" on most frames from 4:40 on. Readers without the counter count as before.
    const isDead = (h: (typeof team)[number]) => h.dead === true && (h.respawn_secs != null || typeof h.respawn_observations !== 'number' || h.respawn_observations > 0);
    alliesDead = team.filter(h => h.ally && isDead(h)).length;
    enemiesDead = team.filter(h => !h.ally && isDead(h)).length;
    unseen = team.filter(h => !h.ally && h.dead !== true && h.bar_visible === false).length;
  }
  const hp = finite(s.hp) && finite(s.max_hp) && s.max_hp >= 100 && s.hp <= s.max_hp && s.hp >= 0 ? s.hp / s.max_hp : null;
  const lanes = map ? [map.green, map.blue, map.yellow] : [];
  const deepWave = lanes.some(l => l && finite(l.push) && l.push < 0.3 && l.enemy_troopers >= 2);
  const push = (map ? lanes.map(l => l && finite(l.push) ? l.push : null) : [null, null, null]) as Sample['push'];
  const souls = finite(s.souls) && s.souls >= 0 ? s.souls : null;
  const idx = ['Green', 'Blue', 'Yellow'].indexOf(map?.your_lane || '');
  return { t, wall, area, enemySide, enemyBase: zone === 'EnemyBase', urn, hp, you, nearestAlly, unseen, alliesDead, enemiesDead, souls, fightAt, deepWave, push, lane: idx >= 0 ? idx as 0 | 1 | 2 : null };
}

/** A death being observed: `ends` = frame clock + respawn timer for each timer read. */
interface OpenDeath { rec: DeathRecord; startT: number; lastDeadT: number; ends: number[]; lowDied: boolean; soulsMin: number | null }
/** Aggregates one match. Keeps only a bounded window of recent samples plus running totals. */
export class MatchAggregator {
  readonly startedAt: number; source: MatchRecord['source'];
  private samples: Sample[] = [];
  private heroVotes = new Map<string, number>(); private idVotes = new Map<number, number>();
  /** First match id read (boundary checks); the record keeps the most-read one: the HUD can keep
   *  the previous match's id for minutes (rec12 read rec11's 127139499 until 4:40). */
  matchId: number | null = null;
  clock: number | null = null; lastWall: number | null = null; lastReadWall: number | null = null; private rejects = 0;
  startClock: number | null = null; private lastRead: number | null = null; result: MatchRecord['result'] = null;
  private time: Record<Area, number> = { lane: 0, jungle: 0, base: 0, dead: 0, unknown: 0 };
  private deaths: DeathRecord[] = []; private open: OpenDeath | null = null; private lastClosed: OpenDeath | null = null;
  /** Observed game seconds (and its value at the last clocked frame), recording gaps (clock). */
  private span = 0; private spanAtRead = 0; private spanAnchor: { wall: number | null; t: number | null } | null = null;
  private gaps: Array<[number, number]> = [];
  private dead = false; private lastDeadT: number | null = null; private respawnedAt: number | null = null; private prevRespawn: number | null = null;
  private fights = { joined: 0, missed: 0, whileDead: 0 }; private fight: { lastSeen: number } | null = null;
  private windows = { used: 0, unused: 0, unusedAt: [] as number[], usedAt: [] as number[], base: 0 };
  private win: { start: number; secs: number; used: boolean; base: boolean; push: Sample['push'] } | null = null;
  private field = 0;
  private urn = { carried: 0, delivered: 0, lost: 0, at: [] as Array<[number, number]> };
  private urnOpen: { start: number; last: number } | null = null; private urnPending: { start: number; last: number } | null = null;
  private team = { reads: 0, leadFrom: null as number | null, maxLead: 0, lastLead: null as number | null };
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

  get observedSecs() { return Object.values(this.time).reduce((a, b) => a + b, 0) + this.field; }
  get hasEvidence() { return this.time.lane + this.time.jungle + this.time.base + this.field + this.time.dead >= LIMITS.min_match_secs || this.deaths.length > 0; }
  get isDead() { return this.dead; }
  /** The last death of this match (closed or still open). */
  get lastDeath(): DeathRecord | null { return this.open?.rec ?? this.deaths.at(-1) ?? null; }
  get deathClocks(): number[] { return [...this.deaths.map(d => d.clock), ...(this.open ? [this.open.rec.clock] : [])]; }

  add(s: Snapshot, wall: number | null): { died: boolean; respawned: boolean } {
    const read = finite(s.game_clock_secs) && s.game_clock_secs >= 0 && s.game_clock_secs <= 14400 ? Math.round(s.game_clock_secs) : null;
    const t = this.effectiveClock(read, wall);
    if (t != null) { this.clock = t; if (this.startClock == null) this.startClock = t; if (t === read) this.lastRead = read; }
    if (wall != null) { this.lastWall = wall; if (read != null) this.lastReadWall = wall; }
    if (finite(s.match_id)) { if (this.matchId == null) this.matchId = s.match_id; this.idVotes.set(s.match_id, (this.idVotes.get(s.match_id) || 0) + 1); }
    if (finite(s.ally_networth) && finite(s.enemy_networth) && s.ally_networth > 0 && s.enemy_networth > 0 && t != null) {
      const lead = s.ally_networth - s.enemy_networth, tm = this.team; tm.reads++; tm.lastLead = lead; tm.maxLead = Math.max(tm.maxLead, lead);
      if (lead < 0) tm.leadFrom = null; else if (tm.leadFrom == null) tm.leadFrom = t;
    }
    if (s.hero && /^hero_/.test(s.hero)) this.heroVotes.set(s.hero, (this.heroVotes.get(s.hero) || 0) + 1);
    const ext = s as Snapshot & { kills?: number | null; deaths?: number | null; assists?: number | null };
    if (finite(ext.kills) || finite(ext.deaths) || finite(ext.assists)) this.kda = { kills: finite(ext.kills) ? ext.kills : this.kda?.kills ?? null, deaths: finite(ext.deaths) ? ext.deaths : this.kda?.deaths ?? null, assists: finite(ext.assists) ? ext.assists : this.kda?.assists ?? null };
    if (finite(s.own_networth) && t != null) { const p = { t, v: s.own_networth }; this.worth.first ??= p; if (s.own_networth >= (this.worth.last?.v ?? 0)) this.worth.last = p; }
    const x = reduce(s, t, wall);
    // Souls barely change between two reads: a frame without a souls read keeps the last one for
    // up to 10 s (rec4 read the souls box on about half of the frames).
    const p = this.prev;
    if (x.souls == null && p && p.souls != null && p.area !== 'dead' && x.area !== 'dead' && p.t != null && x.t != null && x.t - p.t <= 10) x.souls = p.souls;
    // One death = one event (rec13 41:05: gameplay frames without a health bar at 41:44 and 41:49,
    // 10 s before a timer that read 55 s at 41:39 ran out, split one death into two): a frame with
    // no health read while the respawn estimate is more than 5 s away is still the death screen.
    if (this.dead && this.open && x.area !== 'dead' && x.hp == null && x.t != null) {
      const est = respawnEstimate(this.open.ends, this.open.lastDeadT);
      if (est != null && x.t < est - 5) x.area = 'dead';
    }
    this.integrate(x);
    this.observeSpan(wall, t, read != null);
    if (x.t != null) this.trackUrn(x);
    const events = { died: false, respawned: false };
    const timer = finite(s.respawn_secs) && s.respawn_secs >= 0 && s.respawn_secs <= 120 ? s.respawn_secs : null;
    // Deaths (review.rs evaluate): entering the death screen, or a death screen > 60 s after the
    // previous one (rec5 merged a death across a 7-minute recording gap). Unknown frames between
    // death screens do not split or end a death.
    if (x.area === 'dead' && x.t != null) {
      // A single gameplay frame inside a death screen is a misread, not a respawn: reopen that
      // death, unless the new timer says a fresh death started.
      const last = this.lastClosed, est = last ? respawnEstimate(last.ends, last.lastDeadT) : null;
      if (!this.dead && last && this.deaths.at(-1) === last.rec && this.respawnedAt != null && x.t - this.respawnedAt <= 8 && this.respawnedAt - last.lastDeadT <= 10
        && !(timer != null && est != null && x.t + timer > est + 15)) {
        this.deaths.pop(); this.open = last; this.lastClosed = null; this.respawnedAt = this.prevRespawn; this.dead = true;
      }
      if (!this.dead || (this.lastDeadT != null && x.t > this.lastDeadT + 60)) { this.closeDeath(null); this.startDeath(x, s); events.died = true; }
      this.dead = true; this.lastDeadT = x.t;
      const o = this.open;
      if (o) {
        o.lastDeadT = x.t;
        if (timer != null) o.ends.push(x.t + timer);
        // The damage panel fills in over a few seconds: the latest reading wins.
        const dd = (s as Snapshot & { death_damage?: Array<{ source?: string; damage?: number }> | null; death_killing_blow?: string | null });
        const top = Array.isArray(dd.death_damage) ? [...dd.death_damage].filter(d => d && typeof d.source === 'string').sort((a, b) => (b.damage || 0) - (a.damage || 0))[0]?.source : undefined;
        const cause = s.death_cause || top || dd.death_killing_blow || null;
        if (cause) { o.rec.cause = String(cause).slice(0, 60); const k = killerFamily(o.rec.cause); o.rec.family = k?.family ?? null; o.rec.killer = k?.hero ?? null; o.rec.close = k?.close ?? null; }
        if (o.rec.souls == null && x.souls != null) o.rec.souls = x.souls;
        if (x.souls != null) o.soulsMin = Math.min(o.soulsMin ?? Infinity, x.souls);
      }
    } else if (s.screen === 'Gameplay' && x.area !== 'dead' && this.dead) {
      this.dead = false;
      // The first alive frame shows what the death-screen shopping left.
      if (this.open && x.souls != null) this.open.soulsMin = Math.min(this.open.soulsMin ?? Infinity, x.souls);
      // The grey "Respawn in: 1" screen is not a death: no respawn either.
      if (this.closeDeath(x.t)) { this.prevRespawn = this.respawnedAt; if (x.t != null) this.respawnedAt = x.t; events.respawned = true; }
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
    if (a.area === 'field') this.field += dt; else this.time[a.area] += dt;
    const alive = a.area === 'lane' || a.area === 'jungle' || a.area === 'base' || a.area === 'field';
    if (alive && a.souls != null) { this.souls.knownSecs += dt; if (a.souls >= Math.max(REVIEW.hoard_souls, spendTarget(a.t ?? null))) { this.souls.hoardSecs += dt; this.souls.peak = Math.max(this.souls.peak, a.souls); } }
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
      if (!this.win) this.win = { start: a.t ?? 0, secs: 0, used: false, base: false, push: [...a.push] };
      const w = this.win; w.secs += dt;
      if (a.enemySide) w.used = true;
      if (a.enemyBase) w.base = true;
      // Only the lane you are on counts: other fronts drift on their own (rec4 32:07, a front
      // moved 0.2 while you stood in base).
      a.push.forEach((p, i) => { if (w.push[i] == null && p != null) w.push[i] = p; });
      if (a.area === 'lane' && a.lane != null) { const p = a.push[a.lane], p0 = w.push[a.lane]; if (p != null && p0 != null && p - p0 >= LIMITS.window_push_gain) w.used = true; }
    } else if (a.area === 'dead' || (nDead != null && nDead < LIMITS.window_enemies)) this.closeWindow();
  }
  private closeWindow() {
    const w = this.win; this.win = null;
    if (!w || w.secs < LIMITS.window_secs) return;
    if (w.used) { this.windows.used++; this.windows.usedAt.push(Math.round(w.start)); if (w.base) this.windows.base++; }
    else { this.windows.unused++; this.windows.unusedAt.push(Math.round(w.start)); }
  }
  /** Soul Urn carries: a carry ends when the effect is gone for > 8 s while alive; it counts as
   *  delivered when no death follows within 10 s, lost when you die carrying it. */
  private trackUrn(x: Sample) {
    const t = x.t!, u = this.urn;
    if (this.urnPending && t > this.urnPending.last + LIMITS.urn_safe_secs) { u.delivered++; u.at.push([Math.round(this.urnPending.start), Math.round(this.urnPending.last)]); if (u.at.length > 10) u.at.shift(); this.urnPending = null; }
    if (x.area === 'dead') { if (this.urnOpen || this.urnPending) u.lost++; this.urnOpen = this.urnPending = null; return; }
    if (x.area === 'unknown') return;
    if (x.urn) { if (!this.urnOpen) { this.urnOpen = { start: t, last: t }; u.carried++; } else this.urnOpen.last = t; }
    else if (this.urnOpen && t - this.urnOpen.last > LIMITS.urn_gap_secs) { this.urnPending = this.urnOpen; this.urnOpen = null; }
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
      // Low health after a won fight (rec13 29:25: five enemies dead, you at 15% hitting their Walker).
      const fightWin = hist.filter(p => p.area !== 'dead' && p.t! + LIMITS.won_fight_secs >= end && p.t! <= end && p.enemiesDead != null);
      if (fightWin.length) {
        const won = fightWin.find(p => p.enemiesDead! >= LIMITS.window_enemies && p.enemiesDead! > (p.alliesDead ?? 0));
        rec.lowAfterWin = !!won && hist.some(p => p.area !== 'dead' && p.hp != null && p.hp < LIMITS.won_fight_low_hp && p.t! >= won.t! - 5 && p.t! <= clock - 4 && p.t! + LIMITS.won_fight_secs >= end);
      }
      const effects = (s as Snapshot & { status_effects?: string[] }).status_effects;
      if (Array.isArray(effects) && effects.length) rec.effects = effects.filter(e => typeof e === 'string').slice(0, 6);
    }
    if (this.respawnedAt != null && end >= this.respawnedAt) rec.afterRespawnSecs = Math.round(end - this.respawnedAt);
    if (rec.souls == null && x.souls != null) rec.souls = x.souls;
    const lowDied = this.lowOpen != null;
    if (lowDied) { this.lowHp.died++; this.lowOpen = null; }
    this.open = { rec, startT: clock, lastDeadT: clock, ends: [], lowDied, soulsMin: x.souls };
  }
  /** Close the open death; `aliveT` = clock of the alive gameplay frame that ended it (null: a
   *  new death or the end of the match). Returns false for the grey "Respawn in: 1" screen
   *  (dropped: not a death, no time dead). */
  private closeDeath(aliveT: number | null): boolean {
    const o = this.open; if (!o) return false;
    this.open = null;
    const est = respawnEstimate(o.ends, o.lastDeadT);
    if (aliveT != null && aliveT - o.startT < LIMITS.downed_secs && o.startT >= LIMITS.downed_after_clock && !(est != null && est - o.startT >= LIMITS.downed_secs)) {
      if (o.lowDied) this.lowHp.died--;
      return false;
    }
    o.rec.deadSecs = Math.round(this.deathSecs(o, aliveT));
    // Souls held just before the death minus the lowest read until the first alive frame. (Unsecured
    // souls lost at the death are counted too; they are small next to an item.)
    if (o.rec.souls != null && o.soulsMin != null && o.rec.souls - o.soulsMin > 0) o.rec.spentDead = Math.round(o.rec.souls - o.soulsMin);
    this.deaths.push(o.rec); this.lastClosed = o;
    return true;
  }
  /** Seconds dead in one death: first death screen -> the respawn-timer estimate, never before
   *  the last death screen nor after the first alive gameplay frame; without a timer read, the
   *  first alive frame (at most 10 s after the last death screen). Recording gaps are removed.
   *  Never past the last observed clock (a match that ends while you are dead). */
  private deathSecs(o: OpenDeath, aliveT: number | null): number {
    const est = respawnEstimate(o.ends, o.lastDeadT);
    let end = est != null ? (aliveT != null ? Math.min(est, aliveT) : est)
      : aliveT != null ? Math.min(aliveT, o.lastDeadT + LIMITS.sample_cap_secs) : o.lastDeadT;
    end = Math.max(end, o.lastDeadT);
    if (aliveT == null && this.clock != null) end = Math.min(end, Math.max(o.lastDeadT, this.clock));
    let secs = Math.max(0, end - o.startT);
    for (const [a, b] of this.gaps) secs -= Math.max(0, Math.min(b, end) - Math.max(a, o.startT));
    return Math.max(0, secs);
  }
  /** Observed game time: every frame-to-frame interval up to 60 s (wall time live, game clock in
   *  replays), counted up to the last frame with a clock read; longer ones are recording gaps. */
  private observeSpan(wall: number | null, t: number | null, clocked: boolean) {
    const a = this.spanAnchor;
    if (a) {
      const raw = wall != null && a.wall != null ? (wall - a.wall) / 1000 : t != null && a.t != null ? t - a.t : null;
      if (raw != null && raw > 0) {
        if (raw <= LIMITS.recording_gap_secs) this.span += raw;
        else if (a.t != null && t != null) { this.gaps.push([a.t, t]); if (this.gaps.length > 50) this.gaps.shift(); }
      }
    }
    if (wall != null || t != null) this.spanAnchor = { wall, t: t ?? a?.t ?? null };
    if (clocked) this.spanAtRead = this.span;
  }
  /** Seconds dead so far (an open death counts up to its respawn estimate). */
  get deadSecs() { return this.deaths.reduce((a, d) => a + (d.deadSecs ?? 0), 0) + (this.open ? this.deathSecs(this.open, null) : 0); }
  get spanSecs() { return this.spanAtRead; }

  /** The match so far (does not close it). */
  record(endedAt: number, partial = false): MatchRecord {
    const hero = [...this.heroVotes.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
    const deaths = [...this.deaths, ...(this.open ? [{ ...this.open.rec, deadSecs: Math.round(this.deathSecs(this.open, null)) }] : [])].map(d => ({ ...d }));
    const windows = { ...this.windows, unusedAt: [...this.windows.unusedAt], usedAt: [...this.windows.usedAt] };
    if (this.win && this.win.secs >= LIMITS.window_secs) {
      if (this.win.used) { windows.used++; windows.usedAt.push(Math.round(this.win.start)); if (this.win.base) windows.base++; }
      else { windows.unused++; windows.unusedAt.push(Math.round(this.win.start)); }
    }
    const matchId = [...this.idVotes.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? this.matchId;
    const tm = this.team;
    const w = this.worth; const span = w.first && w.last ? w.last.t - w.first.t : 0;
    const round = (r: Record<string, number>) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, Math.round(v)]));
    return { v: 1, id: `${this.startedAt.toString(36)}-${(this.startClock ?? 0).toString(36)}`, source: this.source, startedAt: this.startedAt, endedAt,
      hero, matchId, startClock: this.startClock, endClock: this.lastRead ?? (this.clock == null ? null : Math.round(this.clock)), observedSecs: Math.round(this.observedSecs), result: this.result,
      ...(partial ? { partial: true } : {}),
      time: round(this.time) as Record<Area, number>, spanSecs: Math.round(this.spanSecs), deadSecs: Math.round(Math.min(this.deadSecs, Math.max(this.spanSecs, 0))),
      fieldSecs: Math.round(this.field),
      deaths, fights: { ...this.fights }, windows,
      urn: { ...this.urn, at: this.urn.at.map(a => [...a] as [number, number]) },
      ...(tm.reads ? { team: { reads: tm.reads, leadFrom: tm.leadFrom, maxLead: Math.round(tm.maxLead), lastLead: tm.lastLead } } : {}),
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
  /** Where a strength comes from: the profile (several matches), the last match, or an
   *  improvement of the last match over the ones before it. Weaknesses are always 'profile'. */
  scope?: 'profile' | 'last' | 'improved';
}
export interface MatchRow {
  id: string; date: number; hero: string | null; heroName: string; source: MatchRecord['source']; result: MatchRecord['result'];
  minutes: number; deaths: number; deadPct: number | null; fightsJoined: number; fightsMissed: number;
  unspentAtDeath: string; topKiller: string | null; partial: boolean;
  /** The match focus goals and whether they were met (absent before goals existed). */
  goals: GoalResult[];
  /** Why the match is left out of the traits (null = it counts); `legacy` = stored before the
   *  time-dead metric (its time dead is not used). */
  excluded: string | null; legacy: boolean;
}
export interface PlayerSummary {
  matches: number; hero: string | null; heroName: string | null; heroes: Array<{ hero: string; name: string; matches: number }>;
  observedMinutes: number; deathsPer10: number | null; deadPct: number | null; farmPerMin: number | null;
  /** Strengths: the last match's highlights first (scope 'last'), then the profile's. */
  strengths: Trait[]; weaknesses: Trait[]; rows: MatchRow[];
  /** "Improved since last time": the last counted match against the ones before it. */
  improvements: Trait[];
  /** The last counted match clearly improved (time dead or deaths, or two other metrics). */
  improved: boolean;
  /** Matches (of the shown ones) left out of the traits. */
  excluded: number;
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
const pct = (n: number, d: number) => d > 0 ? Math.round(n * 100 / d) : 0;
/** Alive time with a location (lane / jungle / base shares). */
const located = (m: MatchRecord) => m.time.lane + m.time.jungle + m.time.base;
/** Alive gameplay time, located or not. */
export const aliveOf = (m: MatchRecord) => located(m) + (m.fieldSecs ?? 0);
const known = (m: MatchRecord) => aliveOf(m) + m.time.dead;
/** The dead-share metric: seconds dead / observed game seconds (docs/player-profile.md). Matches
 *  stored before it fall back to their sampled dead time over all sampled time (unknown included);
 *  their time dead is shown nowhere and never used by a trait (`legacyMetric`). */
export const spanOf = (m: MatchRecord) => m.spanSecs ?? m.observedSecs;
export const deadOf = (m: MatchRecord) => m.deadSecs ?? m.time.dead;
export const legacyMetric = (m: MatchRecord) => m.spanSecs == null || m.deadSecs == null;
/** Why a match is left out of every trait, or null when it counts (docs "Which matches count"):
 *  under 8 minutes observed (a partial match with one death is not a habit), no readable
 *  gameplay (menus, spectating, a death screen), or more than 50% dead (a misread: real matches
 *  are 7-24%; the old "readable time" metric said 65-90% on live scans). */
export function excludedReason(m: MatchRecord): string | null {
  if (spanOf(m) < LIMITS.trait_min_secs) return `under ${LIMITS.trait_min_secs / 60} min observed`;
  if (aliveOf(m) < LIMITS.trait_min_alive_secs) return 'no readable gameplay';
  if (spanOf(m) > 0 && deadOf(m) / spanOf(m) > LIMITS.max_dead_share) return `implausible ${pct(deadOf(m), spanOf(m))}% dead`;
  return null;
}
/** Alive stretches without a death: [from, to] clocks (match start / respawn -> next death / end). */
export function deathlessStretches(m: MatchRecord): Array<[number, number]> {
  if (m.startClock == null || m.endClock == null) return [];
  const out: Array<[number, number]> = []; let from = m.startClock;
  for (const d of [...m.deaths].sort((a, b) => a.clock - b.clock)) {
    if (d.clock > from) out.push([from, d.clock]);
    from = Math.max(from, d.clock + (d.deadSecs ?? 0));
  }
  if (m.endClock > from) out.push([from, m.endClock]);
  return out;
}
export const clockText = (c: number) => { const s = Math.max(0, Math.round(c)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };
const minutes = (secs: number) => `${Math.round(secs / 60)} min`;
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
  heroName(id: string | null | undefined) {
    return id ? this.names[id] || FALLBACK_NAMES[id] || id.replace(/^hero_/, '').split('_').map(w => w ? w[0].toUpperCase() + w.slice(1) : w).join(' ') : 'Unknown hero';
  }

  /** Traits for matches (newest first). */
  traits(all: MatchRecord[]): { strengths: Trait[]; weaknesses: Trait[] } {
    // Only matches that count (excludedReason); each ratio sums numerators and denominators, so a
    // long match weighs more than a short one (observed-time weighting), times the recency weight.
    const ms = all.filter(m => !excludedReason(m)).slice(0, 10);
    const out: Trait[] = [];
    if (!ms.length) return { strengths: [], weaknesses: [] };
    const hero = mode(ms.map(m => m.hero));
    const heroName = this.heroName(hero);
    const longRange = hero != null && LONG_RANGE.has(hero), frontline = hero != null && FRONTLINE.has(hero);
    const deathsAll = ms.flatMap(m => m.deaths);
    const push = (t: Omit<Trait, 'confidence' | 'confidenceLabel'> & { events: number; needed: number }) => {
      const { events, needed, ...rest } = t; out.push({ ...rest, ...confidence(events, needed, t.matches) });
    };

    // 1. Dying: share of observed game time spent dead (review.json pattern_deaths: >= 60 s dead in
    //    a 5-minute window; ground truth from the respawn timers: rec4 24%, rec5 19%, rec8 / rec11
    //    17%, all judged too much). Weak >= 15%, strong < 7%.
    //    Matches stored before the metric (legacyMetric) never count here.
    const deadF = (m: MatchRecord): Ratio | null => !legacyMetric(m) ? { num: deadOf(m), den: spanOf(m) } : null;
    const dead = ratio(ms, deadF);
    const per10 = ratio(ms, m => !legacyMetric(m) ? { num: m.deaths.length, den: spanOf(m) / 600 } : null);
    if (dead && per10) {
      // Thresholds use the recency-weighted value; the words use the plain time-weighted share of
      // the games named ("dead 18% of your last 3 games").
      const v = dead.value * 100, d10 = per10.value;
      const said = Math.round(dead.raw.num * 100 / dead.raw.den), said10 = per10.raw.num / per10.raw.den;
      const games = dead.n === 1 ? 'your last game' : `your last ${dead.n} games`;
      const evidence = `${said}% of the game dead (${minutes(dead.raw.num)} of ${minutes(dead.raw.den)} observed); ${said10.toFixed(1)} deaths per 10 min (${per10.raw.num} deaths, ${dead.n} match${dead.n === 1 ? '' : 'es'}).`;
      const how = longRange ? 'Stay at max range behind your front line.' : 'Fight from behind your front line.';
      if (v >= 15 || d10 >= 2) push({ id: 'dying', kind: 'weakness', label: 'Dying too often', evidence, severity: severity(Math.max(v / 15, d10 / 2), 1), value: Math.round(v), threshold: 15, matches: dead.n,
        trend: trend(ms, deadF, true), events: per10.raw.num, needed: 6,
        tip: `Play for zero deaths: fight from behind your front line and leave on the first big hit.${longRange ? ` ${heroName} is long-range: stay at max range and let the front line take space.` : ''}`,
        focus: v >= 15 ? `Profile focus: you were dead ${said}% of ${games}; aim under ${LIMITS.dead_target_pct}%. ${how}`
          : `Profile focus: you died ${said10.toFixed(1)} times per 10 minutes in ${games}; aim under 1.5. ${how}` });
      else if (v < 7 && d10 < 1 && per10.raw.den >= 1.5) push({ id: 'survival', kind: 'strength', label: 'Stays alive', evidence, severity: 1, value: Math.round(v), threshold: 7, matches: dead.n,
        trend: trend(ms, deadF, true), events: per10.raw.den, needed: 3, tip: 'Keep it up: being alive is what lets you take the next objective.',
        focus: `Keep it up: you were dead only ${said}% of ${games}.` });
    }

    // 2. Unspent souls (common-mistakes.md #6; review.json souls_at_death 1200, hoard_souls 3000,
    //    pattern_hoard = >= 60 s of a 5-minute window, i.e. 20%).
    const atDeathF = (m: MatchRecord): Ratio | null => { const d = m.deaths.filter(x => x.souls != null); return d.length ? { num: d.filter(x => x.souls! >= deathSpend(x.clock)).length, den: d.length } : null; };
    const hoardF = (m: MatchRecord): Ratio | null => m.souls.knownSecs >= 120 ? { num: m.souls.hoardSecs, den: m.souls.knownSecs } : null;
    const atDeath = ratio(ms, atDeathF), hoard = ratio(ms, hoardF);
    const withSouls = deathsAll.filter(d => d.souls != null);
    const soulsAt = withSouls.map(d => d.souls!);
    // The threshold depends on the game clock (half the usual tier, at least 1,200): name its range.
    const needs = withSouls.map(d => deathSpend(d.clock));
    const need = needs.length ? (Math.min(...needs) === Math.max(...needs) ? `${k(needs[0])}+ souls` : `half an item tier or more (${k(Math.min(...needs))}-${k(Math.max(...needs))} by game time)`) : `${k(REVIEW.souls_at_death)}+ souls`;
    const peak = Math.max(0, ...ms.map(m => m.souls.peak));
    const weakAt = atDeath && atDeath.raw.den >= 2 && atDeath.value >= 0.5, weakHoard = hoard && hoard.value >= 0.2;
    if (weakAt || weakHoard) {
      const parts: string[] = [];
      if (atDeath) parts.push(`died holding ${need} in ${atDeath.raw.num} of ${atDeath.raw.den} deaths${soulsAt.length ? ` (held ${k(Math.min(...soulsAt))}-${k(Math.max(...soulsAt))})` : ''}`);
      if (hoard) parts.push(`over ${k(REVIEW.hoard_souls)} unspent for ${pct(hoard.num, hoard.den)}% of alive time${peak ? `, peak ${k(peak)}` : ''}`);
      const v = Math.max(atDeath ? atDeath.value / 0.5 : 0, hoard ? hoard.value / 0.2 : 0);
      push({ id: 'unspent_souls', kind: 'weakness', label: 'Unspent souls', evidence: cap(parts.join('; ')) + '.', severity: severity(v, 1), value: Math.round((atDeath?.value ?? hoard!.value) * 100), threshold: 50,
        matches: Math.max(atDeath?.n ?? 0, hoard?.n ?? 0), trend: agree(atDeath ? trend(ms, atDeathF, true) : null, hoard ? trend(ms, hoardF, true) : null), events: (atDeath?.raw.den ?? 0) + (hoard ? hoard.raw.den / 300 : 0), needed: 6,
        tip: 'Spend at 3,000 souls: shop when your wave is safe, and never walk back into a fight with souls in your pocket.', focus: 'Profile focus: spend your souls at 3,000 before you go back in.' });
    } else if (atDeath && hoard && atDeath.raw.den >= 3 && atDeath.value <= 0.15 && hoard.value <= 0.05) {
      push({ id: 'spending', kind: 'strength', label: 'Spends souls on time', evidence: `Only ${atDeath.raw.num} of ${atDeath.raw.den} deaths holding ${need}; over ${k(REVIEW.hoard_souls)} for ${pct(hoard.num, hoard.den)}% of alive time.`,
        severity: 1, value: Math.round(atDeath.value * 100), threshold: 15, matches: atDeath.n, trend: trend(ms, atDeathF, true), events: atDeath.raw.den, needed: 5, tip: 'Keep shopping on time.', focus: 'Keep it up: you spend your souls before you go back in.' });
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
        matches: win.n, trend: trend(ms, winF, true), events: win.raw.den, needed: 4, tip: 'Keep converting kills into objectives.', focus: 'Keep it up: you turn enemy deaths into pushes and objectives.' });
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
        matches: fights.n, trend: trend(ms, fightF, true), events: fights.raw.den, needed: 10, tip: 'Good team presence. Keep catching waves between fights.', focus: 'Keep it up: you show up for team fights.' });
    }

    // 7. Jungle vs lanes (review.json pattern_jungle: >= 50% of alive time; jungle-tactics.md:
    //    camps between waves). A low jungle share is not a weakness.
    const jungleF = (m: MatchRecord): Ratio | null => located(m) >= 300 ? { num: m.time.jungle, den: located(m) } : null;
    const laneF = (m: MatchRecord): Ratio | null => located(m) >= 300 ? { num: m.time.lane, den: located(m) } : null;
    const jungle = ratio(ms, jungleF), lane = ratio(ms, laneF);
    if (jungle && lane && (jungle.value >= 0.5 || (jungle.value > lane.value && jungle.value >= 0.35))) {
      push({ id: 'jungle_heavy', kind: 'weakness', label: 'Too much jungle', evidence: `${pct(jungle.num, jungle.den)}% of alive time in the jungle vs ${pct(lane.num, lane.den)}% in lanes.`, severity: severity(jungle.value, 0.5),
        value: Math.round(jungle.value * 100), threshold: 50, matches: jungle.n, trend: trend(ms, jungleF, true), events: jungle.raw.den / 300, needed: 4,
        tip: 'Take camps between waves, then go back to a lane or your team.', focus: 'Profile focus: take camps between waves, then go back to a lane.' });
    } else if (lane && jungle && lane.value >= 0.6 && lane.raw.den >= 600) {
      push({ id: 'lane_presence', kind: 'strength', label: 'Keeps lanes covered', evidence: `${pct(lane.num, lane.den)}% of alive time on a lane, ${pct(jungle.num, jungle.den)}% in the jungle.`, severity: 1, value: Math.round(lane.value * 100), threshold: 60,
        matches: lane.n, trend: trend(ms, laneF, false), events: lane.raw.den / 300, needed: 4, tip: 'Good wave presence. Take a camp only between waves.', focus: 'Keep it up: you keep your lanes covered.' });
    }

    // 8. Waves left deep while in the jungle (review.json pattern_waves: >= 30 s per 5 minutes).
    const deepF = (m: MatchRecord): Ratio | null => located(m) >= 300 ? { num: m.deepAwaySecs, den: located(m) / 600 } : null;
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
      severity: 1, value: Math.round((1 - low.value) * 100), threshold: 75, matches: low.n, trend: trend(ms, lowF, true), events: low.raw.den, needed: 6, tip: 'Good discipline at low health.', focus: 'Keep it up: you back off when your health is low.' });

    // Rank: severity x confidence x specificity (a generic symptom such as "most deaths were
    // fast" says less about what to change than time dead or unspent souls).
    const score = (t: Trait) => t.severity * t.confidence * (SPECIFICITY[t.id] ?? 1);
    const weaknesses = out.filter(t => t.kind === 'weakness').sort((a, b) => score(b) - score(a) || PRIORITY.indexOf(a.id) - PRIORITY.indexOf(b.id)).map(t => ({ ...t, scope: 'profile' as const }));
    const strengths = out.filter(t => t.kind === 'strength').sort((a, b) => b.confidence - a.confidence).map(t => ({ ...t, scope: 'profile' as const }));
    return { strengths, weaknesses };
  }

  /** What went well in one match (the newest counted one), with evidence: praise is as specific
   *  as the fixes ("used 2 of 2 enemy-dead windows to push their base"). */
  highlights(m: MatchRecord): Trait[] {
    const out: Trait[] = [];
    const add = (id: string, label: string, evidence: string, focus: string, tip: string, value: number, threshold: number, events: number, needed: number) =>
      out.push({ id, kind: 'strength', label, evidence, focus, tip, value, threshold, severity: 1, trend: 'new', matches: 1, scope: 'last', ...confidence(events, needed, 3) });
    const span = spanOf(m);
    // Little time dead (< 10% of at least 15 minutes).
    if (!legacyMetric(m) && span >= 900 && deadOf(m) / span < 0.1) {
      const p = Math.round(deadOf(m) * 100 / span);
      add('last_low_dead', 'Stayed alive', `Dead ${p}% of the game (${minutes(deadOf(m))} of ${minutes(span)} observed), ${m.deaths.length} death${m.deaths.length === 1 ? '' : 's'}.`,
        `Keep it up: last game you were dead only ${p}% of the time.`, 'Being alive kept you farming and in every push.', p, 10, span / 300, 3);
    }
    // Long deathless stretches (>= 6 minutes).
    const long = deathlessStretches(m).filter(([a, b]) => b - a >= LIMITS.deathless_secs).sort((x, y) => (y[1] - y[0]) - (x[1] - x[0]));
    if (long.length) {
      const best = long[0][1] - long[0][0];
      add('last_deathless', 'Long deathless stretches', `No deaths for ${long.slice(0, 2).map(([a, b]) => `${clockText(b - a)} (${clockText(a)}-${clockText(b)})`).join(' and ')}.`,
        `Keep it up: last game you went ${Math.floor(best / 60)} minutes without dying.`, 'Long stretches alive are where souls and objectives come from.', Math.round(best), LIMITS.deathless_secs, best / 120, 4);
    }
    // Enemy-dead windows used.
    const w = m.windows, total = w.used + w.unused;
    const base = w.base ?? 0;
    if (w.used >= 1 && (w.used >= w.unused || base >= 1)) {
      const at = (w.usedAt ?? []).slice(0, 3).map(clockText).join(', ');
      const what = w.used === total ? `${w.used} of ${total} enemy-dead window${total === 1 ? '' : 's'}` : `${w.used} enemy-dead window${w.used === 1 ? '' : 's'}`;
      // Mostly used = the habit (it cancels the "unused windows" reminder); a few base pushes among
      // many unused windows are still praised, but as plays, not as the habit.
      const mostly = w.used >= w.unused;
      add(mostly ? 'last_windows' : 'last_base_push', mostly ? 'Used enemy-dead windows' : 'Pushed their base with enemies dead', `Used ${w.used} of ${total} windows with ${LIMITS.window_enemies}+ enemies dead${at ? ` (${at})` : ''}${base ? `; ${base === w.used ? (base === 1 ? 'it' : 'all of them') : `${base} of them`} pushed into their base` : ''}.`,
        `Keep it up: last game you used ${what} to push${base === w.used ? ' their base' : base ? `, ${base === 1 ? 'once' : base === 2 ? 'twice' : `${base} times`} into their base` : ''}.`, 'Keep converting kills into structures before they respawn.', w.used, 1, w.used * 2, 2);
    }
    // Soul Urn delivered.
    if (m.urn && m.urn.delivered >= 1) {
      const at = m.urn.at.slice(0, 2).map(([a, b]) => `${clockText(a)}-${clockText(b)}`).join(', ');
      add('last_urn', 'Delivered the Soul Urn', `Carried the Soul Urn${at ? ` (${at})` : ''} and kept it alive: ${m.urn.delivered} of ${m.urn.carried} carr${m.urn.carried === 1 ? 'y' : 'ies'} delivered.`,
        'Keep it up: last game you delivered the Soul Urn without dying.', 'Urn runs are free souls for the team when the map is safe.', m.urn.delivered, 1, 2, 2);
    }
    // Shopping on the death screen.
    const spent = m.deaths.filter(d => (d.spentDead ?? 0) >= LIMITS.death_spend_min);
    if (spent.length) {
      const top = [...spent].sort((a, b) => b.spentDead! - a.spentDead!)[0];
      add('last_death_spend', 'Shops on the death screen', `Spent ${k(top.spentDead!)} souls between the ${clockText(top.clock)} death and the respawn${spent.length > 1 ? ` (${spent.length} of ${m.deaths.length} deaths)` : ''}.`,
        `Keep it up: last game you spent ${k(top.spentDead!)} souls on the death screen and came back stronger.`, 'Spending while dead turns the wait into power.', spent.length, 1, 2, 2);
    }
    // Lanes covered (>= 60% of located alive time, >= 10 minutes located).
    if (located(m) >= 600 && m.time.lane / located(m) >= 0.6) {
      const p = pct(m.time.lane, located(m));
      add('last_lanes', 'Kept your lane covered', `${p}% of located alive time on a lane, ${pct(m.time.jungle, located(m))}% in the jungle.`,
        `Keep it up: last game you spent ${p}% of your time on a lane.`, 'Wave presence is steady income and map control.', p, 60, located(m) / 300, 4);
    }
    // No death with a pile of unspent souls (>= 2 deaths with souls read).
    const ws = m.deaths.filter(d => d.souls != null);
    if (ws.length >= 2 && ws.every(d => d.souls! < deathSpend(d.clock)))
      add('last_spent', 'Spent before dying', `None of ${ws.length} deaths with half an item tier or more unspent.`, 'Keep it up: last game you never died with a pile of unspent souls.', 'Keep shopping before you go back in.', 0, 0, ws.length, 3);
    // The team's observed net-worth lead (a team result, shown as context for the praise).
    const tm = m.team;
    if (tm && tm.reads >= 20 && tm.leadFrom != null && tm.maxLead >= 10_000 && (tm.lastLead ?? 0) > 0)
      add('last_team_lead', 'Team lead held', `Your team led the observed net worth from ${clockText(tm.leadFrom)} to the end, up to +${k(tm.maxLead)}.`,
        `Keep it up: last game your team held a lead from ${clockText(tm.leadFrom)}, up to ${k(tm.maxLead)} souls.`, 'A lead is kept by staying alive and taking objectives, as you did.', Math.round(tm.maxLead / 1000), 10, tm.reads / 10, 4);
    // Concrete plays first (they say what to repeat), outcomes after.
    return out.sort((a, b) => HIGHLIGHT_ORDER.indexOf(a.id) - HIGHLIGHT_ORDER.indexOf(b.id));
  }

  /** "Improved since last time": the newest counted match against the (up to 5) counted ones
   *  before it. Each needs a clear margin, so noise is not praised. */
  improvements(ms: MatchRecord[]): Trait[] {
    const [m, ...rest] = ms; const prev = rest.slice(0, 5);
    if (!m || !prev.length) return [];
    const out: Trait[] = [];
    const n = prev.length, games = n === 1 ? 'your previous game' : `your previous ${n} games`;
    const add = (id: string, label: string, evidence: string, focus: string, value: number, threshold: number) =>
      out.push({ id, kind: 'strength', label, evidence, focus, tip: 'Keep doing what changed.', value, threshold, severity: 1, trend: 'improving', matches: 1 + n, scope: 'improved', ...confidence(1 + n, 3, 1 + n) });
    const share = (xs: MatchRecord[], f: (x: MatchRecord) => Ratio | null) => { let a = 0, b = 0; for (const x of xs) { const r = f(x); if (r && r.den > 0) { a += r.num; b += r.den; } } return b > 0 ? a / b : null; };
    const deadF = (x: MatchRecord): Ratio | null => legacyMetric(x) ? null : { num: deadOf(x), den: spanOf(x) };
    const d0 = share([m], deadF), d1 = share(prev, deadF);
    if (d0 != null && d1 != null && d1 - d0 >= 0.05 && d0 <= d1 * 0.75)
      add('improved_dead', 'Less time dead', `Dead ${Math.round(d0 * 100)}% of the game last match vs ${Math.round(d1 * 100)}% in ${games}.`,
        `Great game: you were dead ${Math.round(d0 * 100)}% of it, down from ${Math.round(d1 * 100)}%. Keep it up.`, Math.round(d0 * 100), Math.round(d1 * 100));
    const perF = (x: MatchRecord): Ratio | null => legacyMetric(x) ? null : { num: x.deaths.length, den: spanOf(x) / 600 };
    const p0 = share([m], perF), p1 = share(prev, perF);
    if (p0 != null && p1 != null && p1 - p0 >= 0.5 && p0 <= p1 * 0.7)
      add('improved_deaths', 'Fewer deaths', `${p0.toFixed(1)} deaths per 10 min last match vs ${p1.toFixed(1)} in ${games}.`,
        `Great game: ${m.deaths.length} death${m.deaths.length === 1 ? '' : 's'}, ${p0.toFixed(1)} per 10 minutes, down from ${p1.toFixed(1)}. Keep it up.`, Math.round(p0 * 10) / 10, Math.round(p1 * 10) / 10);
    const winF = (x: MatchRecord): Ratio | null => x.windows.used + x.windows.unused ? { num: x.windows.used, den: x.windows.used + x.windows.unused } : null;
    const w0 = share([m], winF), w1 = share(prev, winF);
    if (w0 != null && w0 - (w1 ?? 0) >= 0.3 && m.windows.used >= 1)
      add('improved_windows', 'Uses more enemy-dead windows', `Used ${m.windows.used} of ${m.windows.used + m.windows.unused} enemy-dead windows last match vs ${w1 == null ? 'none seen' : `${Math.round(w1 * 100)}%`} in ${games}.`,
        `Keep it up: you used ${m.windows.used} of ${m.windows.used + m.windows.unused} enemy-dead windows, more than before.`, Math.round(w0 * 100), Math.round((w1 ?? 0) * 100));
    const soulsF = (x: MatchRecord): Ratio | null => { const d = x.deaths.filter(y => y.souls != null); return d.length ? { num: d.filter(y => y.souls! >= deathSpend(y.clock)).length, den: d.length } : null; };
    const s0 = share([m], soulsF), s1 = share(prev, soulsF);
    if (s0 != null && s1 != null && m.deaths.filter(d => d.souls != null).length >= 2 && s1 - s0 >= 0.25)
      add('improved_souls', 'Fewer deaths with unspent souls', `${Math.round(s0 * 100)}% of deaths with half an item tier or more unspent last match vs ${Math.round(s1 * 100)}% in ${games}.`,
        'Keep it up: you died with fewer souls in your pocket than before.', Math.round(s0 * 100), Math.round(s1 * 100));
    const best = (x: MatchRecord) => Math.max(0, ...deathlessStretches(x).map(([a, b]) => b - a));
    const b0 = best(m), b1 = sum(prev.map(best)) / n;
    if (b0 >= LIMITS.deathless_secs && b0 >= 1.5 * b1 && b0 - b1 >= 180)
      add('improved_streak', 'Longer deathless stretches', `Longest stretch without a death ${clockText(b0)} last match vs ${clockText(b1)} on average in ${games}.`,
        `Keep it up: you went ${Math.floor(b0 / 60)} minutes without dying, longer than before.`, Math.round(b0), Math.round(b1));
    return out;
  }

  summary(all: MatchRecord[], hero?: string | null): PlayerSummary {
    const ms = hero ? all.filter(m => m.hero === hero) : all;
    // Only matches that count, as in traits() (excludedReason); time dead only from the metric's matches.
    const recent = ms.filter(m => !excludedReason(m)).slice(0, 10);
    const timed = recent.filter(m => !legacyMetric(m));
    const heroes = [...all.reduce((m, r) => r.hero ? m.set(r.hero, (m.get(r.hero) || 0) + 1) : m, new Map<string, number>())].sort((a, b) => b[1] - a[1]).map(([h, n]) => ({ hero: h, name: this.heroName(h), matches: n }));
    const kn = sum(timed.map(spanOf)), deaths = sum(timed.map(m => m.deaths.length));
    const farm = recent.map(m => m.farm.perMin).filter(finite);
    const { strengths, weaknesses } = this.traits(recent);
    const highlights = recent[0] ? this.highlights(recent[0]) : [];
    const improvements = this.improvements(recent);
    // A profile strength that the last match already shows more concretely is not repeated.
    const SAME: Record<string, string> = { survival: 'last_low_dead', used_windows: 'last_windows', lane_presence: 'last_lanes', spending: 'last_spent' };
    const profile = strengths.filter(t => !highlights.some(h => h.id === SAME[t.id]));
    const improved = improvements.some(t => t.id === 'improved_dead' || t.id === 'improved_deaths') || improvements.length >= 2;
    const main = hero ?? mode((recent.length ? recent : ms.slice(0, 10)).map(m => m.hero));
    return { matches: ms.length, hero: main, heroName: main ? this.heroName(main) : null, heroes, observedMinutes: Math.round(sum(recent.map(spanOf)) / 60),
      deathsPer10: kn >= 300 ? Math.round(deaths * 6000 / kn) / 10 : null, deadPct: kn >= 300 ? pct(sum(timed.map(deadOf)), kn) : null,
      farmPerMin: farm.length ? Math.round(sum(farm) / farm.length) : null, strengths: [...highlights, ...profile], weaknesses, improvements, improved,
      excluded: ms.filter(m => excludedReason(m)).length, rows: ms.map(m => this.row(m)) };
  }

  row(m: MatchRecord): MatchRow {
    const withSouls = m.deaths.filter(d => d.souls != null);
    const fam = mode(m.deaths.map(d => d.family));
    const killer = fam ? (() => { const h = m.deaths.find(d => d.family === fam)?.killer; const n = m.deaths.filter(d => d.family === fam).length; return `${h ? this.heroName(h) : fam} x${n}`; })() : null;
    const legacy = legacyMetric(m);
    return { id: m.id, date: m.startedAt, hero: m.hero, heroName: this.heroName(m.hero), source: m.source, result: m.result, minutes: Math.round(spanOf(m) / 6) / 10,
      deaths: m.deaths.length, deadPct: !legacy && spanOf(m) >= 60 ? pct(deadOf(m), spanOf(m)) : null, fightsJoined: m.fights.joined, fightsMissed: m.fights.missed,
      unspentAtDeath: withSouls.length ? `${withSouls.filter(d => d.souls! >= deathSpend(d.clock)).length}/${withSouls.length} · avg ${k(sum(withSouls.map(d => d.souls!)) / withSouls.length)}` : '—',
      topKiller: killer, partial: m.partial === true, goals: m.goals ?? [], excluded: excludedReason(m) ?? (legacy ? 'time dead from an older version: not used' : null), legacy };
  }

  /** Compact text for an AI prompt (<= 900 characters, ai-coach PROFILE_MAX_CHARS): weaknesses
   *  (kept first: ai-coach.ts reads the labels between "Weak:" and "Strong:"), strengths with
   *  evidence, improvements since the previous matches, and the practice focus. */
  promptText(all: MatchRecord[], hero?: string | null): string | null {
    const s = this.summary(all, hero);
    if (!s.matches) return null;
    const head = `Player profile (${s.matches} match${s.matches === 1 ? '' : 'es'}${s.heroName ? `, mostly ${s.heroName}` : ''}${s.deadPct != null ? `, ${s.deadPct}% dead, ${s.deathsPer10}/10min deaths` : ''}).`;
    const clip = (x: string, n: number) => x.length > n ? x.slice(0, n - 3) + '...' : x;
    const weak = s.weaknesses.slice(0, 3).map(t => clip(`${t.label}: ${t.evidence.replace(/\.$/, '')}${t.trend === 'improving' || t.trend === 'worse' ? ` [${t.trend}]` : ''}`, 120));
    const strong = s.strengths.slice(0, 4).map(t => clip(`${t.scope === 'last' ? 'latest match: ' : ''}${t.evidence.replace(/\.$/, '')}`, 100));
    const better = s.improvements.slice(0, 2).map(t => clip(t.evidence.replace(/\.$/, ''), 100));
    const focus = s.weaknesses[0] ? clip(s.weaknesses[0].tip, 130) : null;
    const praise = strong.length || better.length ? 'Praise what went well as specifically as the fixes.' : null;
    // Pick by priority within 900 characters (one of each group first), render grouped.
    const groups: Record<'weak' | 'strong' | 'better', string[]> = { weak: [], strong: [], better: [] };
    let size = head.length + 30 + (focus ? focus.length + 8 : 0) + (praise ? praise.length + 1 : 0);
    const order: Array<['weak' | 'strong' | 'better', string | undefined]> = [['weak', weak[0]], ['strong', strong[0]], ['better', better[0]], ['weak', weak[1]], ['strong', strong[1]],
      ['better', better[1]], ['weak', weak[2]], ['strong', strong[2]], ['strong', strong[3]]];
    for (const [g, x] of order) if (x && size + x.length + 12 <= 900) { groups[g].push(x); size += x.length + 2; }
    const end = (x: string) => /[.!?]$/.test(x) ? x : x + '.';
    const parts = [head, end(`Weak: ${groups.weak.length ? groups.weak.join('; ') : 'none clear'}`)];
    if (groups.strong.length) parts.push(end(`Strong: ${groups.strong.join('; ')}`));
    if (groups.better.length) parts.push(end(`Improved: ${groups.better.join('; ')}`));
    if (focus) parts.push(end(`Focus: ${focus}`));
    if (praise) parts.push(praise);
    return parts.join(' ').slice(0, 900);
  }

  /** Spoken lines: praise (last match, improvements, profile strengths) and fixes (weaknesses the
   *  last match did not already contradict). `leadWithPraise` after a clearly improved match. */
  focusLines(all: MatchRecord[], hero?: string | null): { praise: FocusLine[]; fixes: FocusLine[]; leadWithPraise: boolean } {
    const s = this.summary(all, hero);
    const ORDER = ['last_windows', 'last_urn', 'last_death_spend', 'last_low_dead', 'last_deathless', 'last_spent', 'last_team_lead', 'last_lanes'];
    const last = s.strengths.filter(t => t.scope === 'last').sort((a, b) => ORDER.indexOf(a.id) - ORDER.indexOf(b.id));
    const profile = s.strengths.filter(t => t.scope !== 'last' && t.confidence >= 0.3);
    const dropLow = s.improvements.some(t => t.id === 'improved_dead');
    // After a clearly improved match the biggest improvement leads, then the plays behind it.
    const lead = s.improved ? s.improvements.slice(0, 1) : [];
    const praise = [...lead, ...last.filter(t => !(dropLow && t.id === 'last_low_dead')), ...s.improvements.filter(t => !lead.includes(t)), ...profile]
      .filter(t => t.focus).map(t => ({ id: t.id, text: t.focus, kind: 'praise' as const }));
    const ids = new Set([...s.strengths, ...s.improvements].map(t => t.id));
    const CONTRADICTS: Record<string, string[]> = { dying: ['last_low_dead', 'improved_dead', 'improved_deaths'], unused_windows: ['last_windows', 'improved_windows'], unspent_souls: ['last_spent', 'improved_souls'] };
    const fixes = s.weaknesses.filter(t => t.confidence >= 0.3 && !(CONTRADICTS[t.id] || []).some(id => ids.has(id))).map(t => ({ id: t.id, text: t.focus, kind: 'fix' as const }));
    return { praise: dedupe(praise), fixes, leadWithPraise: s.improved && praise.length > 0 };
  }
}
export interface FocusLine { id: string; text: string; kind: 'praise' | 'fix' }
function dedupe(xs: FocusLine[]) { const seen = new Set<string>(); return xs.filter(x => !seen.has(x.id) && (seen.add(x.id), true)); }
/** Alternate praise and fixes (never only fixes when there is something to praise). */
export function interleave(l: { praise: FocusLine[]; fixes: FocusLine[]; leadWithPraise: boolean }): FocusLine[] {
  const a = l.leadWithPraise ? l.praise : l.fixes, b = l.leadWithPraise ? l.fixes : l.praise, out: FocusLine[] = [];
  for (let i = 0; i < Math.max(a.length, b.length); i++) { if (a[i]) out.push(a[i]); if (b[i]) out.push(b[i]); }
  return out;
}
const HIGHLIGHT_ORDER = ['last_windows', 'last_base_push', 'last_urn', 'last_death_spend', 'last_low_dead', 'last_deathless', 'last_spent', 'last_lanes', 'last_team_lead'];
const SPECIFICITY: Record<string, number> = { dying: 1.3, unspent_souls: 1.1, burst: 0.6, low_hp: 0.7, respawn_deaths: 0.8, repeat_killer: 0.9 };
const PRIORITY = ['dying', 'unspent_souls', 'close_range', 'repeat_killer', 'alone_deaths', 'unused_windows', 'forward_unseen', 'fight_lost', 'respawn_deaths', 'weak_entry', 'burst', 'missed_fights', 'low_hp', 'jungle_heavy', 'deep_waves'];
function mode<T>(xs: Array<T | null | undefined>): T | null {
  const m = new Map<T, number>(); for (const x of xs) if (x != null) m.set(x, (m.get(x) || 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}
const cap = (s: string) => s ? s[0].toUpperCase() + s.slice(1) : s;

// ---------------------------------------------------------------------------------------------
// Match focus goals (docs/coach-review-13.md item 13)
// ---------------------------------------------------------------------------------------------
interface GoalDef {
  id: string; text: string; short: string;
  /** Spoken at the respawn after a death that broke the goal (<= 15 words). */
  respawn: string;
  /** Death-based goal: the death breaks it (null = not measurable for this death). */
  death?: (d: DeathRecord) => boolean | null;
  /** Match-based goal: events (count) and whether it can be scored at all. */
  match?: (m: MatchRecord) => { events: number; measurable: boolean; detail: string };
  /** Practice tip for the Player page. */
  tip: string;
}
/** Measurable goals, most specific first (the order breaks ties). */
export const GOALS: GoalDef[] = [
  { id: 'won_fight_low', text: 'No deaths below 35% health after a won fight', short: 'no deaths below 35% after a won fight',
    respawn: 'Goal check: that was a low-health death after a won fight. Next time, heal first.', death: d => d.lowAfterWin ?? null,
    tip: 'After a won fight, back off and heal before the objective; take it together at full health.' },
  { id: 'close_range', text: 'No close-range deaths', short: 'no close-range deaths',
    respawn: 'Goal check: that was a close-range death. Next life, hold max range behind the front line.', death: d => d.close,
    tip: 'Fight from behind your front line and save your escape for the dive.' },
  { id: 'unspent_souls', text: 'Spend at the usual buy (1,600 / 3,200 / 6,400): no death holding half an item tier', short: 'spend at the usual buy',
    respawn: 'Goal check: you died holding an item\'s worth of souls. Spend before you walk out.', death: d => d.souls == null ? null : d.souls >= deathSpend(d.clock),
    tip: 'Shop when your wave is safe, and never walk into a fight with an item in your pocket.' },
  { id: 'respawn_deaths', text: 'No deaths within 60 s of a respawn', short: 'no deaths right after a respawn',
    respawn: 'Goal check: that came right after a respawn. Walk out with a teammate this time.', death: d => d.afterRespawnSecs == null ? null : d.afterRespawnSecs <= 60,
    tip: 'After a respawn, walk out with the next teammate or catch the closest safe wave.' },
  { id: 'alone_deaths', text: 'No deaths with no ally near', short: 'no deaths alone',
    respawn: 'Goal check: you died alone. Next life, stay within reach of a teammate.', death: d => d.alone,
    tip: 'Farm the side of the map your team is on; rotate with someone.' },
  { id: 'weak_entry', text: 'No deaths after going back in under 60% health', short: 'no going back in hurt',
    respawn: 'Goal check: you went back in hurt. Heal to full before the next fight.', death: d => d.weakEntry,
    tip: 'Heal to full before you rejoin; shopping does not heal.' },
  { id: 'forward_unseen', text: 'No deaths in their half with 3+ enemies unseen', short: 'no deaths forward with enemies missing',
    respawn: 'Goal check: you were forward with enemies missing. Hold back until they show.', death: d => d.forwardUnseen,
    tip: 'When three enemies are missing, stay on your side of the wave.' },
  { id: 'unused_windows', text: 'Push every enemy-dead window', short: 'push every enemy-dead window', respawn: '',
    match: m => ({ events: m.windows.unused, measurable: m.windows.used + m.windows.unused > 0, detail: `${m.windows.used} of ${m.windows.used + m.windows.unused} enemy-dead windows used${m.windows.unusedAt.length ? ` (unused at ${m.windows.unusedAt.map(clockText).join(', ')})` : ''}` }),
    tip: 'When two or more enemies are dead, group and hit the nearest standing structure.' },
];
const goalDef = (id: string) => GOALS.find(g => g.id === id) ?? null;
const countedRecent = (matches: MatchRecord[]) => matches.filter(m => !excludedReason(m)).slice(0, LIMITS.goal_matches);
/** 1-2 measurable goals from the weaknesses of the last 3 counted matches (newest first). */
export function chooseGoals(matches: MatchRecord[], max: number = LIMITS.goal_max): MatchGoal[] {
  const recent = countedRecent(matches);
  if (!recent.length) return [];
  const scored = GOALS.map((g, order) => {
    let events = 0, inMatches = 0, score = 0, newestClean = false;
    recent.forEach((m, age) => {
      const n = g.death ? m.deaths.filter(d => g.death!(d) === true).length : g.match!(m).events;
      // The newest match already got it right (measurably): not this match's goal (as focusLines' CONTRADICTS).
      if (age === 0 && !n) newestClean = g.death ? m.deaths.some(d => g.death!(d) === false) : g.match!(m).measurable;
      if (n) { events += n; inMatches++; score += n * Math.pow(0.85, age); }
    });
    return { g, order, events, inMatches, score, newestClean };
  }).filter(x => !x.newestClean && (x.events >= LIMITS.goal_events || x.inMatches >= 2)).sort((a, b) => b.score - a.score || a.order - b.order);
  const out: MatchGoal[] = scored.slice(0, max).map(x => ({ id: x.g.id, text: x.g.text }));
  // Many deaths and no specific pattern: a death cap below the recent median.
  if (!out.length) {
    const ds = recent.map(m => m.deaths.length).sort((a, b) => a - b), median = ds[Math.floor(ds.length / 2)];
    if (median >= 5) { const target = Math.max(2, Math.round(median * 0.7)); out.push({ id: 'deaths', text: `At most ${target} deaths`, target }); }
  }
  return out;
}
/** Score one goal on a (finished or running) match. */
export function scoreGoal(goal: MatchGoal, m: MatchRecord, running = false): GoalResult {
  if (goal.id === 'deaths') {
    const n = m.deaths.length, t = goal.target ?? 0;
    return { ...goal, met: n <= t ? (running ? null : true) : false, detail: `${n} death${n === 1 ? '' : 's'} (goal at most ${t})` };
  }
  const def = goalDef(goal.id);
  if (!def) return { ...goal, met: null, detail: 'not measurable in this version' };
  if (def.match) {
    const r = def.match(m);
    return { ...goal, met: !r.measurable ? null : r.events === 0 ? (running ? null : true) : false, detail: r.measurable ? r.detail : 'no enemy-dead window observed' };
  }
  const v = m.deaths.map(d => ({ d, v: def.death!(d) }));
  const broke = v.filter(x => x.v === true), unknown = v.filter(x => x.v == null).length;
  if (broke.length) return { ...goal, met: false, detail: `broken ${broke.length} time${broke.length === 1 ? '' : 's'} (${broke.map(x => clockText(x.d.clock)).join(', ')}) in ${m.deaths.length} death${m.deaths.length === 1 ? '' : 's'}` };
  const met = running ? null : unknown === m.deaths.length && m.deaths.length > 0 ? null : true;
  return { ...goal, met, detail: m.deaths.length ? `${m.deaths.length} death${m.deaths.length === 1 ? '' : 's'}, none broke it${unknown ? ` (${unknown} not measurable)` : ''}` : 'no deaths' };
}
export const scoreGoals = (goals: MatchGoal[], m: MatchRecord, running = false) => goals.map(g => scoreGoal(g, m, running));
/** The goal(s) a death broke. */
export function brokenBy(goals: MatchGoal[], d: DeathRecord): MatchGoal[] {
  return goals.filter(g => goalDef(g.id)?.death?.(d) === true);
}
/** "Match focus" line spoken at the match start (<= 15 words). */
export function goalStartLine(goals: MatchGoal[]): string | null {
  const shorts = goals.map(goalShort).filter(Boolean);
  return shorts.length ? `Profile focus: this match, ${shorts.join(' and ')}.` : null;
}
export function goalShort(g: MatchGoal): string { return g.id === 'deaths' ? g.text.toLowerCase() : goalDef(g.id)?.short ?? g.text.toLowerCase(); }
export const goalTip = (id: string) => goalDef(id)?.tip ?? null;

// ---------------------------------------------------------------------------------------------
// Persistence + live API
// ---------------------------------------------------------------------------------------------
export interface KeyValueStore { getItem(key: string): string | null; setItem(key: string, value: string): void; removeItem(key: string): void }
export interface StoredProfile { schema: number; matches: MatchRecord[]; current?: MatchRecord | null; lastFocusAt?: number | null;
  /** Focus rotation: cues said since the newest completed match `focusFor` (praise / fix alternate). */
  focusTurn?: number; focusFor?: string | null;
  /** Focus goals of the match in progress (chosen at its start). */
  goals?: MatchGoal[] }

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
    ...(Array.isArray(d.effects) ? { effects: d.effects.filter((e: unknown) => typeof e === 'string').slice(0, 6) } : {}),
    ...(finite(d.deadSecs) && d.deadSecs >= 0 ? { deadSecs: d.deadSecs } : {}),
    ...(finite(d.spentDead) && d.spentDead > 0 ? { spentDead: d.spentDead } : {}),
    ...(typeof d.lowAfterWin === 'boolean' ? { lowAfterWin: d.lowAfterWin } : {}) }));
  const goals = Array.isArray(m.goals) ? m.goals.filter((g: unknown) => g && typeof g === 'object' && typeof (g as GoalResult).id === 'string').slice(0, 4).map((g: GoalResult) => ({
    id: g.id.slice(0, 30), text: str(g.text, 120) || g.id.slice(0, 30), ...(finite(g.target) ? { target: g.target } : {}), met: typeof g.met === 'boolean' ? g.met : null, detail: str(g.detail, 160) || '' })) : null;
  const o = (v: unknown) => v && typeof v === 'object' ? v as Record<string, unknown> : {};
  const f = o(m.fights), w = o(m.windows), s = o(m.souls), l = o(m.lowHp), u = o(m.urn), tm = o(m.team);
  const pairs = (v: unknown) => Array.isArray(v) ? v.filter((p): p is [number, number] => Array.isArray(p) && p.length === 2 && finite(p[0]) && finite(p[1])).slice(0, 10).map(p => [p[0], p[1]] as [number, number]) : [];
  return { v: 1, id: str(m.id, 40) || `import-${Math.random().toString(36).slice(2, 10)}`, source: m.source === 'replay' || m.source === 'import' ? m.source : 'live',
    startedAt: num(m.startedAt, Date.now()), endedAt: num(m.endedAt, num(m.startedAt, Date.now())), hero: typeof m.hero === 'string' && /^hero_[a-z_]+$/.test(m.hero) ? m.hero : null,
    matchId: numOrNull(m.matchId), startClock: numOrNull(m.startClock), endClock: numOrNull(m.endClock), observedSecs: num(m.observedSecs, Object.values(time).reduce((a, b) => a + b, 0)),
    result: m.result === 'win' || m.result === 'loss' ? m.result : null, ...(m.partial === true ? { partial: true } : {}), ...(typeof m.fingerprint === 'string' ? { fingerprint: m.fingerprint.slice(0, 80) } : {}),
    time, ...(finite(m.spanSecs) && m.spanSecs >= 0 && finite(m.deadSecs) && m.deadSecs >= 0 ? { spanSecs: m.spanSecs, deadSecs: Math.min(m.deadSecs, m.spanSecs) } : {}),
    ...(finite(m.fieldSecs) && m.fieldSecs >= 0 ? { fieldSecs: m.fieldSecs } : {}),
    deaths, fights: { joined: num(f.joined), missed: num(f.missed), whileDead: num(f.whileDead) },
    windows: { used: num(w.used), unused: num(w.unused), unusedAt: Array.isArray(w.unusedAt) ? w.unusedAt.filter(finite).slice(0, 20) : [],
      ...(Array.isArray(w.usedAt) ? { usedAt: w.usedAt.filter(finite).slice(0, 20) } : {}), ...(finite(w.base) ? { base: w.base } : {}) },
    ...(m.urn && typeof m.urn === 'object' ? { urn: { carried: num(u.carried), delivered: num(u.delivered), lost: num(u.lost), at: pairs(u.at) } } : {}),
    ...(m.team && typeof m.team === 'object' && finite(tm.reads) ? { team: { reads: num(tm.reads), leadFrom: numOrNull(tm.leadFrom), maxLead: num(tm.maxLead), lastLead: numOrNull(tm.lastLead) } } : {}),
    souls: { knownSecs: num(s.knownSecs), hoardSecs: num(s.hoardSecs), peak: num(s.peak) },
    lowHp: { knownSecs: num(l.knownSecs), lowSecs: num(l.lowSecs), episodes: num(l.episodes), survived: num(l.survived), died: num(l.died) },
    deepAwaySecs: num(m.deepAwaySecs), farm: { perMin: numOrNull(o(m.farm).perMin) },
    kda: m.kda && typeof m.kda === 'object' ? { kills: numOrNull(m.kda.kills), deaths: numOrNull(m.kda.deaths), assists: numOrNull(m.kda.assists) } : null,
    ...(goals?.length ? { goals } : {}) };
}
/** Versioned schema. Schema 0 (pre-release) was a bare array of matches. */
export function migrate(raw: unknown): StoredProfile {
  if (Array.isArray(raw)) raw = { schema: 1, matches: raw };
  if (!raw || typeof raw !== 'object') return { schema: PLAYER_PROFILE_SCHEMA, matches: [] };
  const r = raw as Record<string, unknown>;
  if (finite(r.schema) && r.schema > PLAYER_PROFILE_SCHEMA) throw new Error(`Player profile schema ${r.schema} is newer than this app (${PLAYER_PROFILE_SCHEMA}).`);
  const matches = (Array.isArray(r.matches) ? r.matches : []).map(sanitizeMatch).filter((m): m is MatchRecord => !!m);
  const current = r.current ? sanitizeMatch(r.current) : null;
  return { schema: PLAYER_PROFILE_SCHEMA, matches: matches.sort((a, b) => b.startedAt - a.startedAt).slice(0, MAX_MATCHES), current, lastFocusAt: numOrNull(r.lastFocusAt),
    focusTurn: num(r.focusTurn), focusFor: str(r.focusFor, 40),
    goals: Array.isArray(r.goals) ? (r.goals as unknown[]).filter((g): g is MatchGoal => !!g && typeof g === 'object' && typeof (g as MatchGoal).id === 'string' && typeof (g as MatchGoal).text === 'string')
      .slice(0, LIMITS.goal_max).map(g => ({ id: g.id.slice(0, 30), text: g.text.slice(0, 120), ...(finite(g.target) ? { target: g.target } : {}) })) : [] };
}

/** Ending a match without a result screen (docs/ai-coach.md "End-of-game review"). rec13 ended
 *  while dead: "Defeat" over the death screen (Dead), then over the Tab scoreboard (Scoreboard),
 *  a black frame (Loading), the MVP screens (Unknown) and the post-game summary (Shop with the
 *  last clock and hero): no PostGame frame, so the match stayed open and was never reviewed. */
export const MATCH_END = {
  /** Only a match observed this long ends without a result screen (or on Stop). */
  min_span_secs: 600,
  /** A black loading frame and >= 3 Loading / Unknown frames, this long after the last live HUD
   *  frame. Unknown frames alone never end a match: rec6 had 3-minute Unknown runs mid-match (a
   *  vision bug), alt-tab shows other windows; the end of a match always passes a black frame. */
  idle_secs: 20, idle_frames: 3,
  /** Never before the own respawn timer (up to ~90 s) has run out plus this; a death screen
   *  without a timer read waits `dead_wait_secs`. */
  respawn_grace_secs: 20, dead_wait_secs: 90,
  /** Replays without wall time: frames instead (one loading frame needed). */
  replay_frames: 5,
} as const;
/** Watches for the end of a match without a result screen. Live HUD frames (a clock that moves,
 *  or the death screen; a shop with souls or while dead) keep the match going; Loading and
 *  Unknown frames are end evidence; menus (pause), countdowns, shops without souls and HUD frames
 *  with a frozen clock (pause, end-of-game camera) are neutral: they neither count nor reset. */
export class MatchEndWatch {
  private liveWall: number | null = null; private liveClock: number | null = null; private liveDead = false;
  private frames = 0; private loading = 0; private deadline: number | null = null; private timer: number | null = null;
  reset() { this.liveWall = null; this.liveClock = null; this.liveDead = false; this.frames = 0; this.loading = 0; this.deadline = null; this.timer = null; }
  /** Feed one frame (in order); true when the match looks over. */
  observe(s: Snapshot, wall: number | null, spanSecs: number): boolean {
    const read = finite(s.game_clock_secs) ? Math.round(s.game_clock_secs) : null;
    const dead = s.dead === true || s.screen === 'Dead';
    const hud = s.screen === 'Gameplay' || s.screen === 'Dead' || s.screen === 'Scoreboard' || (s.screen == null && read != null);
    const live = (hud && read != null && (this.liveClock == null || read !== this.liveClock || dead)) || (s.screen === 'Shop' && (dead || finite(s.souls)));
    if (live) {
      this.liveWall = wall; if (read != null) this.liveClock = read; this.liveDead = dead; this.frames = 0; this.loading = 0;
      const timer = finite(s.respawn_secs) && s.respawn_secs >= 0 && s.respawn_secs <= 120 ? s.respawn_secs : null;
      if (!dead) { this.deadline = null; this.timer = null; } else if (timer != null) { this.timer = timer; if (wall != null) this.deadline = wall + timer * 1000; }
      return false;
    }
    // An Unknown frame that still reads a clock is a HUD the classifier missed (rec4), not a menu.
    if (s.screen === 'Loading' || (s.screen === 'Unknown' && read == null)) { this.frames++; if (s.screen === 'Loading') this.loading++; }
    if (spanSecs < MATCH_END.min_span_secs || this.liveClock == null) return false;
    // Without wall time a death screen counts only when its last timer read was about to run out.
    if (wall == null || this.liveWall == null) return (!this.liveDead || (this.timer != null && this.timer <= MATCH_END.respawn_grace_secs)) && this.loading >= 1 && this.frames >= MATCH_END.replay_frames;
    const idle = (wall - this.liveWall) / 1000;
    if (this.liveDead && (this.deadline != null ? wall < this.deadline + MATCH_END.respawn_grace_secs * 1000 : idle < MATCH_END.dead_wait_secs)) return false;
    return this.loading >= 1 && this.frames >= MATCH_END.idle_frames && idle >= MATCH_END.idle_secs;
  }
  /** A black frame has been seen since the last live frame: later non-live frames (the MVP and
   *  summary screens, read as Unknown / Shop with an extrapolated clock) are not game time. */
  get suspended() { return this.loading > 0; }
}

export interface ObserveEvents { matchStarted: boolean; respawned: boolean; died: boolean; ended: MatchRecord | null;
  /** How `ended` was decided: a result screen, a match boundary (clock reset, new id, long gap) or the end watch. */
  endedBy?: 'result' | 'boundary' | 'idle' }
export interface FocusCue { key: string; text: string; priority: 'Normal'; cooldown_secs: number }
export const FOCUS_COOLDOWN_MS = 5 * 60_000;

/** The profile: completed matches (newest first) plus the match in progress. */
export class PlayerProfile {
  readonly model = new PlayerProfileModel();
  matches: MatchRecord[] = [];
  current: MatchAggregator | null = null;
  lastFocusAt: number | null = null;
  /** Focus rotation (persisted): cues said since the newest completed match `focusFor`. */
  focusTurn = 0; focusFor: string | null = null;
  /** Focus goals of the match in progress (chosen from the last 3 counted matches at its start). */
  goals: MatchGoal[] = [];
  /** Focus cues of the match in progress: the start line was said; deaths (clock) already followed up. */
  private startSaidFor: MatchAggregator | null = null; private respawnSaid = new Set<number>(); private goalSaidAt: Record<string, number> = {};
  private store: KeyValueStore | null; private key: string; private now: () => number;
  private lastSave = 0; private resultSeen = false;
  /** The final clock and wall time of a match ended by its result screen (see after_result_secs). */
  private ended: { clock: number | null; wall: number | null } | null = null;
  private endWatch = new MatchEndWatch();
  constructor(opts: { store?: KeyValueStore | null; key?: string; now?: () => number } = {}) {
    this.store = opts.store ?? null; this.key = opts.key ?? PLAYER_PROFILE_KEY; this.now = opts.now ?? (() => Date.now());
  }
  setHeroNames(names: Record<string, string>) { this.model.names = names; }

  load(): this {
    if (!this.store) return this;
    try {
      const text = this.store.getItem(this.key); if (!text) return this;
      const data = migrate(JSON.parse(text));
      this.matches = data.matches; this.lastFocusAt = data.lastFocusAt ?? null; this.focusTurn = data.focusTurn ?? 0; this.focusFor = data.focusFor ?? null; this.goals = data.current ? data.goals ?? [] : [];
      // A match left open by a closed tab is kept as a partial match.
      if (data.current && (known(data.current) >= LIMITS.min_match_secs || data.current.deaths.length)) this.addMatch({ ...data.current, partial: true, ...(this.goals.length ? { goals: scoreGoals(this.goals, data.current) } : {}) }, false);
    } catch { /* corrupt or newer data: keep it untouched in storage, start empty in memory */ this.store = null; }
    return this;
  }
  save(): boolean {
    if (!this.store) return false;
    const data: StoredProfile = { schema: PLAYER_PROFILE_SCHEMA, matches: this.matches.slice(0, MAX_MATCHES), lastFocusAt: this.lastFocusAt, focusTurn: this.focusTurn, focusFor: this.focusFor, goals: this.goals,
      current: this.current?.hasEvidence ? this.current.record(this.now(), true) : null };
    try { this.store.setItem(this.key, JSON.stringify(data)); this.lastSave = this.now(); return true; } catch { return false; }
  }

  /** Feed one snapshot. `wallMs` = when the frame was captured (null for replays without it). */
  observe(s: Snapshot, wallMs: number | null = null, source: MatchRecord['source'] = 'live'): ObserveEvents {
    const ev: ObserveEvents = { matchStarted: false, respawned: false, died: false, ended: null };
    if (!s || typeof s !== 'object') return ev;
    const wall = finite(wallMs) ? wallMs : null;
    // The Victory / Defeat banner read by the vision (also over a death screen or the scoreboard).
    if (this.current && (s.match_result === 'win' || s.match_result === 'loss')) this.current.result = s.match_result;
    const result = resultOf(s.screen);
    if (result) {
      if (this.current && !this.resultSeen) {
        if (result !== 'unknown') this.current.result = result; this.resultSeen = true;
        this.ended = { clock: this.current.clock, wall: wall ?? this.current.lastWall }; ev.ended = this.endMatch();
        if (ev.ended) ev.endedBy = 'result';
      }
      return ev;
    }
    const read = finite(s.game_clock_secs) ? s.game_clock_secs : null;
    // No result screen: loading / unknown screens long enough after the last live HUD frame.
    if (this.current && this.endWatch.observe(s, wall, this.current.spanSecs)) {
      this.ended = { clock: this.current.clock, wall: wall ?? this.current.lastWall }; ev.ended = this.endMatch();
      if (ev.ended) ev.endedBy = 'idle';
      return ev;
    }
    // After a black frame, non-live frames wait for the verdict outside the match (rec8 / rec13:
    // the post-game summary read as Shop with an extrapolated clock added ~20 s of "game time").
    if (this.current && this.endWatch.suspended) return ev;
    const cur = this.current;
    // Menus and loading screens long after the last clocked frame belong to no match.
    if (cur && read == null && wall != null && cur.lastReadWall != null && wall - cur.lastReadWall > LIMITS.stale_unclocked_ms) return ev;
    if (cur) {
      const gap = wall != null && cur.lastWall != null ? wall - cur.lastWall : null;
      const reset = read != null && cur.clock != null && cur.clock > 120 && read < 60;
      const newId = finite(s.match_id) && cur.matchId != null && digitDiff(cur.matchId, s.match_id) >= 3 && !cur.continuous(read, wall);
      const longGap = gap != null && (gap > LIMITS.wall_gap_new_match_ms || (gap > LIMITS.wall_gap_check_ms && read != null && !cur.continuous(read, wall)));
      if (reset || newId || longGap) { ev.ended = this.endMatch(); if (ev.ended) ev.endedBy = 'boundary'; }
    }
    if (!this.current) {
      // A match starts with readable gameplay evidence (a clock on a gameplay or death screen).
      if (read == null || (s.screen !== 'Gameplay' && s.screen !== 'Dead' && s.dead !== true)) return ev;
      // The end-of-game camera after a result screen still shows the final clock: not a new match.
      const e = this.ended;
      if (e && e.clock != null && read >= 60 && Math.abs(read - e.clock) <= LIMITS.after_result_secs && !(wall != null && e.wall != null && wall - e.wall > LIMITS.wall_gap_check_ms)) return ev;
      this.current = new MatchAggregator(wall ?? this.now(), source); this.resultSeen = false; this.ended = null; this.endWatch.reset();
      // The focus goals of this match: from the last 3 counted matches' weaknesses.
      this.goals = chooseGoals(this.matches); this.respawnSaid = new Set(); this.goalSaidAt = {};
      ev.matchStarted = true;
    }
    const r = this.current.add(s, wall);
    ev.respawned = r.respawned; ev.died = r.died;
    if (source === 'live' && this.now() - this.lastSave > 60_000) this.save();
    return ev;
  }
  /** A black frame since the last live frame: the frames now are not part of the match (MATCH_END). */
  get suspended() { return this.current != null && this.endWatch.suspended; }
  /** Capture stopped (Stop, share ended): a match observed at least `minSecs` is over; a shorter
   *  one stays open (a long gap or a new clock ends it later). */
  finishSession(minSecs: number = MATCH_END.min_span_secs): MatchRecord | null {
    const cur = this.current; if (!cur || cur.spanSecs < minSecs) return null;
    this.ended = { clock: cur.clock, wall: cur.lastWall };
    return this.endMatch();
  }
  /** Close the match in progress; kept when it had enough evidence. */
  endMatch(): MatchRecord | null {
    const cur = this.current; this.current = null;
    if (!cur || !cur.hasEvidence) return null;
    const rec = cur.record(this.now());
    if (this.goals.length) rec.goals = scoreGoals(this.goals, rec);
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
  /** Short spoken lines, praise and fixes alternating ("Keep it up: ..." / "Profile focus: ..."),
   *  praise first after a clearly improved match. */
  focusTips(n = 1, hero?: string | null): string[] {
    return interleave(this.model.focusLines(this.all(), hero)).slice(0, Math.max(0, n)).map(l => l.text);
  }
  /** The goals of the match in progress, else the ones the next match will get. */
  nextGoals(): { goals: MatchGoal[]; current: boolean } {
    return this.current && this.goals.length ? { goals: this.goals, current: true } : { goals: chooseGoals(this.matches), current: false };
  }
  /** The focus goals of the match in progress with their progress so far (for the AI prompt), or null. */
  goalText(): string | null {
    if (!this.current || !this.goals.length) return null;
    const rec = this.current.record(this.now(), true);
    return scoreGoals(this.goals, rec, true).map(r => `${r.text} (so far: ${r.met === false ? r.detail : `on track, ${r.detail}`})`).join(' · ');
  }
  /** Focus cue (docs/player-profile.md "Live reminders"): at most once per match start and once per
   *  respawn when relevant. Start: the match focus goals ("Match focus: no close-range deaths and
   *  spend at the usual buy."), else one line of the praise / fix rotation learned from completed
   *  matches. Respawn: only when the death broke a focus goal, and never praise (rec13 said "Keep it
   *  up: last game you went 9 minutes without dying" 5 s after a death). Without a match in
   *  progress (tests, the Player page) a start cue is limited to one per 5 minutes. */
  focusCue(kind: 'start' | 'respawn' = 'start'): FocusCue | null {
    const now = this.now(), cur = this.current;
    if (kind === 'respawn') {
      const d = cur?.lastDeath;
      if (!cur || !d || this.respawnSaid.has(d.clock)) return null;
      // Not when the death screen already fixed it (souls spent before the respawn), and one goal
      // check per goal per 5 minutes of game time (rec11 would have heard the same line 7 times).
      const broken = brokenBy(this.goals, d).find(g => !(g.id === 'unspent_souls' && (d.spentDead ?? 0) >= deathSpend(d.clock))
        && !(this.goalSaidAt[g.id] != null && d.clock - this.goalSaidAt[g.id] < FOCUS_COOLDOWN_MS / 1000));
      if (!broken) return null;
      this.respawnSaid.add(d.clock); this.lastFocusAt = now; this.goalSaidAt[broken.id] = d.clock;
      const text = GOALS.find(g => g.id === broken.id)?.respawn || `Goal check: that death broke our focus goal: ${goalShort(broken)}.`;
      return { key: `player:focus:goal:${broken.id}`, text, priority: 'Normal', cooldown_secs: 60 };
    }
    if (cur) { if (this.startSaidFor === cur) return null; }
    else if (this.lastFocusAt != null && now - this.lastFocusAt < FOCUS_COOLDOWN_MS) return null;
    const goals = cur ? this.goals : chooseGoals(this.matches);
    const goalLine = goalStartLine(goals);
    let seq = interleave(this.model.focusLines(this.matches));
    // Never praise within 60 s of a death of this match, nor contradict it ("no deaths" lines after one).
    const deaths = cur?.deathClocks ?? [];
    const lastDeath = deaths.at(-1);
    if (lastDeath != null && cur?.clock != null && cur.clock - lastDeath < LIMITS.praise_after_death_secs) seq = seq.filter(l => l.kind === 'fix');
    else if (deaths.length) seq = seq.filter(l => !(l.kind === 'praise' && /deathless|low_dead|improved_dead|improved_deaths/.test(l.id)));
    const cue = (key: string, text: string): FocusCue => { this.lastFocusAt = now; this.startSaidFor = cur; return { key, text, priority: 'Normal', cooldown_secs: FOCUS_COOLDOWN_MS / 1000 }; };
    if (!seq.length) return goalLine ? cue('player:focus:goal', goalLine) : null;
    const latest = this.matches.find(m => !excludedReason(m))?.id ?? null;
    if (latest !== this.focusFor) { this.focusFor = latest; this.focusTurn = 0; }
    const line = seq[this.focusTurn % seq.length];
    this.focusTurn++;
    // The match focus goal is the fix of the match start, after one specific praise of the last
    // matches when there is one (the coach never only lists what went wrong).
    if (goalLine) { const praise = line.kind === 'praise' ? line : seq.find(l => l.kind === 'praise'); return cue('player:focus:goal', praise ? `${praise.text} ${goalLine}` : goalLine); }
    return cue(`player:focus:${line.id}`, line.text);
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
  reset() { this.matches = []; this.current = null; this.lastFocusAt = null; this.focusTurn = 0; this.focusFor = null; this.goals = []; try { this.store?.removeItem(this.key); } catch { /* ignore */ } }
}
