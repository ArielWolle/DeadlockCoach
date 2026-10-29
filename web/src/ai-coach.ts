// Optional AI coach: DOM-free logic (history, state summary, knowledge retrieval, prompt,
// provider request shapes, reply parsing) plus a small controller that owns triggers,
// the single in-flight request, staleness and error backoff. The rule-based coach is
// unchanged; this module only adds advice on top of it. Nothing here touches the DOM, so
// tests/unit.test.ts can import it directly. The only value import is player-profile.ts, which is
// DOM-free as well (it owns the time-scaled souls tiers shared with the rule engine).
//
// Input budget (documented in docs/ai-coach.md): ~2,500 tokens, measured as chars / 4.
//   system prompt ≈ 2,300 chars · coach brief ≈ 1,500 · state summary ≤ 3,000
//   player profile ≤ 900 · retrieved excerpts ≤ 2,100 (≤ 3 × 700) · task/framing ≈ 500
// buildPrompt() enforces MAX_INPUT_CHARS by shrinking excerpts, then profile, then summary.
//
// Quality gates (docs/ai-coach.md "Repetition and souls gating"): every automatic answer passes
// checkAdvice() before it is spoken. Souls sentences are dropped unless the souls gate is open
// (the usual buy for the game time is affordable and the rule coach has not said it in 3 min, or a
// death with at least the death-spend threshold). Theme memory: a theme (positioning, healing,
// team, objective, economy, map, itemization, wave, mechanics) or a profile weakness concept (close
// range, walking out alone, ...) that was coached in the last 5 min is not coached again unless a
// NEW concrete trigger for it happened since (a diver now near you, a new respawn, a new
// enemy-dead window...). Near-identical text to a recent AI or rule call is not spoken either.
//
// Cadence (docs/ai-coach.md "When the coach asks"): periodic requests fire on meaningful changes
// (phase change, respawn, enemy-dead window, objective within 60 s, souls tier reached, big team
// souls swing) at most every EVENT_GAP_SECS, otherwise every `aiInterval` (default 90 s).
//
// Match boundaries (docs/ai-coach.md "Match boundaries"): the HUD match id repeats across matches
// (rec11 and rec12 both read 127139499), so AiHistory starts a new match itself when the clock
// restarts (also across clock-less menu frames), after a post-game screen, when the hero changes
// with a clock that is not continuous, or after a long gap with an impossible clock jump. Death
// evidence then only comes from the current death: rule death reviews of its time window, damage
// attributed to this match's enemy heroes, and named heroes in a reply are checked against this
// match's roster (rec12 named Baba and Graves from the previous match).
//
// Reinforcement (docs/ai-coach.md "Reinforcing good plays"): AiHistory derives cheap good-play
// signals (an enemy-dead window used to push, the urn delivered, souls spent on the death screen,
// deathless stretches, a low-health reset, a growing team lead). A fresh one is a cadence event
// and may be praised in one short specific sentence: focus 'reinforce', at most 1 in 4 AI tips.
import { deathSpend, resultOf, spendTarget } from './player-profile.ts';
import type { Cue, Knowledge, Priority, Settings, Snapshot } from './types';

export type AiProvider = 'openai' | 'anthropic';
export type AiTrigger = 'periodic' | 'death' | 'manual';
/** Reasoning ("thinking") budget sent to the gateway (Settings › Model reasoning). */
export type AiReasoning = 'off' | 'low' | 'default';
export interface AiConfig { provider: AiProvider; baseUrl: string; model: string; key: string; maxTokens: number; headers: string;
  /** Unset = 'default' (nothing is sent; the model decides). configFrom() fills it from Settings. */
  reasoning?: AiReasoning }
/** Snapshot plus optional fields other perception work may add. All are optional. */
export type AiSnapshot = Snapshot & {
  status_effects?: string[] | null;
  death_damage?: Array<{ source?: string | null; damage?: number | null }> | null;
  death_killing_blow?: unknown;
  level?: number | null; unsecured_souls?: number | null;
  kills?: number | null; deaths?: number | null; assists?: number | null;
  minimap?: (NonNullable<Snapshot['minimap']> & { zone?: string | null }) | null;
};

export const MAX_INPUT_CHARS = 10000; // ≈ 2,500 tokens
export const SUMMARY_MAX_CHARS = 3000;
export const PROFILE_MAX_CHARS = 900;
export const EXCERPT_MAX_CHARS = 700;
export const EXCERPTS_MAX = 3;
export const HISTORY_WINDOW_SECS = 90;
export const STALE_MS = 15000;
/** No two automatic AI calls on the same topic within this much game time (unless urgent). */
export const REPEAT_WINDOW_SECS = 180;
/** A rule-coach souls reminder ('unspent*') closes the AI souls gate for this long. */
export const SOULS_RULE_WINDOW_SECS = 180;
/** Near-duplicate text: share of the shorter call's content words found in the other one. */
export const NEAR_DUPLICATE = 0.6;
/** Earlier AI calls compared for near-duplicates (game seconds). */
export const DUPLICATE_WINDOW_SECS = 300;
/** One alternative is asked for only when the rejected answer came back this fast (cheap). */
export const RETRY_MAX_MS = 2500;
/** Just respawned = alive again for at most this long (review.json respawn_window_secs is 40). */
export const RESPAWN_SECS = 45;
/** Death review request: after the rule death review or this long after the death panel is read… */
export const DEATH_WAIT_MS = 4000;
/** …and at the latest this long after the death (even without a readable panel). */
export const DEATH_WAIT_MAX_MS = 8000;
/** A theme or weakness concept is coached at most once per this much game time, unless a NEW
 *  concrete trigger for it happened since (see THEME_TRIGGERS / CONCEPTS). */
export const THEME_WINDOW_SECS = 300;
/** Event-driven periodic requests (respawn, enemy-dead window, ...) are at least this far apart. */
export const EVENT_GAP_SECS = 45;
/** An event older than this (wall seconds) no longer triggers a request. */
export const EVENT_FRESH_SECS = 40;
/** Team souls difference change since the last periodic request that counts as a big swing. */
export const NW_SWING = 4000;
/** Wall seconds without a clocked frame after which a hero change or an impossible clock jump is a new match. */
export const LONG_GAP_SECS = 180;
/** Consecutive reads of another hero (clock continuous) before the hero switches without a new match. */
export const HERO_CONFIRM = 3;
/** A good play can be praised for this much game time after it happened. */
export const REINFORCE_FRESH_SECS = 90;
/** At most one praise among this many AI tips (1 in 4), and at least this much game time apart. */
export const REINFORCE_EVERY = 4;
export const REINFORCE_GAP_SECS = 150;
/** Deathless stretches worth a word: first at 7 minutes, then every 5. */
export const DEATHLESS_FIRST_SECS = 420, DEATHLESS_STEP_SECS = 300;
/** All heroes (data/heroes.json class_name → name), for the named-hero check of AI replies. */
export const HERO_NAMES: Record<string, string> = {
  hero_inferno: 'Infernus', hero_gigawatt: 'Seven', hero_hornet: 'Vindicta', hero_ghost: 'Lady Geist', hero_atlas: 'Abrams', hero_wraith: 'Wraith', hero_forge: 'McGinnis',
  hero_chrono: 'Paradox', hero_dynamo: 'Dynamo', hero_kelvin: 'Kelvin', hero_haze: 'Haze', hero_astro: 'Holliday', hero_bebop: 'Bebop', hero_nano: 'Calico', hero_orion: 'Grey Talon',
  hero_krill: 'Mo & Krill', hero_shiv: 'Shiv', hero_tengu: 'Ivy', hero_warden: 'Warden', hero_yamato: 'Yamato', hero_lash: 'Lash', hero_viscous: 'Viscous', hero_synth: 'Pocket',
  hero_mirage: 'Mirage', hero_viper: 'Vyper', hero_magician: 'Sinclair', hero_vampirebat: 'Mina', hero_drifter: 'Drifter', hero_priest: 'Venator', hero_frank: 'Victor',
  hero_bookworm: 'Paige', hero_doorman: 'The Doorman', hero_punkgoat: 'Billy', hero_necro: 'Graves', hero_fencer: 'Apollo', hero_deadpack: 'Deadman Danny', hero_familiar: 'Rem',
  hero_werewolf: 'Silver', hero_unicorn: 'Celeste', hero_ratking: 'Rat King', hero_chessmaster: 'Solomon', hero_artist: 'Violet', hero_nurse: 'Nurse Harrow', hero_baba: 'Baba',
};
export const estimateTokens = (chars: number) => Math.ceil(chars / 4);

export const PROVIDER_PRESETS: Record<AiProvider, { label: string; baseUrl: string; modelHint: string }> = {
  openai: { label: 'OpenAI-compatible (OpenAI, OpenRouter, LiteLLM, Azure proxy, Ollama, LM Studio)', baseUrl: 'https://api.openai.com/v1', modelHint: 'e.g. gpt-4o-mini · openai/gpt-4o-mini · llama3.1' },
  anthropic: { label: 'Anthropic Messages', baseUrl: 'https://api.anthropic.com', modelHint: 'e.g. claude-3-5-haiku-latest' },
};

// ---------------------------------------------------------------------------------------
// Prompt text
// ---------------------------------------------------------------------------------------
export const SYSTEM_PROMPT = `You are an elite Deadlock coach (Valve's 6v6 hero-shooter MOBA) speaking into the player's ear during a live match. A rule-based coach already speaks hundreds of routine reminders (souls, timers, waves, low health, missing enemies, a profile focus line); you speak rarely and only add what a top coach would see that the rules cannot.
Rules:
1. Evidence: use ONLY STATE, PLAYER PROFILE and KNOWLEDGE. Never invent enemy positions, cooldowns, items, health or timers. "unseen", "missing" or "unknown" means unknown, never safe.
2. One idea for the next 30 seconds, shaped as situation → specific action → why (the principle, in a few words). Example: "Two enemies are dead for 15 seconds: group with your nearest ally and hit the Yellow lane Guardian now, because a structure is worth more than chasing kills."
3. Be concrete: name heroes of THIS match only (STATE lists them; PLAYER PROFILE heroes are from other matches), lanes ("Green lane", "in Yellow lane"), items with their cost, and timers from STATE. Green, Blue and Yellow are LANES, never heroes or players: never say "enemy yellow" or "Green is missing".
4. Read WHY NOW first: it says what changed. Coach that change. If WHY NOW is only a routine check and you have nothing new and specific, PASS.
5. Variety: THEMES COACHED lists themes and profile weaknesses already coached. Do not coach them again within 5 minutes unless WHY NOW or COACHING ANGLES shows a NEW trigger for them. Never restate a profile weakness as a generic reminder ("stay at max range behind your front line"): the rule coach already does that. Do not repeat RECENT COACH CALLS.
6. Teach: prefer the insight the player can reuse (why this timing, what the enemy wants, what the trade is worth) over obvious safety lines.
7. Souls: mention souls, spending or buying ONLY if STATE says "Souls advice: allowed". Holding less than the usual buy for the game time is saving, not hoarding; never suggest a cheaper tier than the usual buy.
8. Never guess ability details or a hero's kit you are not certain of; use the role given in STATE.
9. Live advice: at most 2 short sentences and 30 words, natural for text-to-speech (no lists, symbols or markdown). No greetings or hedging.
10. PASS (speak null) when nothing important changed, the evidence is thin, or your idea repeats THEMES COACHED or a recent call. Passing is good: silence keeps the player's attention for the calls that matter.
11. Reinforce what works: when GOOD PLAYS says "Praise: allowed", you may open with ONE short, specific sentence naming the good play and why it worked, then the next action. Example: "Good push while three were dead: that is how a lead snowballs. Next, back off before they respawn." Use focus "reinforce". Never praise without a GOOD PLAYS line, never generic ("nice job"), never when it says not allowed.
Reply with ONLY one JSON object, no code fence, no other text:
{"speak": string or null, "priority": "High" | "Normal" | "Low", "why": "one line citing the evidence you used", "focus": "one of positioning, healing, team, objective, economy, map, itemization, wave, mechanics, reinforce", "weakness": "the PLAYER PROFILE weakness this coaches, or null"}`;

export const TASKS: Record<AiTrigger, string> = {
  periodic: 'TASK: Coach what WHY NOW says changed, with one specific call on a theme not in THEMES COACHED (unless the change is a NEW trigger for it). PASS if nothing new and specific is worth the player\'s attention or the rule coach already covers it.',
  death: 'TASK: The player just died. Explain WHY from the evidence of THIS death only (its Death line, Damage by hero, the rule death review of this death, killing blow, status effects or crowd control, health before, allies near, rule calls just before it such as "outnumbered"), then give the ONE change for the next life. Name only heroes of this match: never one from the PLAYER PROFILE or an earlier death. If the panel cause says Trooper but hero abilities did the damage, heroes killed the player. If THEMES COACHED shows the same weakness was coached recently, say what was different this time instead of repeating the same line. Talk about souls only if STATE says "Souls advice: allowed"; if the player already spent on the death screen, acknowledge it in a few words. Up to 3 short sentences, at most 45 words.',
  manual: 'TASK: The player pressed "Ask the coach now". Give the most useful specific call for this moment, preferably a COACHING ANGLE or a theme not in THEMES COACHED; only PASS if there is no usable evidence.',
};

/** Compact Deadlock facts written from docs/timings and docs/strategy (wiki, retrieved 2026-10-25). */
export const COACH_BRIEF = `DEADLOCK FACTS (wiki 2026-10; patches may change numbers):
- Souls are currency and XP. HUD souls = secured + unsecured (haunted). Haunted souls from camps, crates, Sinner's Sacrifices and the urn are lost on death (a Soul Bag drops only above 50 + 5/min). Secure or spend them before risky plays.
- Item tiers cost 800 / 1,600 / 3,200 / 6,400. Big investment spike at 4,800 in one category. Ultimate unlocks at 3,800 souls gathered. You can shop while dead; shops sit by each Guardian, in base and two secret shops.
- Hero kill bounty 200 + 50/min. Dying gives the enemy souls and costs farm time.
- Timers: easy camps 2:00; Powerups (bridge buffs, heavy melee) 5:00 then every 5 min; medium camps 5:00; Zipline Boost 6:00; Sinner's Sacrifices and hard camps 8:00; laning ends about 8:00; Soul Urn 10:00 then every 5 min; Unstable Rift about 11:00 then about every 7 min; Guardians weakest from 12:00; Walkers weakest from 18:00; waves every 25 s from 20:00.
- Midboss is alive from 0:00 (so its icon is almost always up), respawns 7/6/5 min after kills and drops the Rejuvenator (team revive buff). It is a target only for a grouped team after a won fight or with 3+ enemies dead, never in the laning phase; "Midboss is up" alone is not advice.
- A Walker gives 3,675 team souls and an extra item slot. When enemies are dead, take a structure instead of chasing.
- Leave lane only after pushing the wave; jungle between waves; trade only for a reward; back off when outnumbered or when enemies are missing.`;

// ---------------------------------------------------------------------------------------
// Rolling history (pure; timestamps are passed in)
// ---------------------------------------------------------------------------------------
export interface Sample { t: number; clock: number | null; hp: number | null; maxHp: number | null; dead: boolean; souls: number | null; unsecured: number | null; screen: string | null;
  status?: string[]; alliesNear?: number | null; enemiesNear?: number | null;
  /** Team souls difference (ally - enemy) when both were read. */
  nwDiff?: number | null }
export interface DeathRecord { t: number; clock: number | null; cause: string | null; damage: Array<{ source: string; damage: number | null }>; killingBlow: string | null;
  soulsBefore: number | null; unsecuredBefore: number | null; hpBefore: number | null; respawn: number | null;
  /** Souls read on the death screen (the death-spend check uses the larger of this and soulsBefore). */
  soulsDead?: number | null;
  /** Status effects (debuffs / crowd control) read in the last 10 s alive or on the death screen. */
  status?: string[]; alliesNear?: number | null; enemiesNear?: number | null;
  /** Game clock / wall time of the first alive sample after this death. */
  respawnedClock?: number | null; respawnedT?: number | null;
  /** Most souls held around this death, and souls spent on the death screen (peak - later read). */
  soulsPeak?: number | null; spent?: number | null; lowReads?: number }
/** A cheap good-play signal derived from the history (evidence for a short praise). */
export type GoodKind = 'window-push' | 'urn' | 'death-shop' | 'reset' | 'deathless' | 'lead';
export interface GoodPlay { t: number; clock: number | null; kind: GoodKind; key: string; text: string }
/** Praise priority when several fresh good plays compete. */
const GOOD_ORDER: GoodKind[] = ['window-push', 'urn', 'death-shop', 'reset', 'deathless', 'lead'];
/** Good plays the coach may praise (the team lead is context only). */
const PRAISABLE = (kind: GoodKind) => kind !== 'lead';
export interface MatchBoundary { t: number; clock: number | null; reason: string }
/** One row of data/abilities.json. */
export interface AbilityRow { name: string; kind?: string; heroes?: string[] }
const norm = (s: string) => s.toLowerCase().replace(/[^a-z]/g, '');
/** Damage-panel source → hero ids, from data/abilities.json (hero abilities and weapons only; OCR
 *  suffixes such as "Stalkers MarkX" still match by prefix). Generic sources (Melee, Gun Damage,
 *  troopers, structures) map to nothing. */
export function abilityIndex(rows: AbilityRow[]): (source: string) => string[] {
  const list = rows.filter(r => r && typeof r.name === 'string' && r.kind !== 'item' && Array.isArray(r.heroes) && r.heroes.length)
    .map(r => ({ key: norm(r.name), heroes: r.heroes!.filter(h => typeof h === 'string') })).filter(r => r.key.length >= 4).sort((a, b) => b.key.length - a.key.length);
  return (source: string) => {
    const s = norm(source || ''); if (s.length < 4 || /^(melee|mele|gundamage|bullet|trooper|guardian|walker|patron|shrine|midboss)/.test(s)) return [];
    const hit = list.find(r => s === r.key) || list.find(r => s.startsWith(r.key) && s.length - r.key.length <= 3) || list.find(r => r.key.startsWith(s) && s.length >= 6);
    return hit ? hit.heroes : [];
  };
}
export interface SpokenCue { t: number; clock: number | null; key: string; text: string; priority: Priority; ai: boolean }
/** AI advice that was actually delivered (used for RECENT AI ADVICE and the repeat gate). */
export interface AdviceRecord { t: number; clock: number | null; trigger: AiTrigger; topic: string; text: string;
  /** Themes the text touches (topic first) and the weakness concepts it coaches (filled by noteAdvice). */
  themes?: string[]; concepts?: string[];
  /** The profile weakness label the advice addressed (model's "weakness" field or a concept match). */
  weakness?: string | null;
  /** Trigger episode counters when it was spoken: a later, higher count is a NEW trigger. */
  episodes?: Record<string, number> }
/** A meaningful change that can trigger a periodic request (rising edge of a live trigger). */
export interface GameEvent { t: number; clock: number | null; kind: string; detail: string }

export const isDead = (s: Snapshot) => s.dead === true || s.screen === 'Dead';
const num = (v: unknown): number | null => typeof v === 'number' && Number.isFinite(v) ? v : null;
const NEAR = .12; // minimap distance counted as "near you" (same as the summary's Minimap line)
function nearCounts(s: AiSnapshot): { allies: number | null; enemies: number | null; missing: number } {
  const map = s.minimap, you = map?.you;
  const heroes = (map?.heroes || []).filter(h => h.hits >= 2);
  const missing = heroes.filter(h => !h.ally && h.unseen_ms > 0).length;
  if (!you || !map?.heroes) return { allies: null, enemies: null, missing };
  const near = (ally: boolean) => heroes.filter(h => h.ally === ally && h.unseen_ms === 0 && Math.hypot(h.pos.x - you.x, h.pos.y - you.y) < NEAR).length;
  return { allies: near(true), enemies: near(false), missing };
}
const str = (v: unknown, max = 80): string | null => typeof v === 'string' && v.trim() ? v.trim().replace(/\s+/g, ' ').slice(0, max) : null;
function describe(v: unknown): string | null {
  if (typeof v === 'string') return str(v);
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>;
    const who = str(o.source) || str(o.name) || str(o.hero) || str(o.ability);
    const dmg = num(o.damage);
    return who ? `${who}${dmg != null ? ` (${Math.round(dmg)})` : ''}` : null;
  }
  return null;
}

const statusOf = (s: AiSnapshot) => Array.isArray(s.status_effects) ? s.status_effects.slice(0, 6).map(x => String(x).slice(0, 30)) : [];
export class AiHistory {
  samples: Sample[] = []; deaths: DeathRecord[] = []; spoken: SpokenCue[] = []; advice: AdviceRecord[] = [];
  deathCount = 0;
  /** Last hero read (kept across the death screen). */
  hero: string | null = null;
  /** Display names for hero ids in trigger details (the controller sets the app's names). */
  name: (id: string) => string = defaultName;
  /** Live triggers active in the latest sample, and how many times each became active (episodes). */
  active: string[] = []; episodes: Record<string, number> = {};
  /** Rising edges of the cadence triggers (newest last, capped). */
  events: GameEvent[] = [];
  /** Good-play signals of this match (newest last) and the ones already praised (keys). */
  good: GoodPlay[] = []; praised: string[] = [];
  /** Hero ids read for each team this match, with how many samples read each. */
  roster: { enemies: Record<string, number>; allies: Record<string, number> } = { enemies: {}, allies: {} };
  /** Last game clock read and its wall time (kept across clock-less menu and death frames). */
  lastClock: number | null = null; lastClockT: number | null = null;
  /** Matches seen by this history (a boundary increments it) and why each boundary was drawn. */
  match = 0; boundaries: MatchBoundary[] = [];
  /** Damage-panel source → hero ids (set from data/abilities.json; empty until loaded). */
  heroesOf: (source: string) => string[] = () => [];
  private ended = false; private heroPending: { hero: string; n: number } | null = null;
  private g = this.freshGood();
  private freshGood() {
    return { win: null as null | { max: number; used: boolean }, carry: null as null | { t: number; clock: number | null; souls: number | null },
      low: null as null | { clock: number | null; t: number; hp: number }, streakFrom: null as number | null, streakMarks: 0, lastLead: null as number | null };
  }
  setAbilities(rows: AbilityRow[] | null | undefined) { if (Array.isArray(rows)) this.heroesOf = abilityIndex(rows); }
  reset() {
    this.samples = []; this.deaths = []; this.spoken = []; this.advice = []; this.deathCount = 0; this.hero = null; this.active = []; this.episodes = {}; this.events = [];
    this.good = []; this.praised = []; this.roster = { enemies: {}, allies: {} }; this.lastClock = null; this.lastClockT = null; this.ended = false; this.heroPending = null; this.g = this.freshGood();
  }
  /** Why `s` starts a new match (null = same match). Pure apart from the post-game flag and the
   *  pending hero switch it keeps. */
  private boundary(s: AiSnapshot, now: number, clock: number | null): string | null {
    const screen = typeof s.screen === 'string' ? s.screen : null;
    if (resultOf(screen) != null) { if (this.samples.length) this.ended = true; return null; }
    if (!this.samples.length) return null;
    const gap = this.lastClockT != null ? (now - this.lastClockT) / 1000 : null;
    if (this.ended && clock != null && screen === 'Gameplay') return 'a post-game screen, then gameplay again';
    if (clock != null && this.lastClock != null) {
      // Restart: compared with the last clock READ, so the clock-less menus between two matches
      // do not hide it (rec12: 37:22 of the Violet match, menus, then 1:07 of the Grey Talon match).
      if (clock + 30 < this.lastClock) return `the clock restarted (${clockText(this.lastClock)} → ${clockText(clock)})`;
      if (gap != null && gap >= LONG_GAP_SECS && clock > this.lastClock + gap + 60) return `the clock jumped from ${clockText(this.lastClock)} to ${clockText(clock)} during a ${Math.round(gap)} s gap`;
    }
    const hero = typeof s.hero === 'string' && s.hero ? s.hero : null;
    if (hero && this.hero && hero !== this.hero && screen === 'Gameplay') {
      if (gap != null && gap >= LONG_GAP_SECS) return `the hero changed (${defaultName(this.hero)} → ${defaultName(hero)}) after a ${Math.round(gap)} s gap`;
      this.heroPending = this.heroPending?.hero === hero ? { hero, n: this.heroPending.n + 1 } : { hero, n: 1 };
      if (this.heroPending.n >= HERO_CONFIRM) {
        // A hero switch with a continuous clock is a portrait misread early on, not a new match.
        const continuous = clock != null && this.lastClock != null && clock >= this.lastClock - 5;
        if (!continuous) return `the hero changed (${defaultName(this.hero)} → ${defaultName(hero)})`;
        this.hero = hero; this.heroPending = null;
      }
    } else if (hero && hero === this.hero) this.heroPending = null;
    return null;
  }
  push(s: AiSnapshot, cues: Cue[], now: number) {
    this.pushSample(s, cues, now);
    // Live triggers and their rising edges (theme memory and cadence). Unknown/dead frames keep the
    // previous set so a death-screen blink does not count as a new episode.
    const clock = num(s.game_clock_secs);
    if (isDead(s) || this.samples.at(-1)?.dead) {
      this.active = this.active.filter(k => k === 'souls-tier' || k.startsWith('phase:'));
      return;
    }
    if (s.screen != null && s.screen !== 'Gameplay') return;
    const now_ = liveTriggers(s, this, now);
    for (const [key, detail] of now_) if (!this.active.includes(key)) {
      this.episodes[key] = (this.episodes[key] || 0) + 1;
      if (CADENCE_TRIGGERS.some(p => key === p || key.startsWith(`${p}:`))) {
        // The first phase seen is the starting state, not a change.
        if (!(key.startsWith('phase:') && !this.active.some(k => k.startsWith('phase:')))) this.events.push({ t: now, clock, kind: key.replace(/:.*/, ''), detail });
      }
    }
    if (this.events.length > 40) this.events.splice(0, this.events.length - 40);
    this.active = [...now_.keys()];
  }
  private pushSample(s: AiSnapshot, cues: Cue[], now: number) {
    const clock = num(s.game_clock_secs);
    // A new match (clock restart, post-game, hero change, long gap): forget the old one entirely.
    const why = this.boundary(s, now, clock);
    if (why) { this.reset(); this.match++; this.boundaries.push({ t: now, clock, reason: why }); if (this.boundaries.length > 10) this.boundaries.shift(); }
    if (clock != null) { this.lastClock = clock; this.lastClockT = now; }
    const last = this.samples.at(-1);
    // The shop or scoreboard opened from the death screen (no health bar) is still the same death,
    // not a respawn followed by a second death.
    const dead = isDead(s) || (!!last?.dead && s.screen != null && s.screen !== 'Gameplay' && num(s.hp) == null);
    const near = nearCounts(s);
    if (typeof s.hero === 'string' && s.hero && !this.hero) this.hero = s.hero;
    else if (typeof s.hero === 'string' && s.hero === this.hero) this.heroPending = null;
    // This match's roster (the death screen and menus hide it; stale first frames are outvoted).
    const vote = (ids: Array<string | null | undefined>, votes: Record<string, number>) => {
      for (const id of new Set(ids)) if (typeof id === 'string' && /^hero_[a-z_]+$/.test(id) && (votes[id] != null || Object.keys(votes).length < 30)) votes[id] = (votes[id] || 0) + 1;
    };
    vote([...(s.enemy_heroes || []), ...(s.hero_status || []).filter(p => !p.ally).map(p => p.hero)], this.roster.enemies);
    vote([...(s.ally_heroes || []), ...(s.hero_status || []).filter(p => p.ally).map(p => p.hero)], this.roster.allies);
    this.samples.push({ t: now, clock, hp: num(s.hp), maxHp: num(s.max_hp), dead, souls: num(s.souls), unsecured: num(s.unsecured_souls), screen: str(s.screen, 20),
      status: statusOf(s), alliesNear: near.allies, enemiesNear: near.enemies,
      nwDiff: num(s.ally_networth) != null && num(s.enemy_networth) != null ? s.ally_networth! - s.enemy_networth! : null });
    if (this.samples.length > 400) this.samples.splice(0, this.samples.length - 400);
    if (dead && (!last || !last.dead)) {
      const alive = [...this.samples].reverse().find(x => !x.dead);
      // Debuffs and who was around in the last 10 s alive (game clock; wall time without one).
      const before = this.samples.filter(x => !x.dead && (clock != null && x.clock != null ? clock - x.clock <= 10 : now - x.t <= 10000));
      this.deathCount++;
      this.deaths.push({ t: now, clock, cause: null, damage: [], killingBlow: null, soulsBefore: alive?.souls ?? null, unsecuredBefore: alive?.unsecured ?? null,
        hpBefore: alive?.hp != null && alive.maxHp ? Math.round(alive.hp / alive.maxHp * 100) : null, respawn: null, soulsDead: null,
        status: [...new Set(before.flatMap(x => x.status || []))].slice(0, 6), alliesNear: alive?.alliesNear ?? null, enemiesNear: alive?.enemiesNear ?? null,
        respawnedClock: null, respawnedT: null, soulsPeak: alive?.souls ?? null, spent: null, lowReads: 0 });
      if (this.deaths.length > 20) this.deaths.shift();
      this.g.carry = null; this.g.low = null; this.g.streakFrom = null; this.g.streakMarks = 0;
    }
    if (!dead && last?.dead) {
      const d = this.deaths.at(-1);
      if (d && d.respawnedT == null) {
        d.respawnedClock = clock; d.respawnedT = now; this.g.streakFrom = clock;
        // Souls spent on the death screen, confirmed by the first alive read.
        const souls = num(s.souls);
        if (souls != null && d.soulsPeak != null) this.noteShop(d, d.soulsPeak - souls, now, clock);
      }
    }
    const death = dead ? this.deaths.at(-1) : undefined;
    if (death) {
      death.cause = str(s.death_cause) ?? death.cause;
      death.respawn = num(s.respawn_secs) ?? death.respawn;
      death.soulsDead = num(s.souls) ?? death.soulsDead ?? null;
      const dmg = Array.isArray(s.death_damage) ? s.death_damage.map(d => ({ source: str(d?.source, 40) || 'unknown', damage: num(d?.damage) })).slice(0, 6) : [];
      if (dmg.length) death.damage = dmg;
      death.killingBlow = describe(s.death_killing_blow) ?? death.killingBlow;
      const st = statusOf(s); if (st.length) death.status = [...new Set([...(death.status || []), ...st])].slice(0, 6);
      // Shopping on the death screen: two reads at least the death-spend threshold below the peak.
      const souls = num(s.souls);
      if (souls != null) {
        death.soulsPeak = Math.max(death.soulsPeak ?? souls, souls);
        death.lowReads = death.soulsPeak - souls >= Math.max(800, deathSpend(death.clock)) ? (death.lowReads || 0) + 1 : 0;
        if ((death.lowReads || 0) >= 2) this.noteShop(death, death.soulsPeak - souls, now, clock);
      }
    }
    if (!dead) this.detectGood(s, now, clock, near);
    for (const c of cues) {
      this.spoken.push({ t: now, clock, key: c.key, text: c.text.slice(0, 200), priority: c.priority, ai: c.key.startsWith('ai:') });
    }
    if (this.spoken.length > 60) this.spoken.splice(0, this.spoken.length - 60);
  }
  private noteShop(d: DeathRecord, spent: number, now: number, clock: number | null) {
    if (spent < Math.max(800, deathSpend(d.clock)) || (d.spent != null && spent <= d.spent)) return;
    d.spent = spent;
    const key = `death-shop:${d.clock ?? d.t}`, text = `spent about ${fmt(Math.round(spent / 100) * 100)} souls on the death screen after the ${clockText(d.clock)} death (walks out stronger instead of holding them)`;
    // More shopping on the same death screen updates the signal (one event per death).
    const known = this.good.find(g => g.key === key);
    if (known) known.text = text; else this.addGood(now, clock, 'death-shop', text, key);
  }
  private addGood(now: number, clock: number | null, kind: GoodKind, text: string, key = `${kind}:${clock ?? now}`) {
    if (this.good.some(g => g.key === key)) return;
    this.good.push({ t: now, clock, kind, key, text });
    if (this.good.length > 30) this.good.shift();
    // A growing team lead is context for the summary, not the player's own play: no event, no praise.
    if (!PRAISABLE(kind)) return;
    this.events.push({ t: now, clock, kind: 'good', detail: text });
    if (this.events.length > 40) this.events.splice(0, this.events.length - 40);
  }
  /** Good-play signals from an alive sample (cheap, incremental). */
  private detectGood(s: AiSnapshot, now: number, clock: number | null, near: { allies: number | null }) {
    const g = this.g, status = s.hero_status || [];
    if (clock != null && g.streakFrom == null && !this.deaths.length) g.streakFrom = clock;
    // 1. An enemy-dead window used to push: in the enemy base, at an enemy objective, or far up a lane.
    const enemyDead = status.length >= 6 ? status.filter(p => !p.ally && p.dead === true).length : null;
    if (enemyDead != null && enemyDead >= 2) {
      g.win = { max: Math.max(g.win?.max ?? 0, enemyDead), used: g.win?.used ?? false };
      const zone = s.minimap?.zone_area;
      const banner = s.objective_banner && s.objective_banner.own === false ? s.objective_banner.name : null;
      const deep = zone === 'EnemyBase' || !!banner || (num(s.minimap?.your_progress) ?? 0) >= .7;
      // One praise per push: a window that closes and reopens within 60 s is the same push.
      const lastPush = this.good.filter(x => x.kind === 'window-push').at(-1);
      const recentPush = !!lastPush && (clock != null && lastPush.clock != null ? clock - lastPush.clock < 60 : now - lastPush.t < 60000);
      if (deep && !g.win.used && recentPush) g.win.used = true;
      if (deep && !g.win.used && s.screen === 'Gameplay') {
        g.win.used = true;
        const where = banner ? `hit ${String(banner).slice(0, 30)}` : zone === 'EnemyBase' ? 'pushed into the enemy base' : `pushed ${s.minimap?.your_lane ? `${s.minimap.your_lane} lane` : 'the lane'} deep`;
        this.addGood(now, clock, 'window-push', `${where} at ${clockText(clock)} while ${g.win.max} enemies were dead${(near.allies ?? 0) >= 2 ? ` (grouped with ${near.allies} allies)` : ''}: a dead-enemy window turned into structure pressure`);
      }
    } else if (enemyDead != null) g.win = null;
    // 2. The Soul Urn delivered: carried by you, then a souls jump of 1,000+ within 25 s.
    const carrying = s.minimap?.urn === 'CarriedByYou' || statusOf(s).some(x => /carrying_urn/i.test(x));
    const souls = num(s.souls);
    if (carrying) g.carry = { t: now, clock, souls: souls ?? g.carry?.souls ?? null };
    else if (g.carry) {
      const secs = clock != null && g.carry.clock != null ? clock - g.carry.clock : (now - g.carry.t) / 1000;
      if (secs > 25) g.carry = null;
      else if (souls != null && g.carry.souls != null && souls - g.carry.souls >= 1000) {
        this.addGood(now, clock, 'urn', `delivered the Soul Urn at ${clockText(clock)} (+${fmt(souls - g.carry.souls)} souls in ${Math.round(secs)} s)`); g.carry = null;
      }
    }
    // 3. A low-health reset: under 35% health, then back to 90%+ without dying.
    const hp = num(s.hp) != null && num(s.max_hp) ? s.hp! / s.max_hp! : null;
    if (hp != null && hp < .35 && !g.low) g.low = { clock, t: now, hp };
    else if (g.low && hp != null && hp >= .9) {
      const secs = clock != null && g.low.clock != null ? clock - g.low.clock : (now - g.low.t) / 1000;
      if (secs <= 150) this.addGood(now, clock, 'reset', `got out at ${Math.round(g.low.hp * 100)}% health at ${clockText(g.low.clock)} and came back full without dying`);
      g.low = null;
    }
    // 4. A deathless stretch (7 minutes, then every 5).
    if (clock != null && g.streakFrom != null) {
      const secs = clock - g.streakFrom;
      const mark = secs >= DEATHLESS_FIRST_SECS ? 1 + Math.floor((secs - DEATHLESS_FIRST_SECS) / DEATHLESS_STEP_SECS) : 0;
      if (mark > g.streakMarks) { g.streakMarks = mark; this.addGood(now, clock, 'deathless', `no deaths for ${Math.floor(secs / 60)} minutes (since ${clockText(g.streakFrom)})${this.deaths.length ? '' : ' this match'}`); }
    }
    // 5. The team's souls lead grew by 5,000+ (or 5% of the team average) in about 3 minutes.
    const d = this.samples.at(-1)?.nwDiff;
    if (clock != null && d != null && d > 0 && (g.lastLead == null || clock - g.lastLead >= 240)) {
      const past = this.samples.filter(x => x.nwDiff != null && x.clock != null && x.clock <= clock - 150 && x.clock >= clock - 240).at(-1);
      const avg = num(s.ally_networth) != null && num(s.enemy_networth) != null ? (s.ally_networth! + s.enemy_networth!) / 2 : 0;
      if (past?.nwDiff != null && d - past.nwDiff >= Math.max(5000, .05 * avg)) {
        g.lastLead = clock;
        this.addGood(now, clock, 'lead', `your team's souls lead grew from ${past.nwDiff >= 0 ? '+' : ''}${fmt(past.nwDiff)} at ${clockText(past.clock)} to +${fmt(d)} at ${clockText(clock)}`);
      }
    }
  }
  /** The current match's roster (most-read heroes; stale reads of the previous match drop out). */
  rosterOf(): { enemies: string[]; allies: string[] } {
    const top = (votes: Record<string, number>) => {
      const xs = Object.entries(votes).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])); const max = xs[0]?.[1] ?? 0;
      return xs.filter(([, n]) => n >= Math.max(1, max * .2)).slice(0, 6).map(([id]) => id);
    };
    return { enemies: top(this.roster.enemies), allies: top(this.roster.allies) };
  }
  /** The rule coach's death reviews of `d` itself: spoken after it died (wall time) and within its
   *  clock window, never one of an earlier death or match. */
  reviewsFor(d: DeathRecord, keys = /^(death:review|active:(close|repeat|cc|debuff))/): SpokenCue[] {
    return this.spoken.filter(c => {
      if (c.ai || !keys.test(c.key) || c.t < d.t - 3000) return false;
      if (d.respawnedT != null && c.t > d.respawnedT + 5000) return false;
      const at = /^death:review:(\d+)/.exec(c.key)?.[1];
      if (at != null && d.clock != null && Math.abs(Number(at) - d.clock) > 40) return false;
      return d.clock == null || c.clock == null || (c.clock >= d.clock - 10 && c.clock <= d.clock + 150);
    });
  }
  /** Last game clock read (the death screen and menus hide it). */
  get clock(): number | null { return this.samples.at(-1)?.clock ?? this.lastClock; }
  /** Record AI advice that was delivered (themes, concepts and trigger episodes are filled in). */
  noteAdvice(rec: AdviceRecord) {
    const themes = rec.themes ?? [...new Set([rec.topic, ...themesIn(rec.text)])];
    this.advice.push({ ...rec, text: rec.text.slice(0, 200), themes, concepts: rec.concepts ?? conceptsIn(rec.text), weakness: rec.weakness ?? null, episodes: rec.episodes ?? { ...this.episodes } });
    if (this.advice.length > 20) this.advice.shift();
  }
  /** The last death, when the player is alive again and respawned at most `secs` ago. */
  respawned(now: number, secs = RESPAWN_SECS): { death: DeathRecord; ago: number } | null {
    const d = this.deaths.at(-1), last = this.samples.at(-1);
    if (!d || !last || last.dead || d.respawnedT == null) return null;
    const ago = last.clock != null && d.respawnedClock != null ? last.clock - d.respawnedClock : Math.round((now - d.respawnedT) / 1000);
    return ago >= 0 && ago <= secs ? { death: d, ago } : null;
  }
  /** Items within the last `secs` of game time (wall time when no clock is known). */
  recent<T extends { t: number; clock: number | null }>(items: T[], now: number, secs = HISTORY_WINDOW_SECS): T[] {
    const clock = this.samples.at(-1)?.clock ?? null;
    return items.filter(x => clock != null && x.clock != null ? x.clock <= clock && clock - x.clock <= secs : now - x.t <= secs * 1000);
  }
}

// ---------------------------------------------------------------------------------------
// State summary (pure, deterministic)
// ---------------------------------------------------------------------------------------
export const clockText = (n?: number | null) => n == null ? '?:??' : `${Math.floor(n / 60)}:${String(Math.floor(n % 60)).padStart(2, '0')}`;
export const defaultName = (id: string) => id.replace(/^(hero_|upgrade_|item_)/, '').replaceAll('_', ' ');
const fmt = (n: number) => Math.round(n).toLocaleString('en-US');

// ---------------------------------------------------------------------------------------
// Souls in the context of the item tier normally bought at this game time
// ---------------------------------------------------------------------------------------
/** spendTarget/deathSpend come from player-profile.ts, which mirrors crates/engine/src/economy.rs:
 *  the usual buy is 1,600 before 10:00, 3,200 until 20:00 and 6,400 after; a death is worth a souls
 *  remark from half that (at least 1,200). */
export interface SoulsView {
  souls: number | null; clock: number | null; dead: boolean;
  /** Usual buy now (alive) / death-spend threshold (dead). */
  target: number; usual: number;
  allowed: boolean; reason: string;
  /** Game clock of the rule coach's last souls reminder within SOULS_RULE_WINDOW_SECS. */
  ruleAt: number | null;
}
export function soulsView(s: AiSnapshot, history: AiHistory, now: number): SoulsView {
  const clock = num(s.game_clock_secs), dead = isDead(s) || !!history.samples.at(-1)?.dead, usual = spendTarget(clock);
  if (dead) {
    const d = history.deaths.at(-1);
    // What can still be spent: the souls read now. A death-screen purchase lowers it (rec12 died
    // with 6,073, spent down to 132, and was still told to spend 6,073). Before a read on the death
    // screen, the larger of the last reads before dying.
    const before = [d?.soulsDead ?? null, d?.soulsBefore ?? null].filter((v): v is number => v != null);
    const souls = num(s.souls) ?? (d?.spent != null ? d.soulsDead ?? null : null) ?? (before.length ? Math.max(...before) : null), need = deathSpend(clock ?? d?.clock ?? null);
    const allowed = souls != null && souls >= need;
    const spent = d?.spent != null ? ` (already spent about ${fmt(Math.round(d.spent / 100) * 100)} on this death screen)` : '';
    return { souls, clock, dead, target: need, usual, allowed, ruleAt: null,
      reason: souls == null ? 'souls not read' : allowed ? `${fmt(souls)} reaches the ${fmt(need)} death-spend threshold` : `${fmt(souls)} is below the ${fmt(need)} death-spend threshold${spent || ': not a souls death'}` };
  }
  const souls = num(s.souls);
  const rule = history.recent(history.spoken, now, SOULS_RULE_WINDOW_SECS).filter(c => !c.ai && /^unspent/.test(c.key)).at(-1);
  const ruleAt = rule ? rule.clock : null;
  const affordable = souls != null && souls >= usual;
  const allowed = affordable && !rule;
  return { souls, clock, dead, target: usual, usual, allowed, ruleAt,
    reason: souls == null ? 'souls not read' : !affordable ? `saving for the usual ${fmt(usual)} buy` : rule ? `the rule coach already said it${rule.clock != null ? ` at ${clockText(rule.clock)}` : ''}` : `the usual ${fmt(usual)} buy is affordable` };
}
function soulsLine(v: SoulsView, s: AiSnapshot, held: string): string {
  const gate = ` · Souls advice: ${v.allowed ? 'allowed' : `not allowed (${v.reason})`}`;
  if (v.souls == null) return `Souls unknown${gate}`;
  const unsecured = num(s.unsecured_souls) != null ? ` (unsecured ${fmt(s.unsecured_souls!)})` : '';
  if (v.dead) return `Souls ${fmt(v.souls)}${unsecured} on the death screen (${v.allowed ? `≥ the ${fmt(v.target)} death-spend threshold: buy before respawning` : `below the ${fmt(v.target)} death-spend threshold, half the usual ${fmt(v.usual)} buy`})${gate}`;
  if (v.souls >= v.usual) return `Unspent souls ${fmt(v.souls)}${unsecured}${held} · can afford the usual ${fmt(v.usual)} buy for ${clockText(v.clock)}${gate}`;
  return `Souls ${fmt(v.souls)}${unsecured} (saving: the usual buy now is a ${fmt(v.usual)} item; ${fmt(v.usual - v.souls)} short)${gate}`;
}

// ---------------------------------------------------------------------------------------
// Coaching angles: live situations that match the player's known weaknesses
// ---------------------------------------------------------------------------------------
/** Melee / close-range divers (coach heuristic from docs/strategy/hero-threats.md and the hero
 *  pages: Abrams, Shiv, Yamato, Mo & Krill, Calico, Lash, Victor, Drifter, Billy). */
export const CLOSE_RANGE_HEROES = new Set(['hero_atlas', 'hero_shiv', 'hero_yamato', 'hero_krill', 'hero_nano', 'hero_lash', 'hero_frank', 'hero_drifter', 'hero_punkgoat']);
/** Long-range heroes (same list as player-profile.ts LONG_RANGE). */
const LONG_RANGE_HEROES = new Set(['hero_orion', 'hero_hornet']);
/** Kit roles for the heroes we are sure of (coach heuristic from the hero pages and
 *  docs/strategy/hero-threats.md). Heroes missing here are "role not in the coach's table": the
 *  model is told not to guess their kit. */
export const HERO_ROLES: Record<string, string> = {
  hero_atlas: 'melee brawler and initiator', hero_shiv: 'melee diver', hero_yamato: 'melee diver', hero_krill: 'melee tank and initiator',
  hero_nano: 'close-range diver', hero_lash: 'diving initiator', hero_frank: 'close-range brawler with a damage aura', hero_drifter: 'melee assassin who hunts isolated targets',
  hero_punkgoat: 'melee brawler', hero_orion: 'long-range marksman', hero_hornet: 'long-range sniper', hero_haze: 'mid-range gun assassin',
  hero_wraith: 'mid-range gunner', hero_inferno: 'mid-range damage-over-time carry', hero_bebop: 'mid-range pick hero (hook)', hero_dynamo: 'support and teamfight initiator',
  hero_kelvin: 'support and healer', hero_tengu: 'support and flexible caster', hero_forge: 'ranged turret and zone controller', hero_viscous: 'mid-range initiator',
  hero_warden: 'mid-range brawler', hero_ghost: 'close-range spirit brawler', hero_chrono: 'mid-range pick hero', hero_gigawatt: 'mid-range spirit damage',
  hero_magician: 'mid-range spirit caster', hero_mirage: 'mid-range gunner', hero_synth: 'mid-range spirit diver', hero_astro: 'mid-range gunner',
  hero_doorman: 'mid-range controller', hero_necro: 'ranged spirit caster with summons',
};
export const phaseOf = (clock: number) => clock < 480 ? 'laning' : clock < 1200 ? 'midgame' : 'lategame';
/** Major objective moments worth a fresh look (rule timers cover the rest). */
const MAJOR_OBJECTIVES: Array<[number, string]> = [[300, 'Powerups and medium camps spawn'], [480, "Sinner's Sacrifices and hard camps spawn (laning ends)"], [660, 'first Unstable Rift (about)'],
  [720, 'Guardians reach minimum resist'], [1080, 'Walkers reach minimum resist'], ...Array.from({ length: 11 }, (_, i) => [600 + i * 300, 'Soul Urn spawns'] as [number, string])];
/** The next major objective within `horizon` seconds, as [time, text]. */
export function objectiveSoon(clock: number, horizon = 60): [number, string] | null {
  const next = MAJOR_OBJECTIVES.filter(([t]) => t > clock && t - clock <= horizon).sort((a, b) => a[0] - b[0])[0];
  return next ? [next[0], `${next[1]} at ${clockText(next[0])} (in ${Math.round(next[0] - clock)}s)`] : null;
}
/** Live triggers that start a periodic request when they become active (rising edge). */
export const CADENCE_TRIGGERS = ['phase', 'respawn', 'enemy-window', 'objective-soon', 'souls-tier'];
/** The concrete live situations (key → detail) the theme memory and the cadence react to. Keys with
 *  a ":suffix" are separate episodes (each objective, each phase). Pure. */
export function liveTriggers(s: AiSnapshot, history: AiHistory, now: number): Map<string, string> {
  const out = new Map<string, string>(), name = history.name;
  const clock = num(s.game_clock_secs);
  if (clock != null) out.set(`phase:${phaseOf(clock)}`, `${phaseOf(clock)} phase started at ${clockText(clock)}`);
  const near = nearCounts(s), status = s.hero_status || [];
  const hp = num(s.hp) != null && num(s.max_hp) ? Math.round(s.hp! / s.max_hp! * 100) : null;
  if (hp != null && hp < 50) out.set('hp-low', `your health is ${hp}%`);
  const back = history.respawned(now); if (back) out.set('respawn', `you respawned ${back.ago}s ago`);
  const label = (p: { hero?: string | null; slot: number }, ally: boolean) => p.hero ? name(p.hero) : `${ally ? 'ally' : 'enemy'} ${p.slot + 1}`;
  const enemyDead = status.filter(p => !p.ally && p.dead === true), allyDead = status.filter(p => p.ally && p.dead === true);
  if (enemyDead.length >= 2) {
    const secs = enemyDead.map(p => p.respawn_secs).filter((v): v is number => typeof v === 'number');
    out.set('enemy-window', `${enemyDead.length} enemies dead (${enemyDead.map(p => label(p, false)).join(', ')})${secs.length ? `, first back in ${Math.min(...secs)}s` : ''}`);
  }
  if (allyDead.length >= 2 && allyDead.length > enemyDead.length) out.set('allies-down', `${allyDead.length} allies dead (${allyDead.map(p => label(p, true)).join(', ')}) vs ${enemyDead.length} enemies`);
  const deadEnemy = new Set(enemyDead.map(p => p.hero).filter(Boolean));
  const roster = s.enemy_heroes?.length ? s.enemy_heroes : status.filter(p => !p.ally && p.hero).map(p => p.hero!);
  const divers = [...new Set(roster)].filter(h => CLOSE_RANGE_HEROES.has(h) && !deadEnemy.has(h));
  if (near.enemies) {
    out.set('enemy-near', `${near.enemies} enemy hero${near.enemies > 1 ? 'es' : ''} near you on the minimap`);
    if (divers.length) out.set('diver-near', `${near.enemies} enemy near you with close-range divers alive (${divers.slice(0, 3).map(name).join(', ')})`);
  }
  if (near.allies === 0 && near.missing >= 2) out.set('isolated', `no ally near you and ${near.missing} enemies missing`);
  if (near.missing >= 3) out.set('missing', `${near.missing} enemies missing from the minimap`);
  if (clock != null) { const o = objectiveSoon(clock); if (o) out.set(`objective-soon:${o[0]}`, o[1]); }
  const souls = num(s.souls);
  if (souls != null && souls >= spendTarget(clock)) out.set('souls-tier', `${fmt(souls)} souls reach the usual ${fmt(spendTarget(clock))} buy`);
  const st = statusOf(s); if (st.length) out.set('debuff', `debuff on you: ${st.slice(0, 3).join(', ')}`);
  const low = status.filter(p => !p.ally && p.dead !== true && p.bar_visible !== false && num(p.health_ratio) != null && p.health_ratio! < .3);
  if (low.length) out.set('enemy-low', `${low.map(p => `${label(p, false)} ${Math.round(p.health_ratio! * 100)}%`).join(', ')} low`);
  return out;
}
/** Profile weakness labels (player-profile.ts traits) that each live situation can coach. */
const PROFILE_MATCH: Record<string, RegExp> = {
  respawn: /Dies soon after respawning|Dies alone/i, hurt: /Goes back in hurt|Stays in at low health|Gets burst down/i,
  close: /Killed at close range|Same killer again|melee/i, window: /Enemy-dead windows unused/i,
  alone: /Dies alone|Pushes in with enemies missing/i, lost: /Stays in lost fights/i, cc: /crowd control|stun|debuff|CC\b/i,
};
/** The profile weakness (its label, e.g. "Killed at close range") whose text matches `re`. The
 *  profile text reads "Player profile (...). Weak: Label: evidence; Label: evidence. Strong: ... Focus: ...". */
function profileLabel(profile: string | null | undefined, re: RegExp): string | null {
  if (!profile) return null;
  const weak = profile.replace(/^[\s\S]*?\bWeak:\s*/, '').split(/\b(?:Strong|Focus):/)[0];
  for (const seg of weak.split(/;\s*|\.\s+(?=[A-Z])/)) {
    if (!re.test(seg)) continue;
    const label = seg.split(':')[0].trim();
    return (label.length <= 40 ? label : seg.slice(0, 40)).trim() || null;
  }
  return null;
}
export interface AngleInput { snapshot: AiSnapshot; history: AiHistory; now: number; profile?: string | null; name?: (id: string) => string }
/** Up to 4 "situation → what a coach would say" lines; ones matching a profile weakness first.
 *  An angle whose concept was coached in the last 5 min (same trigger episode) is left out, so the
 *  model is not invited to restate it. */
export function coachingAngles({ snapshot: s, history, now, profile, name = defaultName }: AngleInput): string[] {
  if (isDead(s)) return [];
  const out: Array<{ match: boolean; text: string }> = [];
  const add = (key: keyof typeof PROFILE_MATCH, concept: string | null, text: string) => {
    if (concept && conceptCoached(concept, history, now)) return;
    const label = profileLabel(profile, PROFILE_MATCH[key]);
    out.push({ match: !!label, text: `${text}${label ? ` [profile: ${label}]` : ''}` });
  };
  const near = nearCounts(s), status = s.hero_status || [];
  const hp = num(s.hp) != null && num(s.max_hp) ? Math.round(s.hp! / s.max_hp! * 100) : null;
  const hero = s.hero ? name(s.hero) : 'you';
  const longRange = !!s.hero && LONG_RANGE_HEROES.has(s.hero);
  const back = history.respawned(now);
  if (back) add('respawn', 'alone', `Respawned ${back.ago}s ago, ${near.allies ?? 'unknown'} allies near you → walk out with a teammate or catch the wave by your Guardian; do not cross the map alone`);
  const delta = num(s.health_delta_ratio);
  if (hp != null && hp < 60 && !(delta != null && delta > .05)) add('hurt', 'hurt', `Health ${hp}% and not recovering → heal to full before the next fight (shopping does not heal) and leave on the first big hit`);
  const deadEnemy = new Set(status.filter(p => !p.ally && p.dead === true && p.hero).map(p => p.hero!));
  const roster = s.enemy_heroes?.length ? s.enemy_heroes : status.filter(p => !p.ally && p.hero).map(p => p.hero!);
  const divers = [...new Set(roster)].filter(h => CLOSE_RANGE_HEROES.has(h) && !deadEnemy.has(h));
  const lastDeath = history.deaths.at(-1);
  const closeDeath = !!lastDeath && /mele|aura|stalker|slash|rend/i.test(`${lastDeath.cause || ''} ${lastDeath.damage.map(d => d.source).join(' ')}`);
  // Only a live reason (an enemy near you, or walking out after a close-range death) — a diver
  // merely being alive is true all game and made the old coach repeat "max range" every call.
  if (divers.length && ((near.enemies ?? 0) > 0 || (closeDeath && back)))
    add('close', 'close', `Close-range divers alive: ${divers.slice(0, 3).map(name).join(', ')}${near.enemies ? ` (${near.enemies} enemy near you)` : ''}${closeDeath ? '; your last death was at close range' : ''} → ${longRange ? `${hero} is long-range: ` : ''}hold max range behind your front line and save your escape for their dive`);
  const enemyDead = status.filter(p => !p.ally && p.dead === true), allyDead = status.filter(p => p.ally && p.dead === true);
  if (enemyDead.length >= 2) {
    const secs = enemyDead.map(p => p.respawn_secs).filter((v): v is number => typeof v === 'number');
    const lanes = (['green', 'blue', 'yellow'] as const).map(k => ({ k, l: s.minimap?.[k] })).filter(x => x.l?.push != null).sort((a, b) => b.l!.push! - a.l!.push!);
    const furthest = lanes[0] ? `${lanes[0].k[0].toUpperCase()}${lanes[0].k.slice(1)} lane (front ${Math.round(lanes[0].l!.push! * 100)}% up)` : 'the furthest pushed lane';
    add('window', null, `${enemyDead.length} enemies dead (${enemyDead.map(p => p.hero ? name(p.hero) : `enemy ${p.slot + 1}`).join(', ')}${secs.length ? `, first back in ${Math.min(...secs)}s` : ''}) → group up and hit the Guardian or Walker in ${furthest} now instead of farming your own half`);
  }
  if (allyDead.length >= 2 && allyDead.length > enemyDead.length) add('lost', 'fallback', `${allyDead.length} allies dead vs ${enemyDead.length} enemies → the fight is lost: back off together toward your Guardian`);
  else if (near.allies === 0 && near.missing >= 2) add('alone', 'missing', `No ally near you and ${near.missing} enemies missing → move toward a teammate before the next trade`);
  // A low enemy is a target, not a threat (rec11: "Calico is at 18%" was turned into a warning).
  const low = status.filter(p => !p.ally && p.dead !== true && p.bar_visible !== false && p.hero && num(p.health_ratio) != null && p.health_ratio! < .3);
  if (low.length && (hp == null || hp >= 50)) add('window', null, `${low.map(p => `${name(p.hero!)} at ${Math.round(p.health_ratio! * 100)}% HP`).join(', ')} and visible → a kill chance if they are in your range with an ally; a low enemy is a target, not a threat`);
  const st = statusOf(s);
  if (st.length) add('cc', 'cc', st.some(x => /stamina/i.test(x)) ? 'Out of stamina: no dash to escape → do not start a trade until it recovers' : `Debuff on you: ${st.slice(0, 3).join(', ')} → disengage or cleanse before you trade`);
  return out.sort((a, b) => Number(b.match) - Number(a.match)).slice(0, 4).map(a => `- ${a.text}`);
}

// ---------------------------------------------------------------------------------------
// Themes, weakness concepts, near-duplicates and the souls sentence filter (pure)
// ---------------------------------------------------------------------------------------
/** A sentence that is about souls/spending (the Soul Urn is an objective, not economy). */
export const ECONOMY_RE = /\bsouls?\b(?!\s*urn)|\bunspent\b|\bspend(?:ing)?\b|\bhoard(?:ing)?\b|\b(?:800|1,?600|3,?200|4,?800|6,?400)\b/i;
/** The coaching themes the theme memory tracks. */
export const THEMES = ['positioning', 'healing', 'team', 'objective', 'economy', 'map', 'itemization', 'wave', 'mechanics', 'reinforce'] as const;
export type Theme = typeof THEMES[number];
/** The model's "focus" tag → theme (old tags such as souls, health, jungle and range included). */
const FOCUS_THEMES: Array<[RegExp, Theme]> = [
  [/reinforc|prais|good.?play|keep.?doing|well.?done/, 'reinforce'],
  [/soul|econom|spend|hoard|unspent/, 'economy'], [/item|build|buy|shop|purchase/, 'itemization'], [/heal|health|\bhp\b|sustain/, 'healing'],
  [/position|range|spacing|distance|safety|retreat|dive|melee|escape/, 'positioning'], [/objective|walker|guardian|midboss|urn|rift|shrine|patron/, 'objective'],
  [/wave|lane|push|trooper|last.?hit|jungle|camp|farm/, 'wave'], [/map|minimap|missing|vision|aware|gank/, 'map'],
  [/team|group|regroup|fight|rotat|respawn|join|window/, 'team'], [/mechanic|aim|abilit|cooldown|stamina|\bcc\b|debuff|combo/, 'mechanics'],
];
/** Themes found in advice text (in priority order). "Guardian" alone is where you heal, not an objective. */
const TEXT_THEMES: Array<[RegExp, Theme]> = [
  [/\bheal|\bhealth\b|\bhp\b|sustain|regen/i, 'healing'],
  [/position|max(?:imum)? range|spacing|distance|retreat|\bdiv(?:e|es|ing|ers?)\b|melee|escape|front ?line|\bcover\b|out of (?:their |his |her )?reach/i, 'positioning'],
  [/objective|walker|midboss|rejuvenator|\burn\b|rift|shrine|patron|structure|(?:take|hit|push|kill|destroy|attack)\b[^.]{0,30}\bguardian/i, 'objective'],
  [/\bwaves?\b|\blane\b|trooper|last.?hit|jungle|\bcamps?\b|\bfarm|sinner/i, 'wave'],
  [/\bmap\b|minimap|missing|vision|aware|gank|unseen|\bfog\b|vanished/i, 'map'],
  [/\bteam|\bgroup|regroup|\bfight|rotat|respawn|\bjoin|window|together|teammate|\bally\b|allies/i, 'team'],
  [/\bitems?\b|\bbuild\b|\bshop\b|purchase|upgrade/i, 'itemization'],
  [/\baim\b|abilit|cooldown|stamina|\bdash\b|parry|combo|\bstun|debuff|cleanse|silence/i, 'mechanics'],
];
const sentencesOf = (text: string) => text.match(/[^.!?]+[.!?]+(?=\s|$)|[^.!?]+$/g)?.map(x => x.trim()).filter(Boolean) || [];
/** All themes an advice text touches. */
export function themesIn(text: string): Theme[] {
  const parts = sentencesOf(text);
  const out = TEXT_THEMES.filter(([re]) => re.test(text)).map(([, t]) => t);
  if (parts.some(p => ECONOMY_RE.test(p))) out.unshift('economy');
  return [...new Set(out)];
}
/** Normalized theme of an answer: all-souls text is 'economy' whatever the model's tag says; a tag
 *  that is no theme (e.g. "death") falls back to the text. */
export function topicOf(focus: string, text?: string | null): string {
  const parts = text ? sentencesOf(text) : [];
  if (parts.length && parts.every(p => ECONOMY_RE.test(p))) return 'economy';
  const f = (focus || '').toLowerCase();
  const tagged = FOCUS_THEMES.find(([re]) => re.test(f))?.[1];
  if (tagged) return tagged;
  const t = text || '';
  return TEXT_THEMES.find(([re]) => re.test(t))?.[1] || 'general';
}
/** Live triggers that make a theme worth coaching again inside the 5-minute window. */
export const THEME_TRIGGERS: Record<string, string[]> = {
  positioning: ['diver-near', 'enemy-near'], healing: ['hp-low'], team: ['respawn', 'allies-down', 'isolated', 'enemy-window'],
  objective: ['enemy-window', 'objective-soon'], economy: ['souls-tier'], map: ['missing', 'isolated'], itemization: ['souls-tier'],
  wave: ['phase'], mechanics: ['debuff'], reinforce: [],
};
/** Profile-weakness concepts: the stock advice lines the old coach restated every call. Each has
 *  the live triggers that make it new again and the profile weakness it coaches. */
export const CONCEPTS: Record<string, { re: RegExp; triggers: string[]; label: string; profile: RegExp }> = {
  close: { re: /max(?:imum)? range|\bmelee\b|close[- ]range|\bdiv(?:e|es|ing|ers?)\b|front ?line|out of (?:their |his |her )?(?:melee )?reach|keep (?:your )?distance/i,
    triggers: ['diver-near', 'enemy-near'], label: 'range against divers', profile: /Killed at close range|Same killer again|melee/i },
  alone: { re: /walk(?:ing)? out|(?:with|beside) a (?:living )?teammate|cross(?:ing)? the map|\balone\b|\bsolo\b|by yourself/i,
    triggers: ['respawn', 'isolated'], label: 'not walking out alone', profile: /Dies soon after respawning|Dies alone/i },
  hurt: { re: /\bheal(?:ed|ing)?\b|full health|low health|(?:above|below|under) \d+ ?(?:%|percent)/i,
    triggers: ['hp-low'], label: 'healing before fights', profile: /Goes back in hurt|Stays in at low health|Gets burst down/i },
  fallback: { re: /fall back|back off|regroup|(?:hold|stay|wait) (?:at|near|by|behind) (?:your )?guardian|until [^.]{0,40}(?:respawns?|returns?|are back|is back)/i,
    triggers: ['allies-down', 'respawn'], label: 'waiting for the team', profile: /Stays in lost fights|Dies alone/i },
  missing: { re: /missing|unseen|vanished|\bfog\b|check the minimap/i, triggers: ['missing', 'isolated'], label: 'missing enemies', profile: /Pushes in with enemies missing/i },
  midboss: { re: /midboss|rejuvenator/i, triggers: ['enemy-window', 'objective-soon'], label: 'Midboss', profile: /Midboss/i },
  cc: { re: /\bstun(?:ned)?\b|silence|\bslow(?:ed)?\b|debuff|cleanse|stamina/i, triggers: ['debuff'], label: 'crowd control', profile: /crowd control|stun|debuff|CC\b/i },
};
/** Weakness concepts an advice text coaches. */
export function conceptsIn(text: string): string[] { return Object.entries(CONCEPTS).filter(([, c]) => c.re.test(text)).map(([k]) => k); }
const ago = (history: AiHistory, now: number, r: { t: number; clock: number | null }) => {
  const clock = history.samples.at(-1)?.clock ?? null;
  return clock != null && r.clock != null ? clock - r.clock : (now - r.t) / 1000;
};
/** A trigger (key or key prefix) is active now and became active again after `rec` was spoken. */
function freshTrigger(history: AiHistory, keys: string[], rec?: AdviceRecord | null): string | null {
  for (const k of history.active) {
    const base = k.replace(/:.*/, '');
    if (!keys.includes(k) && !keys.includes(base)) continue;
    if (!rec) return k;
    if ((history.episodes[k] || 0) > (rec.episodes?.[k] ?? 0)) return k;
  }
  return null;
}
/** The most recent AI advice (within THEME_WINDOW_SECS) that coached `concept` with no new trigger
 *  since — or a rule-coach profile-focus call that said it (those count unless a trigger is live). */
function conceptBlocker(concept: string, history: AiHistory, now: number): { clock: number | null; by: string } | null {
  const c = CONCEPTS[concept]; if (!c) return null;
  const rec = [...history.advice].reverse().find(r => ago(history, now, r) <= THEME_WINDOW_SECS && ago(history, now, r) >= 0 && (r.concepts ?? conceptsIn(r.text)).includes(concept));
  if (rec) return freshTrigger(history, c.triggers, rec) ? null : { clock: rec.clock, by: 'AI' };
  const rule = history.recent(history.spoken, now, THEME_WINDOW_SECS).filter(x => !x.ai && /^player:focus/.test(x.key) && c.re.test(x.text)).at(-1);
  if (rule) return freshTrigger(history, c.triggers, null) ? null : { clock: rule.clock, by: 'rule profile focus' };
  return null;
}
function conceptCoached(concept: string, history: AiHistory, now: number): boolean { return !!conceptBlocker(concept, history, now); }
/** "THEMES COACHED" for the prompt: themes and weakness concepts of the last 10 minutes, with the
 *  time each was last spoken and whether a NEW trigger now allows it again. */
export function themeMemory(history: AiHistory, now: number, profile?: string | null): string[] {
  const recs = history.advice.filter(r => { const a = ago(history, now, r); return a >= 0 && a <= 600; });
  if (!recs.length) return [];
  const lines: string[] = [];
  const themes = new Map<string, AdviceRecord[]>();
  for (const r of recs) themes.set(r.topic, [...(themes.get(r.topic) || []), r]);
  for (const [theme, rs] of themes) {
    const last = rs.at(-1)!, a = ago(history, now, last);
    const fresh = a <= THEME_WINDOW_SECS ? freshTrigger(history, THEME_TRIGGERS[theme] || [], last) : null;
    lines.push(`- theme ${theme}: ${rs.map(r => clockText(r.clock)).join(', ')}${a > THEME_WINDOW_SECS ? ' (free again)' : fresh ? ` (NEW trigger now: ${fresh.replace(/:.*/, '')})` : ' (blocked)'}`);
  }
  for (const [key, c] of Object.entries(CONCEPTS)) {
    const rs = recs.filter(r => (r.concepts ?? []).includes(key)); if (!rs.length) continue;
    const blocker = conceptBlocker(key, history, now);
    const weakness = profileLabel(profile, c.profile);
    lines.push(`- weakness "${c.label}"${weakness ? ` [${weakness}]` : ''}: ${rs.map(r => clockText(r.clock)).join(', ')}${blocker ? ' (blocked)' : ' (allowed: new trigger or 5 min passed)'}`);
  }
  return lines;
}
const STOP = new Set('the and for you your you\'re now with this that then them they are from into before after its it\'s get got have has was were will can not don\'t but just still too very over back off out off'.split(' '));
export function contentWords(text: string): Set<string> {
  return new Set(text.toLowerCase().replace(/\d[\d,.]*/g, ' # ').split(/[^a-z#']+/).filter(w => w === '#' || (w.length >= 3 && !STOP.has(w))).map(w => w.replace(/'s$/, '')));
}
/** Share of the shorter text's content words that the other text also has (0..1). */
export function overlap(a: string, b: string): number {
  const A = contentWords(a), B = contentWords(b);
  const small = A.size <= B.size ? A : B, big = small === A ? B : A;
  if (small.size < 3) return 0;
  let n = 0; for (const w of small) if (big.has(w)) n++;
  return n / small.size;
}
/** Drop the souls sentences from `text` (when the souls gate is closed). */
export function withoutEconomy(text: string): string { return sentencesOf(text).filter(s => !ECONOMY_RE.test(s)).join(' ').trim(); }

// ---------------------------------------------------------------------------------------
// This match's heroes: damage attribution and the named-hero check (pure)
// ---------------------------------------------------------------------------------------
const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
/** Hero names in `text` that are not in this match (allies, enemies, you), once at least three
 *  enemies are known. "Seven" followed by a unit ("seven seconds") is a number, not the hero. */
export function strangerHeroes(text: string, history: AiHistory): string[] {
  const r = history.rosterOf(); if (r.enemies.length < 3) return [];
  const here = new Set([...r.enemies, ...r.allies, history.hero].filter(Boolean));
  const out: string[] = [];
  for (const [id, label] of Object.entries(HERO_NAMES)) {
    if (here.has(id)) continue;
    for (const n of new Set([label, history.name(id)])) {
      if (n.length < 3 || n === n.toLowerCase()) continue;
      const re = new RegExp(`(?<![\\w'])${escapeRe(n)}(?:'s)?(?![\\w])${id === 'hero_gigawatt' ? '(?!\\s+(?:seconds?|minutes?|enemies|allies|heroes|of|more|troopers|souls|k\\b|thousand|hundred))' : ''}`);
      if (re.test(text)) { out.push(n); break; }
    }
  }
  return out;
}
/** Damage-panel rows attributed to this match's enemy heroes by ability name (data/abilities.json). */
export function damageByHero(rows: DeathRecord['damage'], history: AiHistory): { heroes: Array<{ hero: string; damage: number; sources: string[] }>; other: DeathRecord['damage'] } {
  const enemies = new Set(history.rosterOf().enemies);
  const by = new Map<string, { hero: string; damage: number; sources: string[] }>(); const other: DeathRecord['damage'] = [];
  for (const r of rows) {
    let ids = history.heroesOf(r.source);
    if (enemies.size) ids = ids.filter(h => enemies.has(h));
    if (ids.length !== 1) { other.push(r); continue; }
    const e = by.get(ids[0]) || { hero: ids[0], damage: 0, sources: [] };
    e.damage += r.damage ?? 0; if (!e.sources.includes(r.source)) e.sources.push(r.source); by.set(ids[0], e);
  }
  return { heroes: [...by.values()].sort((a, b) => b.damage - a.damage), other };
}

// ---------------------------------------------------------------------------------------
// Reinforcement gate (pure)
// ---------------------------------------------------------------------------------------
/** A sentence that praises (opens with "Good", "Nice", "Well done", "That's how"...). */
const PRAISE_RE = /^(?:good|great|nice|well done|excellent|perfect|clean|smart|strong|solid|textbook|that(?:'s| is) (?:how|the way|exactly|a great|a good))\b|\b(?:well done|good (?:job|work|push|call|play|reset|buy|shopping|delivery)|nice (?:push|work|job|play|reset|delivery)|great (?:job|push|play|work|delivery)|keep (?:doing|it up))\b/i;
export const isPraise = (sentence: string) => PRAISE_RE.test(sentence.trim());
export interface ReinforceGate { allowed: boolean; reason: string; play: GoodPlay | null; fresh: GoodPlay[] }
/** May the next AI tip praise a good play? Only for a fresh, not yet praised good play, at most one
 *  praise among REINFORCE_EVERY tips and REINFORCE_GAP_SECS apart. */
export function reinforceGate(history: AiHistory, now: number): ReinforceGate {
  const fresh = history.good.filter(g => { const a = ago(history, now, g); return PRAISABLE(g.kind) && a >= 0 && a <= REINFORCE_FRESH_SECS && !history.praised.includes(g.key); });
  const play = [...fresh].sort((a, b) => GOOD_ORDER.indexOf(a.kind) - GOOD_ORDER.indexOf(b.kind) || (b.clock ?? 0) - (a.clock ?? 0))[0] ?? null;
  if (!play) return { allowed: false, reason: `no fresh good play in the last ${REINFORCE_FRESH_SECS} s`, play: null, fresh };
  const recent = history.advice.slice(-(REINFORCE_EVERY - 1));
  if (recent.some(r => r.topic === 'reinforce')) return { allowed: false, reason: `one of your last ${REINFORCE_EVERY - 1} tips was already praise (at most 1 in ${REINFORCE_EVERY})`, play, fresh };
  const last = [...history.advice].reverse().find(r => r.topic === 'reinforce');
  if (last && ago(history, now, last) < REINFORCE_GAP_SECS) return { allowed: false, reason: `you praised at ${clockText(last.clock)}`, play, fresh };
  return { allowed: true, reason: `fresh good play: ${play.text}`, play, fresh };
}

export type AdviceCheck = { ok: true; speak: string; topic: string; note?: string;
  /** The good play this advice praised (its key), when it is a reinforcement. */
  praised?: string } | { ok: false; reason: string; topic: string };
/** Decide whether an automatic answer may be spoken. Pure: everything comes from the history. */
export function checkAdvice(advice: Advice, trigger: AiTrigger, souls: SoulsView, history: AiHistory, now: number): AdviceCheck {
  let speak = advice.speak || '';
  let topic = topicOf(advice.focus, speak);
  if (!speak) return { ok: false, reason: 'model passed', topic };
  let note: string | undefined;
  // Heroes that are not in this match (rec12 named the previous match's Baba and Graves): the
  // sentences naming them go, for every trigger (also when the player asked).
  const strangers = strangerHeroes(speak, history);
  if (strangers.length) {
    const rest = sentencesOf(speak).filter(x => !strangerHeroes(x, history).length).join(' ').trim();
    const why = `names ${strangers.join(', ')}, not in this match`;
    if (!rest || contentWords(rest).size < 4 || !ACTION_RE.test(rest)) return { ok: false, reason: why, topic };
    note = `sentence dropped (${why})`; speak = rest; topic = topicOf(advice.focus, speak);
  }
  if (trigger === 'manual') return { ok: true, speak, topic, ...(note ? { note } : {}) }; // the player asked: no further gating
  // Reinforcement: praise only for a fresh good play and at most 1 in 4 tips; otherwise the praise
  // sentences go and the rest must stand alone.
  const gate = reinforceGate(history, now);
  const praising = topic === 'reinforce' || sentencesOf(speak).some(isPraise);
  let praised: string | undefined;
  if (praising && !gate.allowed) {
    const rest = sentencesOf(speak).filter(x => !isPraise(x)).join(' ').trim();
    const why = `praise not allowed: ${gate.reason}`;
    if (!rest || contentWords(rest).size < 4 || !ACTION_RE.test(rest)) return { ok: false, reason: why, topic };
    note = [note, `praise dropped (${gate.reason})`].filter(Boolean).join('; '); speak = rest;
    topic = topicOf(topic === 'reinforce' ? '' : advice.focus, speak);
  } else if (praising) { topic = 'reinforce'; praised = gate.play!.key; }
  // A tier cheaper than the usual buy (alive) or than half of it (death screen) is poor advice late.
  const minTier = souls.dead ? Math.floor(souls.usual / 2) : souls.usual;
  const cheap = (x: string) => [...x.matchAll(/\b(800|1,?600|3,?200)\b(?:[- ]souls?)?[- ](?:items?|tier|upgrade)/gi)].some(m => Number(m[1].replace(',', '')) < minTier);
  // A praise of death-screen shopping talks about souls already spent: not souls advice.
  const blocked = (x: string) => ECONOMY_RE.test(x) && (!souls.allowed || cheap(x)) && !(praised && isPraise(x));
  if (sentencesOf(speak).some(blocked)) {
    const rest = sentencesOf(speak).filter(x => !blocked(x)).join(' ').trim();
    const why = souls.allowed ? `names a tier cheaper than ${fmt(minTier)} at ${clockText(souls.clock)}` : souls.reason;
    if (!rest) return { ok: false, reason: `souls advice blocked: ${why}`, topic: 'economy' };
    note = `souls sentence dropped (${why})`; speak = rest;
    // The model's tag described the dropped part: re-derive the topic from what is left.
    if (!praised) topic = topicOf(['economy', 'itemization'].includes(topicOf(advice.focus)) ? '' : advice.focus, speak);
  }
  const since = (r: { t: number; clock: number | null }) => ago(history, now, r);
  for (const r of history.advice) if (since(r) <= DUPLICATE_WINDOW_SECS && overlap(speak, r.text) >= NEAR_DUPLICATE)
    return { ok: false, reason: `near-duplicate of the AI call at ${clockText(r.clock)}`, topic };
  for (const c of history.recent(history.spoken, now)) if (!c.ai && overlap(speak, c.text) >= NEAR_DUPLICATE)
    return { ok: false, reason: `repeats the rule call "${c.text.slice(0, 50)}"`, topic };
  const done = (): AdviceCheck => ({ ok: true, speak, topic, ...(note ? { note } : {}), ...(praised ? { praised } : {}) });
  if (trigger !== 'periodic') return done();
  // Theme memory, 1: a sentence restating a weakness concept coached in the last 5 min (same trigger
  // episode) is dropped — rec11 appended "hold max range behind your front line" to 15 tips.
  const stale = (x: string) => conceptsIn(x).map(k => ({ k, b: conceptBlocker(k, history, now) })).find(c => c.b);
  const parts = sentencesOf(speak), dropped = parts.map(stale);
  if (dropped.some(Boolean)) {
    const first = dropped.find(Boolean)!;
    const why = `"${CONCEPTS[first.k].label}" was coached at ${clockText(first.b!.clock)}${first.b!.by === 'AI' ? '' : ` by the ${first.b!.by}`} and nothing new triggered it`;
    const rest = parts.filter((_, i) => !dropped[i]).join(' ').trim();
    // What is left must stand alone: an action, and no pronoun pointing at the dropped sentence
    // ("Take it together once your group is back." after dropping the Midboss sentence).
    const orphan = dropped[0] && /\b(it|its|them|they|that|this|there|then|him|her)\b/i.test(rest);
    // A praise may stand alone ("Good push while three were dead: that is how a lead snowballs.").
    const praiseOnly = !!praised && sentencesOf(rest).some(isPraise);
    if (!rest || contentWords(rest).size < 4 || orphan || !(ACTION_RE.test(rest) || praiseOnly)) return { ok: false, reason: why, topic };
    note = [note, `sentence dropped (${why})`].filter(Boolean).join('; '); speak = rest;
    if (!praised) topic = topicOf(advice.focus, speak);
  }
  // Theme memory, 2: one call per theme per 5 min unless a NEW trigger for that theme happened
  // since (a High priority alone is not new: rec11 marked half its repeats High).
  const prev = [...history.advice].reverse().find(r => r.topic === topic && since(r) >= 0 && since(r) <= THEME_WINDOW_SECS);
  // Reinforcement has its own gate (reinforceGate).
  if (prev && topic !== 'general' && topic !== 'reinforce' && !freshTrigger(history, THEME_TRIGGERS[topic] || [], prev))
    return { ok: false, reason: `same theme (${topic}) as the AI call at ${clockText(prev.clock)} and no new trigger since`, topic };
  return done();
}
/** The profile weakness label an advice text coaches (first matching concept), if any. */
export function weaknessOf(text: string, profile: string | null | undefined): string | null {
  for (const k of conceptsIn(text)) { const label = profileLabel(profile, CONCEPTS[k].profile); if (label) return label; }
  return null;
}

/** Upcoming objective events (seconds) within `horizon` of `clock`, from docs/timings/match-timeline.md. */
export function upcomingObjectives(clock: number, horizon = 180): string[] {
  const events: Array<[number, string]> = [[120, 'easy camps spawn'], [300, 'medium camps spawn'], [360, 'Zipline Boost unlocks'], [480, "Sinner's Sacrifices and hard camps spawn"],
    [660, 'first Unstable Rift (about)'], [720, 'Guardians reach minimum resist'], [1080, 'Walkers reach minimum resist']];
  for (let t = 300; t <= 3600; t += 300) events.push([t, 'Powerups spawn']);
  for (let t = 600; t <= 3600; t += 300) events.push([t, 'Soul Urn spawns']);
  return events.filter(([t]) => t > clock && t - clock <= horizon).sort((a, b) => a[0] - b[0] || a[1].localeCompare(b[1]))
    .map(([t, what]) => `${what} at ${clockText(t)} (in ${clockText(t - clock)})`);
}

/** Item facts for the summary (name, cost, slot), from assets/items.json. */
export interface ItemInfo { name: string; cost?: number | null; slot?: string | null }
export interface SummaryInput { snapshot: AiSnapshot; history: AiHistory; now: number; trigger: AiTrigger; name?: (id: string) => string;
  /** Player-profile text, used to mark COACHING ANGLES that match a known weakness. */
  profile?: string | null;
  /** What changed to trigger this request ("WHY NOW"); periodic requests always state one. */
  reason?: string | null;
  /** Item lookup for names, costs and slots (optional). */
  items?: (id: string) => ItemInfo | null }
export function buildSummary({ snapshot: s, history, now, trigger, name = defaultName, profile, reason, items }: SummaryInput, maxChars = SUMMARY_MAX_CHARS): string {
  // [weight, text]: lower weights are dropped first when the summary exceeds its budget.
  const lines: Array<[number, string]> = [];
  const add = (w: number, t: string | null | false | undefined) => { if (t) lines.push([w, t]); };
  const clock = num(s.game_clock_secs), dead = isDead(s);
  const phase = clock == null ? 'unknown phase' : clock < 480 ? 'laning phase, until about 8:00' : clock < 1200 ? 'midgame, 8:00 to 20:00' : 'lategame, from 20:00';
  add(10, `Trigger: ${trigger} · clock ${clockText(clock)} (${phase}) · screen ${s.screen || 'unknown'}${dead ? ' · YOU ARE DEAD' : ''}`);
  if (reason) add(10, `WHY NOW: ${reason}`);
  const kda = [s.kills, s.deaths, s.assists].every(v => num(v) != null) ? ` · K/D/A ${s.kills}/${s.deaths}/${s.assists}` : '';
  const hero = s.hero || history.hero; // the death screen hides the hero portrait
  const role = hero ? HERO_ROLES[hero] : undefined;
  // A profile learned on another hero (rec11: "mostly Grey Talon" while playing Violet).
  const profileHero = profile?.match(/mostly ([A-Z][\w &'.-]+?)[,)]/)?.[1];
  const other = hero && profileHero && profileHero.toLowerCase() !== name(hero).toLowerCase() ? ` · profile is mostly from ${profileHero} games: check its weaknesses fit ${name(hero)}` : '';
  add(10, `You: ${hero ? `${name(hero)} (${role || 'role not in the coach table: do not guess the kit'})` : 'hero unknown'}${num(s.level) != null ? ` · level ${s.level}` : ''}${kda}${other}`);
  // Health and its trend over the window.
  const samples = history.recent(history.samples, now);
  if (!dead && num(s.hp) != null && num(s.max_hp)) {
    const pct = Math.round(s.hp! / s.max_hp! * 100);
    const latestClock = samples.at(-1)?.clock ?? null;
    const at = (ago: number) => {
      const target = latestClock != null ? latestClock - ago : null;
      const pick = target != null ? samples.filter(x => x.clock != null && x.clock <= target).at(-1) : undefined;
      return pick && pick.hp != null && pick.maxHp ? (pick.dead ? 'dead' : `${Math.round(pick.hp / pick.maxHp * 100)}`) : null;
    };
    const trend = [at(90), at(60), at(30)].filter(Boolean);
    const delta = num(s.health_delta_ratio);
    add(9, `HP ${fmt(s.hp!)}/${fmt(s.max_hp!)} (${pct}%)${trend.length ? ` · HP% 90s/60s/30s ago→now: ${[...trend, String(pct)].join('→')}` : ''}${delta != null && Math.abs(delta) >= .05 ? ` · ${delta < 0 ? 'falling' : 'rising'} ${Math.round(Math.abs(delta) * 100)}% recently` : ''}`);
  }
  if (Array.isArray(s.status_effects) && s.status_effects.length) add(7, `Status effects: ${s.status_effects.slice(0, 6).map(x => String(x).slice(0, 30)).join(', ')}`);
  // Souls against the item tier normally bought at this game time (economy.rs spend_target):
  // below it the player is saving, not hoarding. The line also states whether souls advice is
  // allowed at all, which checkAdvice() enforces.
  {
    const view = soulsView(s, history, now); let held = '';
    if (view.souls != null && view.souls >= view.usual && !view.dead) {
      const floor = view.souls * .75; let first: Sample | undefined;
      for (let i = samples.length - 1; i >= 0 && samples[i].souls != null && samples[i].souls! >= floor; i--) first = samples[i];
      const last = samples.at(-1);
      const secs = first && last ? (first.clock != null && last.clock != null ? last.clock - first.clock : Math.round((last.t - first.t) / 1000)) : 0;
      if (secs >= 20) held = ` · held ≥${fmt(floor)} for ${secs}s`;
    }
    add(9, soulsLine(view, s, held));
  }
  // Just respawned: the window where this player's profile says deaths come from walking out alone.
  const back = !dead ? history.respawned(now) : null;
  if (back) add(9, `Respawned ${back.ago}s ago (last death at ${clockText(back.death.clock)}${back.death.cause ? `, cause ${back.death.cause}` : ''})`);
  if (s.items?.length) {
    // Names with cost, and souls per slot: the itemization theme needs to see what the build lacks.
    const info = s.items.slice(0, 12).map(id => ({ id, i: items?.(id) ?? null }));
    const slots = new Map<string, number>();
    for (const { i } of info) if (i?.slot && i.cost) slots.set(i.slot, (slots.get(i.slot) || 0) + i.cost);
    const bySlot = slots.size ? ` · invested by slot: ${['weapon', 'vitality', 'spirit'].map(k => `${k} ${fmt(slots.get(k) || 0)}`).join(', ')}` : '';
    add(6, `Your items (${s.items.length}): ${info.map(({ id, i }) => `${i?.name || name(id)}${i?.cost ? ` (${fmt(i.cost)})` : ''}`).join(', ')}${bySlot}`);
  }
  // Area.
  const map = s.minimap;
  if (map?.your_lane || map?.zone) add(8, `Area: ${map.your_lane ? `${map.your_lane} lane` : 'lane unknown'}${num(map.your_progress) != null ? `, ${Math.round(map.your_progress! * 100)}% up lane` : ''}${map.zone ? ` · zone ${String(map.zone).slice(0, 40)}` : ''}`);
  // Teams from the top bar.
  const status = s.hero_status || [];
  for (const ally of [true, false]) {
    const team = status.filter(p => p.ally === ally);
    if (!team.length) continue;
    const label = (p: typeof team[number]) => p.hero ? name(p.hero) : `${ally ? 'ally' : 'enemy'} ${p.slot + 1}`;
    const deadOnes = team.filter(p => p.dead === true), unseen = team.filter(p => p.dead !== true && p.bar_visible === false);
    const low = team.filter(p => p.dead !== true && p.bar_visible !== false && num(p.health_ratio) != null && p.health_ratio! < .35);
    const alive = team.length - deadOnes.length;
    add(ally ? 8 : 9, `${ally ? 'Allies' : 'Enemies'}: ${alive} alive${deadOnes.length ? `, dead: ${deadOnes.map(p => `${label(p)}${p.respawn_secs != null ? ` (${p.respawn_secs}s)` : ''}`).join(', ')}` : ''}${!ally && unseen.length ? `, not seen recently: ${unseen.length}` : ''}${low.length ? `, low HP: ${low.map(p => `${label(p)} ${Math.round(p.health_ratio! * 100)}%`).join(', ')}` : ''}`);
  }
  if (!status.length) {
    if (s.enemy_heroes?.length) add(6, `Enemy heroes: ${s.enemy_heroes.map(name).join(', ')}`);
  } else if (s.enemy_heroes?.length && status.filter(p => !p.ally && p.hero).length < s.enemy_heroes.length) add(5, `Enemy heroes: ${s.enemy_heroes.map(name).join(', ')}`);
  if (s.ally_heroes?.length && !status.some(p => p.ally && p.hero)) add(4, `Allied heroes: ${s.ally_heroes.map(name).join(', ')}`);
  {
    // Enemy kit roles for the heroes in the coach table (so "Calico dives" is grounded, and Graves
    // is not called a melee hero).
    const roster = [...new Set(s.enemy_heroes?.length ? s.enemy_heroes : status.filter(p => !p.ally && p.hero).map(p => p.hero!))];
    const known = roster.filter(h => HERO_ROLES[h]);
    if (known.length) add(5, `Enemy roles: ${known.map(h => `${name(h)} ${HERO_ROLES[h]}`).join('; ')}${known.length < roster.length ? `; others not in the coach table` : ''}`);
  }
  if (s.enemy_items?.length) add(4, `Enemy items seen: ${s.enemy_items.slice(0, 10).map(id => items?.(id)?.name || name(id)).join(', ')}`);
  if (num(s.ally_networth) != null && num(s.enemy_networth) != null) {
    const d = s.ally_networth! - s.enemy_networth!;
    // Trend: the difference about 3 minutes ago (game clock) from the sample history.
    const past = clock != null ? history.samples.filter(x => x.nwDiff != null && x.clock != null && x.clock <= clock - 150 && x.clock >= clock - 240).at(-1) : undefined;
    const trend = past?.nwDiff != null ? (() => { const ch = d - past.nwDiff!; return ` · at ${clockText(past.clock)} it was ${past.nwDiff! >= 0 ? '+' : ''}${fmt(past.nwDiff!)}: ${Math.abs(ch) < 1500 ? 'steady' : ch > 0 ? `your team gained ${fmt(ch)} since` : `your team fell ${fmt(-ch)} further behind since`}`; })() : '';
    add(6, `Team souls: yours ${fmt(s.ally_networth!)} vs enemy ${fmt(s.enemy_networth!)} (${d >= 0 ? '+' : ''}${fmt(d)})${trend}`);
  }
  // Minimap hero tracks relative to you.
  if (map?.heroes?.length) {
    const enemies = map.heroes.filter(h => !h.ally && h.hits >= 2), visible = enemies.filter(h => h.unseen_ms === 0);
    const missing = enemies.filter(h => h.unseen_ms > 0);
    const you = map.you;
    const near = you ? visible.filter(h => Math.hypot(h.pos.x - you.x, h.pos.y - you.y) < .12).length : null;
    const allyNear = you ? map.heroes.filter(h => h.ally && h.unseen_ms === 0 && h.hits >= 2 && Math.hypot(h.pos.x - you.x, h.pos.y - you.y) < .12).length : null;
    // Lane colours are places, never heroes: "last seen 3s ago in Yellow lane" (rec11's "last seen 3s
    // ago Yellow" came back as "Enemy yellow is missing").
    const heroes = (n: number) => `${n} enemy hero${n === 1 ? '' : 'es'}`;
    add(8, `Minimap: ${heroes(visible.length)} visible${near != null ? ` (${near} near you, ${allyNear} allies near you)` : ''}${missing.length ? `, ${heroes(missing.length)} missing (last seen ${missing.map(h => `${Math.round(h.unseen_ms / 1000)}s ago${h.lane ? ` in ${h.lane} lane` : ''}`).join(', ')})` : ''}`);
  }
  if (map) {
    const lanes = (['green', 'blue', 'yellow'] as const).map(k => {
      const l = map[k]; if (!l) return null;
      return `${k[0].toUpperCase()}${k.slice(1)} lane ${l.push == null ? 'front unknown' : `front ${Math.round(l.push * 100)}% up`} (troopers ${l.ally_troopers} yours v ${l.enemy_troopers} theirs)`;
    }).filter(Boolean);
    if (lanes.length) add(6, `Lanes: ${lanes.join('; ')}`);
    const urn = { Visible: 'marker seen', Ground: 'on the ground', CarriedByYou: 'carried by you', CarriedByAlly: 'carried by an ally (inferred)', CarriedByEnemy: 'carried by an enemy (inferred)' } as const;
    // The Midboss is alive almost all game: only worth a line when it can matter.
    const enemyDeadNow = status.filter(p => !p.ally && p.dead === true).length;
    const midboss = map.midboss_up ? (clock != null && clock < 600 ? null : `Midboss alive${enemyDeadNow >= 3 ? ` and ${enemyDeadNow} enemies dead: a team take is possible` : ' (team target only after a won fight)'}`) : null;
    const objs = [midboss, map.urn ? `Soul Urn ${urn[map.urn]}` : null].filter(Boolean);
    if (objs.length) add(6, `Objectives: ${objs.join(' · ')}`);
  }
  if (clock != null) { const next = upcomingObjectives(clock); if (next.length) add(6, `Upcoming: ${next.slice(0, 3).join('; ')}`); }
  // Deaths.
  const deaths = history.deaths;
  const recentDeaths = history.recent(deaths, now, trigger === 'death' ? 600 : HISTORY_WINDOW_SECS);
  const shown = trigger === 'death' && deaths.length && !recentDeaths.includes(deaths.at(-1)!) ? [...recentDeaths, deaths.at(-1)!] : recentDeaths;
  if (deaths.length) add(7, `Deaths this match: ${history.deathCount}${deaths.length > 1 ? ` (at ${deaths.slice(-4).map(d => clockText(d.clock)).join(', ')})` : ''}`);
  const lastDeath = deaths.at(-1);
  for (const d of shown.slice(-2)) {
    const need = deathSpend(d.clock);
    const soulsAt = Math.max(d.soulsBefore ?? -1, d.soulsDead ?? -1);
    const current = (trigger === 'death' || dead) && d === lastDeath;
    add(current ? 10 : trigger === 'death' ? 6 : 8, `${current || trigger !== 'death' ? '' : 'Earlier '}Death at ${clockText(d.clock)}${current && shown.length > 1 ? ' (THIS death)' : ''}: cause ${d.cause || 'unknown'}${d.damage.length ? ` · damage taken: ${d.damage.map(x => `${x.source}${x.damage != null ? ` ${fmt(x.damage)}` : ''}`).join(', ')}` : ''}${d.killingBlow ? ` · killing blow ${d.killingBlow}` : ''}${d.status?.length ? ` · status effects before death: ${d.status.join(', ')}` : ''}${d.hpBefore != null ? ` · last alive HP ${d.hpBefore}%` : ''}${d.alliesNear != null ? ` · allies near you ${d.alliesNear}, enemies near ${d.enemiesNear ?? '?'}` : ''}${d.soulsBefore != null ? ` · unspent before death ${fmt(d.soulsBefore)}` : ''}${d.unsecuredBefore != null ? ` (unsecured ${fmt(d.unsecuredBefore)})` : ''}${soulsAt >= 0 ? ` · death-spend threshold ${fmt(need)}: ${soulsAt >= need ? 'reached' : 'not reached'}` : ''}${d.respawn != null ? ` · respawn ${d.respawn}s` : ''}`);
    // Who did the damage, by ability name, among THIS match's enemies (rec12 22:10: the panel said
    // "Trooper" while Drifter's Rend and Silver's Boot Kick were on it).
    if (current && d.damage.length) {
      const by = damageByHero(d.damage, history);
      const total = d.damage.reduce((n, x) => n + (x.damage ?? 0), 0), heroDmg = by.heroes.reduce((n, x) => n + x.damage, 0);
      const trooper = /trooper|creep|neutral|camp/i.test(`${d.cause || ''} ${d.killingBlow || ''}`) && heroDmg > 0;
      if (by.heroes.length || trooper) add(10, `Damage by hero (ability names matched to this match's enemies): ${by.heroes.length ? by.heroes.map(x => `${name(x.hero)} ${fmt(x.damage)} (${x.sources.join(', ')})`).join(', ') : 'none matched'}${by.other.length ? `; not attributable to one hero: ${by.other.map(x => `${x.source}${x.damage != null ? ` ${fmt(x.damage)}` : ''}`).join(', ')}` : ''}${trooper ? ` · the panel cause "${d.cause || d.killingBlow}" is misleading: hero abilities did ${total ? Math.round(heroDmg * 100 / total) : 0}% of the listed damage, so enemy heroes killed you, not troopers` : ''}`);
    }
  }
  if (trigger === 'death') {
    const r = history.rosterOf();
    if (r.enemies.length && !status.length && !s.enemy_heroes?.length) add(9, `Enemy heroes this match: ${r.enemies.map(name).join(', ')}`);
  }
  // The rule coach's death review of THIS death (names the killer hero and crowd control); a review
  // of an earlier death or match is never used.
  const review = lastDeath ? history.reviewsFor(lastDeath).slice(-2) : [];
  if (review.length && trigger === 'death')
    add(10, `Rule death review of this death (already spoken): ${review.map(c => c.text).join(' ')}`.slice(0, 400));
  else if (lastDeath && trigger !== 'death' && !shown.includes(lastDeath)) {
    // The last death, compactly, so a periodic call can build on it (killer, cause, how long ago).
    const killer = history.reviewsFor(lastDeath, /^death:review/).at(-1)?.text.match(/^Why you died: ([^.]{0,90})/)?.[1];
    const top = [...lastDeath.damage].sort((a, b) => (b.damage ?? 0) - (a.damage ?? 0))[0];
    add(history.recent([lastDeath], now, 300).length ? 7 : 5, `Last death at ${clockText(lastDeath.clock)}${clock != null && lastDeath.clock != null ? ` (${clockText(clock - lastDeath.clock)} ago)` : ''}: ${killer ? `${killer}; ` : ''}cause ${lastDeath.cause || 'unknown'}${top ? ` · most damage ${top.source}${top.damage != null ? ` ${fmt(top.damage)}` : ''}` : ''}${lastDeath.status?.length ? ` · ${lastDeath.status.join(', ')}` : ''}`);
  }
  // Good plays of this match (evidence for a short praise) and whether praise is allowed now.
  {
    const gate = reinforceGate(history, now);
    const recentGood = history.recent(history.good, now, 300).slice(-4);
    const streak = !dead && clock != null ? (() => { const d = history.deaths.at(-1); const from = d ? d.respawnedClock : history.samples.find(x => x.clock != null)?.clock; return from != null && clock - from >= 180 ? `alive without dying for ${Math.floor((clock - from) / 60)} min (since ${clockText(from)})` : null; })() : null;
    if (recentGood.length || streak) add(8, `GOOD PLAYS (this match, observed; evidence for praise):\n${[...recentGood.map(g => `- [${clockText(g.clock)}] ${g.text}${history.praised.includes(g.key) ? ' (already praised)' : g.kind === 'lead' ? ' (team context, not a play to praise)' : ''}`), ...(streak && !recentGood.some(g => g.kind === 'deathless') ? [`- now: ${streak}`] : [])].join('\n')}\nPraise: ${gate.allowed ? `allowed (${gate.reason})` : `not allowed (${gate.reason})`}`);
  }
  // What the AI already said and the theme memory (checkAdvice() enforces it).
  const mine = history.recent(history.advice, now, 600).slice(-4);
  if (mine.length) add(9, `RECENT AI ADVICE (yours):\n${mine.map(a => `- [${clockText(a.clock)}] ${a.topic}: ${a.text}`).join('\n')}`);
  const memory = themeMemory(history, now, profile);
  if (memory.length) add(9, `THEMES COACHED (last 10 min; "blocked" = do not coach it again unless WHY NOW gives a new trigger for it):\n${memory.join('\n')}`);
  // Recently spoken rule calls, so the model does not repeat them.
  const spoken = history.recent(history.spoken, now).filter(c => !c.ai).slice(-6);
  if (spoken.length) add(9, `RECENT COACH CALLS (already spoken):\n${spoken.map(c => `- [${clockText(c.clock)}] rule: ${c.text}`).join('\n')}`);
  else add(3, 'RECENT COACH CALLS: none');
  const angles = coachingAngles({ snapshot: s, history, now, profile, name });
  if (angles.length) add(9, `COACHING ANGLES (live situation → fix; [profile] = matches a known weakness):\n${angles.join('\n')}`);
  // Enforce the budget: drop the lowest-weight lines (latest first) until it fits.
  const keep = lines.map((l, i) => ({ w: l[0], t: l[1], i }));
  const total = () => keep.reduce((n, l) => n + l.t.length + 1, 0);
  while (total() > maxChars && keep.length > 1) {
    let drop = 0; for (let i = 1; i < keep.length; i++) if (keep[i].w <= keep[drop].w) drop = i;
    keep.splice(drop, 1);
  }
  const out = keep.map(l => l.t).join('\n');
  return out.length > maxChars ? out.slice(0, maxChars - 1) + '…' : out;
}

// ---------------------------------------------------------------------------------------
// Knowledge retrieval (keyword scoring over doc sections)
// ---------------------------------------------------------------------------------------
export interface KnowledgeSection { id: string; title: string; heading: string; text: string }
const SKIP_HEADING = /machine-friendly|update history|unverified|^sources?\b|known gaps|not obtained|^files$|^index$|^purpose$|camp positions|coordinate|^(weak|medium|strong|vault) \(\d+\)|how to read|cue table|counts and notes|verified quotes/i;
export function sections(docs: Knowledge[]): KnowledgeSection[] {
  const out: KnowledgeSection[] = [];
  for (const doc of docs) {
    if (!['strategy', 'jungle', 'timings'].includes(doc.category) || /README\.md$|coach-cue-library/.test(doc.id)) continue;
    const parts = doc.body.split(/^##\s+/m).slice(1); // ### subsections stay inside their ## section
    for (const part of parts) {
      const nl = part.indexOf('\n'); const heading = (nl < 0 ? part : part.slice(0, nl)).trim();
      if (SKIP_HEADING.test(heading)) continue;
      const text = (nl < 0 ? '' : part.slice(nl + 1)).split('\n')
        .filter(l => !/^\s*(Sources?|Retrieved|Legend)\b/i.test(l) && !/^\s*-\s*(Detection|Confidence)( signal)?:/i.test(l) && !/^\s*\|?\s*-{3,}/.test(l))
        .join(' ').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/https?:\/\/\S+/g, '').replace(/[*`#>]/g, '')
        .replace(/\s*\|\s*/g, ' | ').replace(/\s+/g, ' ').trim();
      if (text.length >= 80) out.push({ id: doc.id, title: doc.title, heading, text });
    }
  }
  return out;
}

/** Weighted search terms for the current situation (a term repeated = more weight). */
export function situationTerms(s: AiSnapshot, history: AiHistory, trigger: AiTrigger): string[] {
  const t: string[] = []; const dead = isDead(s) || trigger === 'death';
  const clock = num(s.game_clock_secs) ?? 0, souls = num(s.souls) ?? 0;
  const hp = num(s.hp) != null && num(s.max_hp) ? s.hp! / s.max_hp! : null;
  if (dead) {
    t.push('dying', 'dying', 'death', 'die', 'alone');
    const cause = (history.deaths.at(-1)?.cause || s.death_cause || '').toLowerCase();
    if (/guardian|walker|tower|patron|shrine/.test(cause)) t.push('guardian', 'guardian', 'dive');
    if (/trooper|camp|neutral|haunt/.test(cause)) t.push('camp', 'jungle');
    // Souls excerpts only for a death that held at least the death-spend threshold.
    const held = Math.max(souls, history.deaths.at(-1)?.soulsBefore ?? 0);
    if (held >= deathSpend(num(s.game_clock_secs))) t.push('haunted', 'unsecured', 'unspent', 'spend');
  } else if (souls >= spendTarget(num(s.game_clock_secs))) t.push('unspent', 'unspent', 'hoarding', 'buy', 'shop'); // saving below the usual buy is not hoarding
  if (!dead && history.respawned(history.samples.at(-1)?.t ?? 0)) t.push('respawn', 'alone', 'regroup');
  const zone = String(s.minimap?.zone || '').toLowerCase();
  if (/jungle|camp|district|neutral/.test(zone)) t.push('jungle', 'jungle', 'jungle', 'camp', 'invading', 'counter-jungling', 'stealing');
  if (hp != null && hp < .4 && !dead) t.push('trading', 'retreat', 'health');
  if (clock < 480) t.push('laning', 'wave', 'last hitting', 'trading');
  else if (clock < 1200) t.push('midgame', 'rotations', 'walker', 'objective');
  else t.push('lategame', 'throw', 'closing', 'objective');
  const enemyDead = (s.hero_status || []).filter(p => !p.ally && p.dead === true).length;
  const allyDead = (s.hero_status || []).filter(p => p.ally && p.dead === true).length;
  if (enemyDead >= 2) t.push('objective', 'objective', 'walker', 'chasing');
  if (allyDead >= 2 && !dead) t.push('regroup', 'split', 'alone');
  const missing = (s.minimap?.heroes || []).filter(h => !h.ally && h.unseen_ms > 0).length;
  if (missing >= 2) t.push('missing', 'minimap');
  // The Midboss icon is up almost all game; its excerpts ("Midboss is up. Don't die alone.") only
  // when a take is plausible (rec11 repeated that line 10 times).
  if (s.minimap?.midboss_up && clock >= 600 && enemyDead >= 3) t.push('midboss', 'rejuvenator');
  if (s.minimap?.urn) t.push('urn', 'urn');
  return t;
}

const countOf = (hay: string, needle: string) => { let n = 0, i = 0; while ((i = hay.indexOf(needle, i)) >= 0) { n++; i += needle.length; } return n; };
export function retrieve(all: KnowledgeSection[], terms: string[], limit = EXCERPTS_MAX): Array<KnowledgeSection & { score: number }> {
  const weights = new Map<string, number>(); for (const term of terms) weights.set(term, (weights.get(term) || 0) + 1);
  const scored = all.map(sec => {
    const heading = sec.heading.toLowerCase(), title = sec.title.toLowerCase(), body = sec.text.toLowerCase(); let score = 0;
    for (const [term, w] of weights) score += w * ((heading.includes(term) ? 6 : 0) + (title.includes(term) ? 3 : 0) + Math.min(3, countOf(body, term)));
    return { ...sec, score };
  }).filter(s => s.score >= 6).sort((a, b) => b.score - a.score || a.id.localeCompare(b.id) || a.heading.localeCompare(b.heading));
  const picked: typeof scored = [];
  for (const s of scored) { if (picked.length >= limit) break; if (picked.filter(p => p.id === s.id).length < 2) picked.push(s); }
  return picked;
}

/** One-line correlational matchup facts from docs/heroes for the player's hero vs known enemies. */
export function heroMatchup(docs: Knowledge[], hero: string | null | undefined, enemies: string[], name: (id: string) => string = defaultName): string | null {
  if (!hero || !enemies.length) return null;
  const doc = docs.find(d => d.category === 'heroes' && d.body.includes(`\`${hero}\``)); if (!doc) return null;
  const names = new Set(enemies.map(e => name(e).toLowerCase()));
  const rows = (title: string) => {
    const part = doc.body.split(/^##\s+/m).find(p => p.toLowerCase().startsWith(title)); if (!part) return [];
    return [...part.matchAll(/^\|\s*([^|]+?)\s*\|\s*([\d.]+)\s*\|/gm)].filter(m => names.has(m[1].toLowerCase())).map(m => `${m[1]} ${m[2]}%`);
  };
  const strong = rows('strong against'), weak = rows('weak against');
  if (!strong.length && !weak.length) return null;
  return `Matchup stats for ${name(hero)} (correlational win rates): ${weak.length ? `weak vs ${weak.join(', ')}` : ''}${weak.length && strong.length ? '; ' : ''}${strong.length ? `strong vs ${strong.join(', ')}` : ''}.`;
}

// ---------------------------------------------------------------------------------------
// Prompt assembly with budget enforcement
// ---------------------------------------------------------------------------------------
export interface PromptInput { trigger: AiTrigger; summary: string; profile?: string | null; excerpts?: Array<{ title: string; heading: string; text: string }>; matchup?: string | null; brief?: string;
  /** Set when asking once more after an answer was rejected as a repeat. */
  retry?: { reason: string; text: string } | null }
export interface Prompt { system: string; user: string; chars: number; tokens: number }
export function buildPrompt(input: PromptInput, maxChars = MAX_INPUT_CHARS): Prompt {
  const brief = input.brief ?? COACH_BRIEF;
  let summary = input.summary.slice(0, SUMMARY_MAX_CHARS);
  let profile = (input.profile || '').trim().slice(0, PROFILE_MAX_CHARS);
  let excerpts = (input.excerpts || []).slice(0, EXCERPTS_MAX).map(e => ({ ...e, text: e.text.length > EXCERPT_MAX_CHARS ? e.text.slice(0, EXCERPT_MAX_CHARS - 1) + '…' : e.text }));
  const render = () => [TASKS[input.trigger], '', 'STATE', summary,
    ...(profile ? ['', 'PLAYER PROFILE (from earlier matches; coach these weaknesses when the live situation matches; heroes and killers it names were in OTHER matches)', profile] : []),
    ...(input.retry ? ['', `YOUR PREVIOUS ANSWER WAS NOT USED (${input.retry.reason}): "${input.retry.text.slice(0, 200)}". Give advice on a different focus, or PASS.`] : []),
    '', 'KNOWLEDGE', brief, ...(input.matchup ? [input.matchup.slice(0, 300)] : []),
    ...(excerpts.length ? ['', 'REFERENCE EXCERPTS', ...excerpts.map(e => `[${e.title} › ${e.heading}] ${e.text}`)] : []),
    '', 'Reply with the JSON object only.'].join('\n');
  let user = render();
  const size = () => SYSTEM_PROMPT.length + user.length;
  while (size() > maxChars && excerpts.length) { excerpts = excerpts.slice(0, -1); user = render(); }
  if (size() > maxChars && profile) { profile = profile.slice(0, Math.max(0, profile.length - (size() - maxChars))); user = render(); }
  if (size() > maxChars) { summary = summary.slice(0, Math.max(200, summary.length - (size() - maxChars))); user = render(); }
  const chars = size();
  return { system: SYSTEM_PROMPT, user, chars, tokens: estimateTokens(chars) };
}

// ---------------------------------------------------------------------------------------
// Provider request shapes and reply extraction
// ---------------------------------------------------------------------------------------
export function configFrom(s: Settings): AiConfig {
  return { provider: s.aiProvider, baseUrl: s.aiBaseUrl, model: s.aiModel, key: s.aiKey, maxTokens: s.aiMaxTokens, headers: s.aiHeaders, reasoning: s.aiReasoning ?? 'low' };
}
/** OpenAI reasoning models (o1/o3/o4-mini, gpt-5*) on the OpenAI API: they take `reasoning_effort`
 *  and `max_completion_tokens` and reject a non-default temperature. */
export const isOpenAiReasoningModel = (model: string) => /^(?:openai\/)?(?:o\d(?:-|$)|gpt-5)/i.test(model.trim());
export const isOpenRouter = (cfg: Pick<AiConfig, 'baseUrl'>) => /(^|\/\/|\.)openrouter\.ai(\/|:|$)/i.test(cfg.baseUrl.trim());
/** How a reasoning setting is sent, and how many extra tokens the reply needs for the thinking.
 *  - OpenRouter (base URL on openrouter.ai): the unified `reasoning` object. Off = {enabled: false},
 *    Low = {effort: "low"} (Anthropic models then think with a 1,024-token budget, which must stay
 *    below max_tokens, hence +1,024), Default = nothing (the model's own default; +2,048 headroom).
 *  - OpenAI reasoning models on the OpenAI API: `reasoning_effort` ("minimal" for gpt-5 / "low"
 *    for o-series when Off; "low" when Low).
 *  - Anthropic Messages: extended thinking is off unless asked for, so nothing is sent.
 *  - Other OpenAI-compatible gateways (Ollama, LiteLLM...): nothing (unknown fields can fail). */
export function reasoningParams(cfg: AiConfig): { body: Record<string, unknown>; headroom: number; label: string } {
  const mode = cfg.reasoning ?? 'default';
  if (cfg.provider === 'anthropic') return { body: {}, headroom: 0, label: 'Anthropic: no extended thinking' };
  if (isOpenRouter(cfg)) {
    if (mode === 'off') return { body: { reasoning: { enabled: false } }, headroom: 0, label: 'OpenRouter reasoning off' };
    if (mode === 'low') return { body: { reasoning: { effort: 'low' } }, headroom: 1024, label: 'OpenRouter reasoning effort low' };
    return { body: {}, headroom: 2048, label: 'OpenRouter model default reasoning' };
  }
  if (isOpenAiReasoningModel(cfg.model)) {
    const gpt5 = /gpt-5/i.test(cfg.model);
    if (mode === 'off') return { body: { reasoning_effort: gpt5 ? 'minimal' : 'low' }, headroom: gpt5 ? 256 : 1024, label: `reasoning_effort ${gpt5 ? 'minimal' : 'low'}` };
    if (mode === 'low') return { body: { reasoning_effort: 'low' }, headroom: 1024, label: 'reasoning_effort low' };
    return { body: {}, headroom: 2048, label: 'model default reasoning' };
  }
  return { body: {}, headroom: 0, label: mode === 'default' ? 'not sent' : `not sent (${cfg.provider} gateway without a known reasoning parameter)` };
}
/** max_tokens actually sent: the reply budget plus the reasoning headroom (capped at 16,384). */
export const effectiveMaxTokens = (cfg: AiConfig, maxTokens = cfg.maxTokens) => Math.min(16384, maxTokens + reasoningParams(cfg).headroom);
export function parseExtraHeaders(text: string): Record<string, string> {
  if (!text.trim()) return {};
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new Error('Extra headers must be a JSON object, e.g. {"HTTP-Referer": "https://example.com"}'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Extra headers must be a JSON object of string values');
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(value)) {
    if (!/^[A-Za-z0-9!#$%&'*+.^_`|~-]+$/.test(k) || typeof v !== 'string') throw new Error(`Invalid extra header "${k.slice(0, 40)}": names must be tokens and values strings`);
    out[k] = v;
  }
  return out;
}
const trimSlash = (u: string) => u.trim().replace(/\/+$/, '');
export function endpoint(cfg: AiConfig): string {
  const base = trimSlash(cfg.baseUrl || PROVIDER_PRESETS[cfg.provider].baseUrl);
  return cfg.provider === 'anthropic' ? `${base.replace(/\/v1$/, '')}/v1/messages` : `${base.replace(/\/chat\/completions$/, '')}/chat/completions`;
}
export interface GatewayRequest { url: string; init: { method: 'POST'; headers: Record<string, string>; body: string } }
/** `maxTokens` is the reply budget; the reasoning headroom of reasoningParams() is added to it.
 *  `opts.noReasoning` leaves the reasoning field out (fallback when a gateway rejects it). */
export function buildRequest(cfg: AiConfig, system: string, user: string, maxTokens = cfg.maxTokens, opts: { noReasoning?: boolean } = {}): GatewayRequest {
  const extra = parseExtraHeaders(cfg.headers);
  const r = opts.noReasoning ? { body: {}, headroom: reasoningParams(cfg).headroom } : reasoningParams(cfg);
  const tokens = Math.min(16384, maxTokens + r.headroom);
  if (cfg.provider === 'anthropic') {
    const headers: Record<string, string> = { 'content-type': 'application/json', 'anthropic-version': '2023-06-01', 'anthropic-dangerous-direct-browser-access': 'true', ...extra };
    if (cfg.key) headers['x-api-key'] = cfg.key;
    return { url: endpoint(cfg), init: { method: 'POST', headers, body: JSON.stringify({ model: cfg.model, max_tokens: tokens, temperature: 0.3, system, messages: [{ role: 'user', content: user }] }) } };
  }
  const headers: Record<string, string> = { 'content-type': 'application/json', ...extra };
  if (cfg.key) headers.authorization = `Bearer ${cfg.key}`;
  const messages = [{ role: 'system', content: system }, { role: 'user', content: user }];
  // OpenAI reasoning models on the OpenAI API: max_completion_tokens, default temperature only.
  const body = !isOpenRouter(cfg) && isOpenAiReasoningModel(cfg.model)
    ? { model: cfg.model, messages, max_completion_tokens: tokens, ...r.body }
    : { model: cfg.model, messages, max_tokens: tokens, temperature: 0.3, ...r.body };
  return { url: endpoint(cfg), init: { method: 'POST', headers, body: JSON.stringify(body) } };
}
/** Token usage of a reply (OpenAI/OpenRouter `usage`, Anthropic `usage`). */
export interface TokenUsage { prompt: number | null; completion: number | null; reasoning: number | null }
export function usageOf(data: unknown): TokenUsage | null {
  const u = (data as { usage?: Record<string, any> } | null)?.usage; // eslint-disable-line @typescript-eslint/no-explicit-any
  if (!u || typeof u !== 'object') return null;
  const n = (v: unknown) => typeof v === 'number' && Number.isFinite(v) ? v : null;
  return { prompt: n(u.prompt_tokens) ?? n(u.input_tokens), completion: n(u.completion_tokens) ?? n(u.output_tokens),
    reasoning: n(u.completion_tokens_details?.reasoning_tokens) ?? n(u.output_tokens_details?.reasoning_tokens) ?? n(u.reasoning_tokens) };
}
/** Text of content parts in any of the shapes gateways use ({type:'text'|'output_text', text}). */
function partsText(content: unknown): string | null {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return null;
  const t = content.map((c: unknown) => typeof c === 'string' ? c : (c as { text?: unknown })?.text).filter((x): x is string => typeof x === 'string').join('');
  return t || null;
}
/** The model's answer text. Besides the two documented shapes it accepts what OpenAI-compatible
 *  gateways actually send: content-part arrays, legacy `choices[0].text`, the Responses API
 *  (`output_text`, `output[].content[]`), Ollama's native `message.content`, an Anthropic-shaped
 *  reply behind an OpenAI URL, and reasoning models that put the answer in `reasoning_content`.
 *  When nothing is found the error says why (finish_reason, refusal, empty content) and shows the
 *  start of the reply; the whole reply is kept in the AI exchange log for the recording export. */
export function extractText(provider: AiProvider, data: unknown): string {
  const d = data as Record<string, any> | null; // eslint-disable-line @typescript-eslint/no-explicit-any
  if (!d || typeof d !== 'object') throw new Error('Gateway reply is not a JSON object');
  const snippet = () => ` Reply starts: ${JSON.stringify(d).slice(0, 300)}`;
  if (d.error) throw new Error(`Gateway returned an error: ${JSON.stringify(d.error).slice(0, 300)}`);
  const anthropic = Array.isArray(d.content) ? partsText(d.content.filter((c: { type?: string }) => !c?.type || c.type === 'text')) : null;
  if (provider === 'anthropic') {
    if (anthropic != null) return anthropic;
    if (d.stop_reason === 'max_tokens') throw new Error(`The model stopped at max_tokens before writing any text; raise "Max tokens".${snippet()}`);
    throw new Error(`Gateway reply has no "content" text.${snippet()}`);
  }
  const choice = Array.isArray(d.choices) ? d.choices[0] : undefined;
  const message = choice?.message ?? choice?.delta;
  const direct = partsText(message?.content) ?? (typeof choice?.text === 'string' ? choice.text : null)
    ?? (typeof d.output_text === 'string' ? d.output_text : null)
    ?? (Array.isArray(d.output) ? partsText(d.output.flatMap((o: { content?: unknown }) => Array.isArray(o?.content) ? o.content : [])) : null)
    ?? partsText(d.message?.content) ?? anthropic;
  if (direct != null && direct.trim()) return direct;
  // Reasoning models (DeepSeek-R1, o-series via proxies, Qwen) can leave content empty.
  const reasoning = [message?.reasoning_content, message?.reasoning, message?.thinking].find((x): x is string => typeof x === 'string' && x.trim().length > 0);
  const finish = choice?.finish_reason ?? d.stop_reason ?? d.status;
  if (reasoning && /"speak"\s*:/.test(reasoning)) return reasoning;
  if (message?.refusal) throw new Error(`The model refused: ${String(message.refusal).slice(0, 200)}`);
  if (finish === 'length' || finish === 'max_tokens' || finish === 'incomplete')
    throw new Error(`The model used all ${reasoning ? 'max tokens on reasoning' : 'max tokens'} before answering (finish_reason "${finish}"). Raise "Max tokens" in Settings${reasoning ? ' or turn reasoning off for this model' : ''}.${snippet()}`);
  if (!choice) throw new Error(`Gateway reply has no "choices" array: this does not look like a chat-completions answer (check the base URL and provider).${snippet()}`);
  throw new Error(`Gateway reply has an empty choices[0].message.content (finish_reason "${finish ?? 'none'}").${snippet()}`);
}

// ---------------------------------------------------------------------------------------
// Exchange log: every gateway call with the whole reply, for diagnosis and recording exports.
// Keys are redacted; the request body is kept as text (prompt only, never headers).
// ---------------------------------------------------------------------------------------
export interface AiExchange {
  t: number; clock: number | null; trigger: string; url: string; model: string; status: number | null; ms: number;
  /** The system prompt length and the full user prompt (game summary, no secrets). */
  system_chars: number; prompt: string;
  /** The whole reply body as received (redacted, truncated to 20,000 characters). */
  reply: string; text: string | null; error: string | null;
  /** Token usage from the reply (reasoning tokens included when the gateway reports them). */
  usage?: TokenUsage | null;
  /** max_tokens sent and how reasoning was requested (Settings › Model reasoning). */
  max_tokens?: number; reasoning?: string;
}
const EXCHANGE_CAP = 60, REPLY_CAP = 20000;
const exchanges: AiExchange[] = [];
const exchangeListeners: ((x: AiExchange) => void)[] = [];
/** The AI exchange log (newest last). */
export function aiExchanges(): AiExchange[] { return exchanges.slice(); }
export function clearAiExchanges() { exchanges.length = 0; }
/** Restore a persisted log (e.g. after a reload while recording). */
export function restoreAiExchanges(xs: unknown) { if (Array.isArray(xs)) { exchanges.length = 0; exchanges.push(...(xs as AiExchange[]).slice(-EXCHANGE_CAP)); } }
/** Subscribe to new exchanges; returns an unsubscribe function. */
export function onAiExchange(fn: (x: AiExchange) => void): () => void { exchangeListeners.push(fn); return () => { const i = exchangeListeners.indexOf(fn); if (i >= 0) exchangeListeners.splice(i, 1); }; }
function logExchange(x: AiExchange) { exchanges.push(x); if (exchanges.length > EXCHANGE_CAP) exchanges.splice(0, exchanges.length - EXCHANGE_CAP); for (const fn of exchangeListeners) { try { fn(x); } catch { /* a listener must not break the call */ } } }

/** Remove secrets from text that may be shown to the user (gateways sometimes echo keys). */
export function redact(text: string, secrets: string[] = []): string {
  let out = text;
  for (const secret of secrets) if (secret && secret.length >= 4) out = out.split(secret).join('[redacted]');
  return out.replace(/\b(sk-[A-Za-z0-9_-]{6,}|sk_[A-Za-z0-9_-]{6,}|Bearer\s+[A-Za-z0-9._~+/-]{8,})/g, '[redacted]');
}
function secretsOf(cfg: AiConfig): string[] {
  try { return [cfg.key, ...Object.values(parseExtraHeaders(cfg.headers))]; } catch { return [cfg.key]; }
}

export type GatewayErrorKind = 'config' | 'network' | 'http' | 'timeout' | 'aborted' | 'reply' | 'truncated';
export class GatewayError extends Error {
  kind: GatewayErrorKind; status?: number;
  constructor(kind: GatewayErrorKind, message: string, status?: number) { super(message); this.kind = kind; this.status = status; this.name = 'GatewayError'; }
}
export function validateConfig(cfg: AiConfig, pageProtocol = 'https:'): void {
  let url: URL;
  try { url = new URL(endpoint(cfg)); } catch { throw new GatewayError('config', 'Base URL is not a valid URL.'); }
  if (!['http:', 'https:'].includes(url.protocol)) throw new GatewayError('config', 'Base URL must start with https:// (or http:// for localhost).');
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (url.protocol === 'http:' && pageProtocol === 'https:' && !local) throw new GatewayError('config', 'This page is served over HTTPS, so the browser blocks http:// gateways other than localhost. Use an https:// URL.');
  if (!cfg.model.trim()) throw new GatewayError('config', 'Enter a model name.');
  if (cfg.provider === 'anthropic' && !cfg.key) throw new GatewayError('config', 'Anthropic requires an API key.');
  try { parseExtraHeaders(cfg.headers); } catch (e) { throw new GatewayError('config', (e as Error).message); }
}

export interface CallOptions { signal?: AbortSignal; maxTokens?: number; timeoutMs?: number; fetchImpl?: typeof fetch; pageProtocol?: string; now?: () => number;
  /** For the exchange log: what triggered the call and the game clock. */
  trigger?: string; clock?: number | null }
export interface CallResult { text: string; ms: number; model: string;
  /** Token usage reported by the gateway (null when it reports none). */
  usage?: TokenUsage | null }
export async function callGateway(cfg: AiConfig, system: string, user: string, opts: CallOptions = {}): Promise<CallResult> {
  validateConfig(cfg, opts.pageProtocol);
  const now = opts.now || (() => Date.now()), started = now();
  const secrets = secretsOf(cfg);
  let req = buildRequest(cfg, system, user, opts.maxTokens);
  const sentTokens = () => { try { const b = JSON.parse(req.init.body); return b.max_tokens ?? b.max_completion_tokens; } catch { return undefined; } };
  const entry: AiExchange = { t: Date.now(), clock: opts.clock ?? null, trigger: opts.trigger || 'call', url: redact(req.url, secrets), model: cfg.model, status: null, ms: 0,
    system_chars: system.length, prompt: redact(user, secrets), reply: '', text: null, error: null, max_tokens: sentTokens(), reasoning: reasoningParams(cfg).label };
  try {
    let r: CallResult;
    try { r = await callGatewayInner(cfg, req, opts, secrets, entry, now, started); }
    catch (e) {
      // A gateway or model that rejects the reasoning field (HTTP 400 naming it): once more without it.
      const sent = Object.keys(reasoningParams(cfg).body).length > 0;
      if (!(sent && e instanceof GatewayError && e.kind === 'http' && e.status === 400 && /reason/i.test(e.message))) throw e;
      req = buildRequest(cfg, system, user, opts.maxTokens, { noReasoning: true }); entry.reasoning = `${entry.reasoning}; rejected by the gateway, sent without it`;
      r = await callGatewayInner(cfg, req, opts, secrets, entry, now, started);
    }
    entry.text = r.text; entry.model = r.model; entry.usage = r.usage ?? null; return r;
  }
  catch (e) { entry.error = e instanceof Error ? e.message : String(e); throw e; }
  finally { entry.ms = Math.round(now() - started); logExchange(entry); }
}
async function callGatewayInner(cfg: AiConfig, req: GatewayRequest, opts: CallOptions, secrets: string[], entry: AiExchange, now: () => number, started: number): Promise<CallResult> {
  const ctrl = new AbortController(); let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; ctrl.abort(); }, opts.timeoutMs ?? 20000);
  const onAbort = () => ctrl.abort();
  opts.signal?.addEventListener('abort', onAbort, { once: true });
  if (opts.signal?.aborted) ctrl.abort();
  try {
    let res: Response;
    try { res = await (opts.fetchImpl || fetch)(req.url, { ...req.init, signal: ctrl.signal, credentials: 'omit', referrerPolicy: 'no-referrer' }); }
    catch (e) {
      if (timedOut) throw new GatewayError('timeout', `No answer within ${Math.round((opts.timeoutMs ?? 20000) / 1000)} s.`);
      if (opts.signal?.aborted) throw new GatewayError('aborted', 'Request canceled.');
      let origin = req.url; try { origin = new URL(req.url).origin; } catch { /* keep */ }
      throw new GatewayError('network', redact(`Network or CORS failure reaching ${origin}: the browser could not complete the request. Check the URL and that the gateway allows CORS from this page (preflight with Authorization/x-api-key/Content-Type headers). Browser detail: ${e instanceof Error ? e.message : String(e)}`, secrets));
    }
    const body = await res.text().catch(() => '');
    entry.status = res.status; entry.reply = redact(body, secrets).slice(0, REPLY_CAP);
    if (!res.ok) throw new GatewayError('http', redact(`HTTP ${res.status}${res.statusText ? ` ${res.statusText}` : ''}: ${body.replace(/\s+/g, ' ').slice(0, 300) || '(empty body)'}`, secrets), res.status);
    let data: unknown;
    try { data = JSON.parse(body); } catch { throw new GatewayError('reply', redact(`Gateway answered HTTP ${res.status} but not JSON: ${body.slice(0, 160)}`, secrets)); }
    const usage = usageOf(data); entry.usage = usage;
    const limit = (() => { try { const b = JSON.parse(req.init.body); return b.max_tokens ?? b.max_completion_tokens ?? opts.maxTokens ?? cfg.maxTokens; } catch { return opts.maxTokens ?? cfg.maxTokens; } })();
    const thought = usage?.reasoning ? ` ${usage.reasoning} of them on reasoning;` : '';
    let text: string;
    try { text = extractText(cfg.provider, data); } catch (e) {
      // Reasoning ate the whole budget: report it as a cut-off so the controller retries with more room.
      const fin = (data as Record<string, any>)?.choices?.[0]?.finish_reason; // eslint-disable-line @typescript-eslint/no-explicit-any
      if (fin === 'length' && usage?.reasoning) throw new GatewayError('truncated', `The model spent ${usage.reasoning} of ${limit} tokens on reasoning and wrote no answer (finish_reason "length"). Set "Model reasoning" to Off or Low, or raise "Max reply tokens".`);
      throw new GatewayError('reply', redact((e as Error).message, secrets));
    }
    // A reply cut off by the token limit (OpenRouter + Claude Haiku at 200 tokens stopped mid-JSON).
    { const dd = data as Record<string, any>; const fin = dd?.choices?.[0]?.finish_reason ?? dd?.stop_reason ?? dd?.incomplete_details?.reason; // eslint-disable-line @typescript-eslint/no-explicit-any
      if ((fin === 'length' || fin === 'max_tokens' || fin === 'max_output_tokens') && parseAdvice(text).ok === false)
        throw new GatewayError('truncated', `The reply was cut off at the ${limit}-token limit (finish_reason "${fin}";${thought} raise "Max reply tokens" or lower "Model reasoning" in Settings).`); }
    const model = typeof (data as { model?: unknown }).model === 'string' ? (data as { model: string }).model : cfg.model;
    return { text, ms: Math.round(now() - started), model, usage };
  } finally { clearTimeout(timer); opts.signal?.removeEventListener('abort', onAbort); }
}

// ---------------------------------------------------------------------------------------
// Reply parsing
// ---------------------------------------------------------------------------------------
export interface Advice { speak: string | null; priority: Priority; why: string; focus: string;
  /** The PLAYER PROFILE weakness the model says it coached (only set when it named one). */
  weakness?: string | null }
export type ParseResult = { ok: true; advice: Advice } | { ok: false; error: string };
/** Balanced {...} substrings of a reply (exported for the end-of-game review parser, ai-review.ts). */
export function jsonCandidates(text: string): string[] {
  const out: string[] = [];
  for (let start = text.indexOf('{'); start >= 0 && out.length < 8; start = text.indexOf('{', start + 1)) {
    let depth = 0, inString = false, escaped = false;
    for (let i = start; i < text.length; i++) {
      const c = text[i];
      if (inString) { if (escaped) escaped = false; else if (c === '\\') escaped = true; else if (c === '"') inString = false; continue; }
      if (c === '"') inString = true; else if (c === '{') depth++; else if (c === '}' && --depth === 0) { out.push(text.slice(start, i + 1)); break; }
    }
  }
  return out;
}
/** A sentence that tells the player to do something (starts with, or contains after a colon or
 *  "so", an imperative). */
const ACTION_RE = /(?:^|[:;,]\s*|\bso\s+|\bthen\s+)(?:don't|do not|never|stay|hold|walk|fall|go|buy|push|take|group|back|wait|check|spend|heal|rotate|farm|clear|save|use|keep|move|retreat|leave|join|play|watch|avoid|contest|kill|focus|shoot|dash|last-hit|hit|secure|ping|look|sell|rush|trade|commit|disengage|step|follow|stick|regroup|rejoin|gather|rotate|help|peel|protect|defend|escort|deliver|grab|pick|kite|poke|bait|cleanse|reposition|reset|return|sit|shove|fight|flank|finish|chase|jump|zip|ult|buy|swap|stop|let|make|get|leave)\b/i;
/** Make advice speakable and enforce the word/sentence limit. */
export function cleanSpeech(text: string, maxWords = 30, maxSentences = 2): string {
  const clean = text.replace(/[*_#`~>|]/g, '').replace(/\s+/g, ' ').trim();
  const sentences = clean.match(/[^.!?]+[.!?]+(?=\s|$)|[^.!?]+$/g)?.map(s => s.trim()).filter(Boolean) || [];
  let out = '', words = 0;
  const picked: string[] = [];
  for (const s of sentences.slice(0, maxSentences)) {
    const w = s.split(' ').length;
    if (words + w > maxWords) {
      if (!out) { out = s.split(' ').slice(0, maxWords).join(' ').replace(/[,;:]$/, '') + '.'; picked.push(out); }
      // Over the limit: keep the sentence that carries the action rather than a bare situation
      // (rec11 spoke only "Calico is at 18% and melee range is how you've died most.").
      else if (picked.length === 1 && !ACTION_RE.test(picked[0]) && ACTION_RE.test(s) && w <= maxWords) out = s;
      break;
    }
    out = out ? `${out} ${s}` : s; words += w; picked.push(s);
  }
  return out;
}
export function parseAdvice(raw: string, maxWords = 30, maxSentences = 2): ParseResult {
  const text = raw.replace(/<think>[\s\S]*?<\/think>/gi, '').replace(/```(?:json)?/gi, '').trim();
  if (/^["']?PASS["']?\.?$/i.test(text)) return { ok: true, advice: { speak: null, priority: 'Low', why: 'Model passed', focus: 'pass' } };
  let obj: Record<string, unknown> | undefined;
  for (const candidate of jsonCandidates(text)) {
    try { const v = JSON.parse(candidate); if (v && typeof v === 'object' && !Array.isArray(v) && ('speak' in v)) { obj = v; break; } } catch { /* try the next candidate */ }
  }
  if (!obj) return { ok: false, error: `Reply was not the requested JSON: "${text.replace(/\s+/g, ' ').slice(0, 120)}"` };
  if (obj.speak != null && typeof obj.speak !== 'string') return { ok: false, error: '"speak" must be a string or null' };
  let speak = typeof obj.speak === 'string' ? obj.speak.trim() : null;
  if (speak && /^PASS\.?$/i.test(speak)) speak = null;
  if (speak) speak = cleanSpeech(speak, maxWords, maxSentences) || null;
  const priority: Priority = obj.priority === 'High' || obj.priority === 'Low' || obj.priority === 'Normal' ? obj.priority
    : typeof obj.priority === 'string' && ['high', 'normal', 'low'].includes(obj.priority.toLowerCase()) ? (obj.priority[0].toUpperCase() + obj.priority.slice(1).toLowerCase()) as Priority : 'Normal';
  const why = typeof obj.why === 'string' ? obj.why.replace(/\s+/g, ' ').trim().slice(0, 220) : '';
  const focus = (typeof obj.focus === 'string' ? obj.focus : '').toLowerCase().replace(/[^a-z0-9 _-]/g, '').trim().slice(0, 24) || 'general';
  const weakness = typeof obj.weakness === 'string' && obj.weakness.trim() && !/^(null|none|n\/a)$/i.test(obj.weakness.trim()) ? obj.weakness.replace(/\s+/g, ' ').trim().slice(0, 60) : null;
  return { ok: true, advice: weakness ? { speak, priority, why, focus, weakness } : { speak, priority, why, focus } };
}

// ---------------------------------------------------------------------------------------
// Player-profile hook (implemented by the player-profile module)
// ---------------------------------------------------------------------------------------
let profileProvider: (() => string | null) | undefined;
/** Register a callback returning a short text summary of the player's profile (or null). */
export function setProfileProvider(fn: (() => string | null) | undefined) { profileProvider = fn; }
export function profileText(): string | null {
  try { const v = profileProvider?.(); return typeof v === 'string' && v.trim() ? v.trim().slice(0, PROFILE_MAX_CHARS) : null; } catch { return null; }
}

// ---------------------------------------------------------------------------------------
// Controller
// ---------------------------------------------------------------------------------------
export interface AdviceMeta { trigger: AiTrigger; ms: number; model: string; tokens: number; live: boolean; clock: number | null;
  /** What changed to trigger the request (periodic) and the reasoning tokens the reply used. */
  reason?: string | null; reasoningTokens?: number | null }
export interface AiStatus {
  state: 'off' | 'idle' | 'thinking' | 'error'; message: string;
  last?: { advice: Advice; meta: AdviceMeta }; error?: string; retryAt?: number;
  requests: number; failures: number; stale: number; lastMs?: number;
  /** Answers not spoken because checkAdvice() rejected them (repeat, theme memory, souls gate). */
  suppressed?: number;
  /** Reasoning tokens: last reply and the total over replies that reported them. */
  reasoningLast?: number | null; reasoningTotal?: number; reasoningReplies?: number;
  /** Periodic requests by cause: event-driven vs. the quiet interval. */
  eventRequests?: number; quietRequests?: number;
}
export interface AiCoachDeps {
  settings: () => Settings; isLive: () => boolean; generation: () => number;
  knowledge: () => Promise<Knowledge[]>; name?: (id: string) => string;
  /** Item names, costs and slots for the summary (optional). */
  items?: (id: string) => ItemInfo | null;
  onAdvice: (cue: Cue, advice: Advice, meta: AdviceMeta) => void; onStatus?: (s: AiStatus) => void;
  now?: () => number; fetchImpl?: typeof fetch; pageProtocol?: () => string;
}
const EVENT_TEXT: Record<string, string> = { phase: 'new phase', respawn: 'respawn', 'enemy-window': 'enemy-dead window', 'objective-soon': 'objective within 60 s', 'souls-tier': 'souls reach the usual buy', 'nw-swing': 'big team souls swing', good: 'good play (see GOOD PLAYS)' };
/** With a long respawn timer the death review waits up to this long for the rule death review. */
export const DEATH_REVIEW_WAIT_MS = 15000;
export class AiCoach {
  readonly history = new AiHistory();
  status: AiStatus = { state: 'off', message: 'AI coach is off.', requests: 0, failures: 0, stale: 0 };
  private deps: AiCoachDeps; private inFlight?: { ctrl: AbortController; trigger: AiTrigger };
  private consecutive = 0; private backoffUntil = 0; private lastPeriodicAt = 0; private lastHighAt = -Infinity; private deathAsked = 0;
  /** Team souls difference at the last periodic request (big-swing trigger). */
  private nwAtLast: number | null = null;
  /** AiHistory.match seen last (a change = a new match). */
  private matchSeen = 0;
  private latest: AiSnapshot = {}; private sectionCache?: { docs: Knowledge[]; sections: KnowledgeSection[] };
  constructor(deps: AiCoachDeps) { this.deps = deps; if (deps.name) this.history.name = deps.name; this.refresh(); }
  private now() { return (this.deps.now || Date.now)(); }
  private set(patch: Partial<AiStatus>) { this.status = { ...this.status, ...patch }; this.deps.onStatus?.(this.status); }
  get busy() { return !!this.inFlight; }
  /** Re-evaluate the idle/off state after settings change. */
  refresh() {
    if (this.inFlight) return;
    const s = this.deps.settings();
    if (!s.aiEnabled) this.set({ state: 'off', message: 'AI coach is off. Configure a gateway in Settings to enable it.' });
    else if (this.status.state === 'off') this.set({ state: 'idle', message: `Ready · ${s.aiModel || 'no model set'}` });
  }
  /** What changed since the last periodic request (fresh events and a big team souls swing), as a
   *  WHY NOW text, or null when nothing meaningful changed. Pure apart from reading state. */
  pendingReason(now = this.now()): string | null {
    const fresh = this.history.events.filter(e => e.t > this.lastPeriodicAt && now - e.t <= EVENT_FRESH_SECS * 1000);
    const parts = fresh.map(e => `${EVENT_TEXT[e.kind] || e.kind}: ${e.detail}`);
    const d = this.history.samples.at(-1)?.nwDiff, s = this.latest;
    // Big = 4,000 or 5% of the average team souls, whichever is larger (lategame totals are 200k+).
    const scale = num(s.ally_networth) != null && num(s.enemy_networth) != null ? .05 * (s.ally_networth! + s.enemy_networth!) / 2 : 0;
    if (d != null && this.nwAtLast != null && Math.abs(d - this.nwAtLast) >= Math.max(NW_SWING, scale))
      parts.push(`${EVENT_TEXT['nw-swing']}: team souls difference went from ${this.nwAtLast >= 0 ? '+' : ''}${fmt(this.nwAtLast)} to ${d >= 0 ? '+' : ''}${fmt(d)}`);
    return parts.length ? [...new Set(parts)].slice(0, 3).join('; ') : null;
  }
  /** Feed every analyzed snapshot (live or replay). Triggers only fire when `live`. */
  observe(s: AiSnapshot, cues: Cue[], live: boolean) {
    const now = this.now(); this.latest = s; this.history.push(s, cues, now);
    // A new match (AiHistory drew a boundary): the controller's own memory goes too.
    if (this.history.match !== this.matchSeen) {
      this.matchSeen = this.history.match; this.cancel();
      this.lastPeriodicAt = 0; this.deathAsked = 0; this.lastHighAt = -Infinity; this.nwAtLast = null;
    }
    if (cues.some(c => c.priority === 'High' && !c.key.startsWith('ai:'))) this.lastHighAt = now;
    if (!live || !this.deps.settings().aiEnabled) return;
    if (isDead(s)) {
      const death = this.history.deaths.at(-1);
      if (!death || this.history.deathCount === this.deathAsked || now < this.backoffUntil) return;
      // Wait for the rule coach's death review of this death (it names the killer hero and crowd
      // control): 4 s after the death panel was read with a short respawn, up to 15 s with a long
      // one, 8 s at most without a panel.
      const panel = !!(death.cause || death.damage.length || death.killingBlow);
      const reviewed = this.history.reviewsFor(death, /^death:review/).length > 0;
      const long = (death.respawn ?? 0) >= 25;
      const ready = reviewed || (panel && now - death.t >= (long ? DEATH_REVIEW_WAIT_MS : DEATH_WAIT_MS)) || now - death.t >= (long ? DEATH_REVIEW_WAIT_MS : DEATH_WAIT_MAX_MS);
      if (!ready) return;
      if (this.inFlight?.trigger === 'periodic') this.cancel();
      if (this.inFlight) return;
      this.deathAsked = this.history.deathCount; void this.ask('death');
      return;
    }
    if (s.screen !== 'Gameplay') return;
    if (!this.lastPeriodicAt) { this.lastPeriodicAt = now; this.nwAtLast = this.history.samples.at(-1)?.nwDiff ?? null; return; }
    if (now - this.lastHighAt < 10000 || now < this.backoffUntil || this.inFlight) return;
    // Adaptive cadence: a meaningful change asks sooner (at most every EVENT_GAP_SECS); otherwise
    // one quiet check per interval (default 90 s), which the model is told it may PASS.
    const since = now - this.lastPeriodicAt;
    const interval = Math.max(15, this.deps.settings().aiInterval) * 1000;
    // The gap also counts from the last spoken AI advice (a death review already coached the respawn).
    const spokeAgo = now - (this.history.advice.at(-1)?.t ?? -Infinity);
    const reason = Math.min(since, spokeAgo) >= Math.min(EVENT_GAP_SECS * 1000, interval) ? this.pendingReason(now) : null;
    if (reason) { this.markPeriodic(now, 'event'); void this.ask('periodic', reason); return; }
    if (since < interval) return;
    const last = this.history.advice.at(-1);
    this.markPeriodic(now, 'quiet');
    void this.ask('periodic', `routine check, nothing new since ${last ? `your last call at ${clockText(last.clock)}` : `${Math.round(since / 1000)} s ago`}: PASS unless you see a NEW, specific insight`);
  }
  private markPeriodic(now: number, kind: 'event' | 'quiet') {
    this.lastPeriodicAt = now; this.nwAtLast = this.history.samples.at(-1)?.nwDiff ?? this.nwAtLast;
    if (kind === 'event') this.status.eventRequests = (this.status.eventRequests || 0) + 1; else this.status.quietRequests = (this.status.quietRequests || 0) + 1;
  }
  cancel() { this.inFlight?.ctrl.abort(); this.inFlight = undefined; if (this.status.state === 'thinking') this.set({ state: this.deps.settings().aiEnabled ? 'idle' : 'off', message: 'Request canceled.' }); }
  reset() { this.cancel(); this.history.reset(); this.latest = {}; this.lastPeriodicAt = 0; this.deathAsked = 0; this.lastHighAt = -Infinity; this.nwAtLast = null; }
  /** Prompt for the current state (exposed for diagnostics/tests). */
  async prompt(trigger: AiTrigger, retry?: PromptInput['retry'], reason?: string | null): Promise<Prompt> {
    const docs = await this.deps.knowledge().catch(() => [] as Knowledge[]);
    if (this.sectionCache?.docs !== docs) this.sectionCache = { docs, sections: sections(docs) };
    const name = this.deps.name || defaultName, s = this.latest, profile = profileText();
    const summary = buildSummary({ snapshot: s, history: this.history, now: this.now(), trigger, name, profile, reason, items: this.deps.items });
    // Midboss excerpts only when the situation asks for them (its icon is up almost all game).
    const terms = situationTerms(s, this.history, trigger);
    const excerpts = retrieve(terms.includes('midboss') ? this.sectionCache.sections : this.sectionCache.sections.filter(x => !/midboss/i.test(x.heading)), terms);
    const enemies = s.enemy_heroes?.length ? s.enemy_heroes : (s.hero_status || []).filter(p => !p.ally && p.hero).map(p => p.hero!);
    return buildPrompt({ trigger, summary, profile, excerpts, matchup: heroMatchup(docs, s.hero, enemies, name), retry });
  }
  /** Ask now. `reason` is the WHY NOW line (what changed) for periodic requests. */
  async ask(trigger: AiTrigger, reason?: string | null): Promise<void> {
    const settings = this.deps.settings();
    if (!settings.aiEnabled) { this.set({ state: 'off', message: 'AI coach is off. Turn it on in Settings and configure a gateway.' }); return; }
    if (this.inFlight) { if (trigger === 'manual') this.set({ message: 'A request is already in flight; wait for it to finish.' }); return; }
    // Nothing read yet (rec11: "Manual trigger with no clock"): do not spend a request.
    const s = this.latest;
    if (trigger === 'manual' && num(s.game_clock_secs) == null && !s.hero && !this.history.hero && num(s.hp) == null && !isDead(s)) {
      this.set({ state: 'idle', message: 'No match state read yet: start coaching or analyze a replay, then ask again.' }); return;
    }
    const ctrl = new AbortController(); this.inFlight = { ctrl, trigger };
    const generation = this.deps.generation(), started = this.now();
    this.set({ state: 'thinking', message: `Asking ${settings.aiModel || 'the model'} (${trigger})…`, requests: this.status.requests + 1 });
    try {
      let retry: PromptInput['retry'] = null;
      let truncated = false;
      for (let attempt = 0; attempt < 2; attempt++) {
        const prompt = await this.prompt(trigger, retry, trigger === 'periodic' ? reason : null);
        if (ctrl.signal.aborted) return;
        const base = trigger === 'death' ? Math.round(settings.aiMaxTokens * 1.5) : settings.aiMaxTokens;
        const maxTokens = Math.min(8192, base * (truncated ? 2 : 1));
        let res: CallResult;
        try { res = await callGateway(configFrom(settings), prompt.system, prompt.user, { signal: ctrl.signal, maxTokens, timeoutMs: trigger === 'manual' ? 45000 : STALE_MS,
          fetchImpl: this.deps.fetchImpl, pageProtocol: this.deps.pageProtocol?.(), now: this.deps.now, trigger: attempt ? `${trigger}:retry` : trigger, clock: num(this.latest.game_clock_secs) }); }
        catch (err) {
          // Cut off by the token limit: one retry with double the limit, then report it.
          if (err instanceof GatewayError && err.kind === 'truncated' && attempt === 0 && !truncated) { truncated = true; continue; }
          throw err;
        }
        if (ctrl.signal.aborted) return;
        const rt = res.usage?.reasoning ?? null;
        if (rt != null) this.set({ reasoningLast: rt, reasoningTotal: (this.status.reasoningTotal || 0) + rt, reasoningReplies: (this.status.reasoningReplies || 0) + 1 });
        const age = this.now() - started, sameGen = generation === this.deps.generation();
        if (!sameGen || (trigger !== 'manual' && (age > STALE_MS || !this.deps.isLive()))) {
          this.consecutive = 0; this.backoffUntil = 0;
          this.set({ state: 'idle', message: `Dropped a stale answer (${Math.round(age / 1000)} s old).`, stale: this.status.stale + 1, lastMs: res.ms }); return;
        }
        const parsed = parseAdvice(res.text, trigger === 'death' ? 45 : 30, trigger === 'death' ? 3 : 2);
        if (!parsed.ok) throw new GatewayError('reply', redact(parsed.error, secretsOf(configFrom(settings))));
        this.consecutive = 0; this.backoffUntil = 0;
        const meta: AdviceMeta = { trigger, ms: res.ms, model: res.model, tokens: prompt.tokens, live: this.deps.isLive() && sameGen, clock: num(this.latest.game_clock_secs) ?? this.history.clock,
          reason: trigger === 'periodic' ? reason ?? null : null, reasoningTokens: rt };
        // Quality gate: souls only when the souls gate is open, theme memory, no near-duplicates.
        const verdict = parsed.advice.speak ? checkAdvice(parsed.advice, trigger, soulsView(this.latest, this.history, this.now()), this.history, this.now()) : null;
        if (verdict && !verdict.ok) {
          this.set({ state: 'idle', message: `Skipped a repeat: ${verdict.reason}.`, suppressed: (this.status.suppressed || 0) + 1, lastMs: res.ms, error: undefined, retryAt: undefined });
          // Ask once for an alternative only when it is cheap (a fast answer, still fresh).
          if (attempt === 0 && trigger !== 'manual' && res.ms <= RETRY_MAX_MS && this.now() - started + res.ms < STALE_MS) { retry = { reason: verdict.reason, text: parsed.advice.speak! }; continue; }
          return;
        }
        const advice: Advice = verdict?.ok ? { ...parsed.advice, speak: verdict.speak, focus: verdict.topic } : parsed.advice;
        this.set({ state: 'idle', message: advice.speak ? `Advice received.${verdict?.ok && verdict.note ? ` (${verdict.note})` : ''}` : 'Model passed: nothing more important than the current calls.', last: { advice, meta }, error: undefined, retryAt: undefined, lastMs: res.ms });
        if (advice.speak) {
          this.history.noteAdvice({ t: this.now(), clock: meta.clock, trigger, topic: advice.focus, text: advice.speak, weakness: advice.weakness ?? weaknessOf(advice.speak, profileText()) });
          if (verdict?.ok && verdict.praised) this.history.praised.push(verdict.praised);
          // AI advice never outranks the rule engine's urgent safety calls in the speech queue.
          const cue: Cue = { key: `ai:${trigger}:${advice.focus}`, text: advice.speak, priority: advice.priority === 'High' ? 'Normal' : advice.priority, cooldown_secs: trigger === 'periodic' ? 30 : 0 };
          this.deps.onAdvice(cue, advice, meta);
        }
        return;
      }
    } catch (e) {
      if (ctrl.signal.aborted && !(e instanceof GatewayError && e.kind === 'timeout')) return;
      const message = e instanceof Error ? e.message : String(e);
      if (e instanceof GatewayError && e.kind === 'config') { this.set({ state: 'error', message: 'Configuration problem.', error: message, failures: this.status.failures + 1 }); return; }
      this.consecutive++;
      const wait = Math.min(300000, 5000 * 2 ** (this.consecutive - 1)); this.backoffUntil = this.now() + wait;
      this.set({ state: 'error', message: `Request failed; automatic requests resume in ${Math.round(wait / 1000)} s.`, error: message, retryAt: this.backoffUntil, failures: this.status.failures + 1 });
    } finally { if (this.inFlight?.ctrl === ctrl) this.inFlight = undefined; }
  }
  /** Short line for the runtime diagnostics panel. Never includes the key. */
  diagnostic(): string {
    const s = this.deps.settings();
    if (!s.aiEnabled) return 'Off';
    let host = s.aiBaseUrl; try { host = new URL(endpoint(configFrom(s))).host; } catch { /* keep raw */ }
    const cfg = configFrom(s);
    const parts = [`${s.aiProvider === 'anthropic' ? 'Anthropic' : 'OpenAI-compatible'} · ${host} · ${s.aiModel || 'no model'}`, `reasoning ${s.aiReasoning} (${reasoningParams(cfg).label}, max_tokens ${effectiveMaxTokens(cfg)})`,
      `on changes, or every ${s.aiInterval}s when quiet`, `${this.status.requests} requests`];
    if (this.status.eventRequests || this.status.quietRequests) parts.push(`${this.status.eventRequests || 0} on events / ${this.status.quietRequests || 0} quiet`);
    if (this.status.lastMs != null) parts.push(`last ${this.status.lastMs} ms`);
    if (this.status.reasoningReplies) parts.push(`reasoning tokens last ${this.status.reasoningLast ?? 0}, avg ${Math.round((this.status.reasoningTotal || 0) / this.status.reasoningReplies)}`);
    if (this.status.stale) parts.push(`${this.status.stale} stale dropped`);
    if (this.status.suppressed) parts.push(`${this.status.suppressed} repeats skipped`);
    if (this.status.failures) parts.push(`${this.status.failures} errors`);
    const b = this.history.boundaries.at(-1); if (b) parts.push(`new match detected ${this.history.match}× (last: ${b.reason})`);
    if (this.backoffUntil > this.now()) parts.push(`backing off ${Math.ceil((this.backoffUntil - this.now()) / 1000)} s`);
    return parts.join(' · ');
  }
}

/** Minimal connectivity check: one tiny request, success or the exact failure. */
export async function testConnection(cfg: AiConfig, opts: CallOptions = {}): Promise<{ ok: boolean; message: string }> {
  try {
    const res = await callGateway(cfg, 'You are a connectivity check. Reply with only this JSON: {"speak": null, "priority": "Low", "why": "connection test", "focus": "test"}', 'Connection test. Reply with the JSON only.', { ...opts, maxTokens: Math.min(64, cfg.maxTokens), timeoutMs: opts.timeoutMs ?? 30000 });
    const parsed = parseAdvice(res.text);
    return { ok: true, message: `Connected · ${res.model} · ${res.ms} ms${res.usage?.reasoning ? ` · ${res.usage.reasoning} reasoning tokens` : ''} · ${parsed.ok ? 'reply is valid coach JSON' : `reply received but not coach JSON: "${redact(res.text, secretsOf(cfg)).replace(/\s+/g, ' ').slice(0, 100)}"`}` };
  } catch (e) {
    const kind = e instanceof GatewayError ? e.kind : 'error';
    const label: Record<string, string> = { config: 'Configuration', network: 'Network/CORS', http: 'Gateway error', timeout: 'Timeout', aborted: 'Canceled', reply: 'Unexpected reply', truncated: 'Reply cut off', error: 'Error' };
    return { ok: false, message: `${label[kind] || 'Error'}: ${e instanceof Error ? e.message : String(e)}` };
  }
}
