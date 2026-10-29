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
// death with at least the death-spend threshold); an answer on the same topic as the previous AI
// call within 3 min, or near-identical to a recent AI or rule call, is not spoken.
import { deathSpend, spendTarget } from './player-profile.ts';
import type { Cue, Knowledge, Priority, Settings, Snapshot } from './types';

export type AiProvider = 'openai' | 'anthropic';
export type AiTrigger = 'periodic' | 'death' | 'manual';
export interface AiConfig { provider: AiProvider; baseUrl: string; model: string; key: string; maxTokens: number; headers: string }
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
export const estimateTokens = (chars: number) => Math.ceil(chars / 4);

export const PROVIDER_PRESETS: Record<AiProvider, { label: string; baseUrl: string; modelHint: string }> = {
  openai: { label: 'OpenAI-compatible (OpenAI, OpenRouter, LiteLLM, Azure proxy, Ollama, LM Studio)', baseUrl: 'https://api.openai.com/v1', modelHint: 'e.g. gpt-4o-mini · openai/gpt-4o-mini · llama3.1' },
  anthropic: { label: 'Anthropic Messages', baseUrl: 'https://api.anthropic.com', modelHint: 'e.g. claude-3-5-haiku-latest' },
};

// ---------------------------------------------------------------------------------------
// Prompt text
// ---------------------------------------------------------------------------------------
export const SYSTEM_PROMPT = `You are an elite Deadlock coach (Valve's 6v6 hero-shooter MOBA) speaking into the player's ear during a live match. A rule-based coach already speaks routine reminders (souls, timers, waves, low health); you add the review-style fix a top coach would give for THIS moment.
Rules:
1. Use ONLY the evidence in STATE, PLAYER PROFILE and KNOWLEDGE. Never invent enemy positions, cooldowns, items, health or timers. "unseen", "missing" or "unknown" means unknown, never safe.
2. Pick the single most impactful fix for the next 30 seconds. One idea only. Prefer what prevents deaths and wins fights: range and positioning against divers, healing before going back in, walking out with a teammate, using enemy-dead windows, objectives, map awareness.
3. When COACHING ANGLES shows the live situation matches a PLAYER PROFILE weakness, coach that weakness in this situation (just respawned: walk out with a teammate; under 60% health: heal first; enemy melee diver alive: hold max range).
4. Souls: mention souls, spending or buying ONLY if STATE says "Souls advice: allowed". Holding less than the usual buy for the game time is saving, not hoarding; never suggest a cheaper tier than the usual buy.
5. Variety: RECENT AI ADVICE is what you already said. Never use the same focus twice in a row within 3 minutes unless it is urgent and new. Do not repeat RECENT COACH CALLS unless you add a concrete how-to.
6. Be concrete and imperative: what to do, where and when (for example "Walk out with Bebop to Blue and stay behind him at max range."). No greetings, hedging or stat lectures.
7. Live advice: at most 2 short sentences and 30 words, natural for text-to-speech (no lists, symbols or markdown).
8. Be hero-specific when heroes are known, but never guess ability details you are not certain of.
9. If nothing important is happening, the evidence is too thin, or your only idea repeats a recent call, set "speak" to null (PASS).
Reply with ONLY one JSON object, no code fence, no other text:
{"speak": string or null, "priority": "High" | "Normal" | "Low", "why": "one line citing the evidence you used", "focus": "one of positioning, health, team, objective, wave, map, jungle, death, economy"}`;

export const TASKS: Record<AiTrigger, string> = {
  periodic: 'TASK: Give the one most useful call for right now, preferably a COACHING ANGLE, on a focus not in RECENT AI ADVICE from the last 3 minutes. PASS if the rule-based coach already covers it.',
  death: 'TASK: The player just died. Explain WHY from the death evidence (damage by source, killing blow, killer hero named by the rule death review, status effects or crowd control before death, health before, allies near), then give the ONE change for the next life, tied to a PLAYER PROFILE weakness when it matches. Talk about souls only if STATE says "Souls advice: allowed". Up to 3 short sentences, at most 45 words.',
  manual: 'TASK: The player pressed "Ask the coach now". Give the most useful call for this moment, preferably a COACHING ANGLE; only PASS if there is no usable evidence.',
};

/** Compact Deadlock facts written from docs/timings and docs/strategy (wiki, retrieved 2026-10-25). */
export const COACH_BRIEF = `DEADLOCK FACTS (wiki 2026-10; patches may change numbers):
- Souls are currency and XP. HUD souls = secured + unsecured (haunted). Haunted souls from camps, crates, Sinner's Sacrifices and the urn are lost on death (a Soul Bag drops only above 50 + 5/min). Secure or spend them before risky plays.
- Item tiers cost 800 / 1,600 / 3,200 / 6,400. Big investment spike at 4,800 in one category. Ultimate unlocks at 3,800 souls gathered. You can shop while dead; shops sit by each Guardian, in base and two secret shops.
- Hero kill bounty 200 + 50/min. Dying gives the enemy souls and costs farm time.
- Timers: easy camps 2:00; Powerups (bridge buffs, heavy melee) 5:00 then every 5 min; medium camps 5:00; Zipline Boost 6:00; Sinner's Sacrifices and hard camps 8:00; laning ends about 8:00; Soul Urn 10:00 then every 5 min; Unstable Rift about 11:00 then about every 7 min; Guardians weakest from 12:00; Walkers weakest from 18:00; waves every 25 s from 20:00.
- Midboss is alive from 0:00, respawns 7/6/5 min after kills and drops the Rejuvenator (team revive buff). Do not die alone while it is up.
- A Walker gives 3,675 team souls and an extra item slot. When enemies are dead, take a structure instead of chasing.
- Leave lane only after pushing the wave; jungle between waves; trade only for a reward; back off when outnumbered or when enemies are missing.`;

// ---------------------------------------------------------------------------------------
// Rolling history (pure; timestamps are passed in)
// ---------------------------------------------------------------------------------------
export interface Sample { t: number; clock: number | null; hp: number | null; maxHp: number | null; dead: boolean; souls: number | null; unsecured: number | null; screen: string | null;
  status?: string[]; alliesNear?: number | null; enemiesNear?: number | null }
export interface DeathRecord { t: number; clock: number | null; cause: string | null; damage: Array<{ source: string; damage: number | null }>; killingBlow: string | null;
  soulsBefore: number | null; unsecuredBefore: number | null; hpBefore: number | null; respawn: number | null;
  /** Souls read on the death screen (the death-spend check uses the larger of this and soulsBefore). */
  soulsDead?: number | null;
  /** Status effects (debuffs / crowd control) read in the last 10 s alive or on the death screen. */
  status?: string[]; alliesNear?: number | null; enemiesNear?: number | null;
  /** Game clock / wall time of the first alive sample after this death. */
  respawnedClock?: number | null; respawnedT?: number | null }
export interface SpokenCue { t: number; clock: number | null; key: string; text: string; priority: Priority; ai: boolean }
/** AI advice that was actually delivered (used for RECENT AI ADVICE and the repeat gate). */
export interface AdviceRecord { t: number; clock: number | null; trigger: AiTrigger; topic: string; text: string }

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
  reset() { this.samples = []; this.deaths = []; this.spoken = []; this.advice = []; this.deathCount = 0; this.hero = null; }
  push(s: AiSnapshot, cues: Cue[], now: number) {
    const prev = this.samples.at(-1), clock = num(s.game_clock_secs);
    // A clock that jumps backwards is a new match/replay: forget the old one.
    if (prev && clock != null && prev.clock != null && clock + 30 < prev.clock) this.reset();
    const last = this.samples.at(-1);
    // The shop or scoreboard opened from the death screen (no health bar) is still the same death,
    // not a respawn followed by a second death.
    const dead = isDead(s) || (!!last?.dead && s.screen != null && s.screen !== 'Gameplay' && num(s.hp) == null);
    const near = nearCounts(s);
    if (typeof s.hero === 'string' && s.hero) this.hero = s.hero;
    this.samples.push({ t: now, clock, hp: num(s.hp), maxHp: num(s.max_hp), dead, souls: num(s.souls), unsecured: num(s.unsecured_souls), screen: str(s.screen, 20),
      status: statusOf(s), alliesNear: near.allies, enemiesNear: near.enemies });
    if (this.samples.length > 400) this.samples.splice(0, this.samples.length - 400);
    if (dead && (!last || !last.dead)) {
      const alive = [...this.samples].reverse().find(x => !x.dead);
      // Debuffs and who was around in the last 10 s alive (game clock; wall time without one).
      const before = this.samples.filter(x => !x.dead && (clock != null && x.clock != null ? clock - x.clock <= 10 : now - x.t <= 10000));
      this.deathCount++;
      this.deaths.push({ t: now, clock, cause: null, damage: [], killingBlow: null, soulsBefore: alive?.souls ?? null, unsecuredBefore: alive?.unsecured ?? null,
        hpBefore: alive?.hp != null && alive.maxHp ? Math.round(alive.hp / alive.maxHp * 100) : null, respawn: null, soulsDead: null,
        status: [...new Set(before.flatMap(x => x.status || []))].slice(0, 6), alliesNear: alive?.alliesNear ?? null, enemiesNear: alive?.enemiesNear ?? null,
        respawnedClock: null, respawnedT: null });
      if (this.deaths.length > 20) this.deaths.shift();
    }
    if (!dead && last?.dead) { const d = this.deaths.at(-1); if (d && d.respawnedT == null) { d.respawnedClock = clock; d.respawnedT = now; } }
    const death = dead ? this.deaths.at(-1) : undefined;
    if (death) {
      death.cause = str(s.death_cause) ?? death.cause;
      death.respawn = num(s.respawn_secs) ?? death.respawn;
      death.soulsDead = num(s.souls) ?? death.soulsDead ?? null;
      const dmg = Array.isArray(s.death_damage) ? s.death_damage.map(d => ({ source: str(d?.source, 40) || 'unknown', damage: num(d?.damage) })).slice(0, 6) : [];
      if (dmg.length) death.damage = dmg;
      death.killingBlow = describe(s.death_killing_blow) ?? death.killingBlow;
      const st = statusOf(s); if (st.length) death.status = [...new Set([...(death.status || []), ...st])].slice(0, 6);
    }
    for (const c of cues) {
      this.spoken.push({ t: now, clock, key: c.key, text: c.text.slice(0, 200), priority: c.priority, ai: c.key.startsWith('ai:') });
    }
    if (this.spoken.length > 60) this.spoken.splice(0, this.spoken.length - 60);
  }
  /** Record AI advice that was delivered. */
  noteAdvice(rec: AdviceRecord) { this.advice.push({ ...rec, text: rec.text.slice(0, 200) }); if (this.advice.length > 20) this.advice.shift(); }
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
    const values = [num(s.souls), d?.soulsDead ?? null, d?.soulsBefore ?? null].filter((v): v is number => v != null);
    const souls = values.length ? Math.max(...values) : null, need = deathSpend(clock);
    const allowed = souls != null && souls >= need;
    return { souls, clock, dead, target: need, usual, allowed, ruleAt: null,
      reason: souls == null ? 'souls not read' : allowed ? `${fmt(souls)} reaches the ${fmt(need)} death-spend threshold` : `${fmt(souls)} is below the ${fmt(need)} death-spend threshold: not a souls death` };
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
/** Up to 4 "situation → what a coach would say" lines; ones matching a profile weakness first. */
export function coachingAngles({ snapshot: s, history, now, profile, name = defaultName }: AngleInput): string[] {
  if (isDead(s)) return [];
  const out: Array<{ match: boolean; text: string }> = [];
  const add = (key: keyof typeof PROFILE_MATCH, text: string) => {
    const label = profileLabel(profile, PROFILE_MATCH[key]);
    out.push({ match: !!label, text: `${text}${label ? ` [profile: ${label}]` : ''}` });
  };
  const near = nearCounts(s), status = s.hero_status || [];
  const hp = num(s.hp) != null && num(s.max_hp) ? Math.round(s.hp! / s.max_hp! * 100) : null;
  const hero = s.hero ? name(s.hero) : 'you';
  const longRange = !!s.hero && LONG_RANGE_HEROES.has(s.hero);
  const back = history.respawned(now);
  if (back) add('respawn', `Respawned ${back.ago}s ago, ${near.allies ?? 'unknown'} allies near you → walk out with a teammate or catch the wave by your Guardian; do not cross the map alone`);
  const delta = num(s.health_delta_ratio);
  if (hp != null && hp < 60 && !(delta != null && delta > .05)) add('hurt', `Health ${hp}% and not recovering → heal to full before the next fight (shopping does not heal) and leave on the first big hit`);
  const deadEnemy = new Set(status.filter(p => !p.ally && p.dead === true && p.hero).map(p => p.hero!));
  const roster = s.enemy_heroes?.length ? s.enemy_heroes : status.filter(p => !p.ally && p.hero).map(p => p.hero!);
  const divers = [...new Set(roster)].filter(h => CLOSE_RANGE_HEROES.has(h) && !deadEnemy.has(h));
  const lastDeath = history.deaths.at(-1);
  const closeDeath = !!lastDeath && /mele|aura|stalker|slash|rend/i.test(`${lastDeath.cause || ''} ${lastDeath.damage.map(d => d.source).join(' ')}`);
  if (divers.length && ((near.enemies ?? 0) > 0 || closeDeath || profileLabel(profile, PROFILE_MATCH.close)))
    add('close', `Close-range divers alive: ${divers.slice(0, 3).map(name).join(', ')}${near.enemies ? ` (${near.enemies} enemy near you)` : ''}${closeDeath ? '; your last death was at close range' : ''} → ${longRange ? `${hero} is long-range: ` : ''}hold max range behind your front line and save your escape for their dive`);
  const enemyDead = status.filter(p => !p.ally && p.dead === true), allyDead = status.filter(p => p.ally && p.dead === true);
  if (enemyDead.length >= 2) {
    const secs = enemyDead.map(p => p.respawn_secs).filter((v): v is number => typeof v === 'number');
    add('window', `${enemyDead.length} enemies dead${secs.length ? ` (first back in ${Math.min(...secs)}s)` : ''} → group up and hit a Guardian, Walker or the furthest pushed lane now instead of farming your own half`);
  }
  if (allyDead.length >= 2 && allyDead.length > enemyDead.length) add('lost', `${allyDead.length} allies dead vs ${enemyDead.length} enemies → the fight is lost: back off together toward your Guardian`);
  else if (near.allies === 0 && near.missing >= 2) add('alone', `No ally near you and ${near.missing} enemies missing → move toward a teammate before the next trade`);
  const st = statusOf(s);
  if (st.length) add('cc', st.some(x => /stamina/i.test(x)) ? 'Out of stamina: no dash to escape → do not start a trade until it recovers' : `Debuff on you: ${st.slice(0, 3).join(', ')} → disengage or cleanse before you trade`);
  return out.sort((a, b) => Number(b.match) - Number(a.match)).slice(0, 4).map(a => `- ${a.text}`);
}

// ---------------------------------------------------------------------------------------
// Topics, near-duplicates and the souls sentence filter (pure)
// ---------------------------------------------------------------------------------------
/** A sentence that is about souls/spending (the Soul Urn is an objective, not economy). */
export const ECONOMY_RE = /\bsouls?\b(?!\s*urn)|\bunspent\b|\bspend(?:ing)?\b|\bhoard(?:ing)?\b|\b(?:800|1,?600|3,?200|4,?800|6,?400)\b/i;
const TOPICS: Array<[RegExp, string]> = [
  [/soul|item|shop|buy|spend|econom|purchase|hoard/, 'economy'], [/death|died|review/, 'death'], [/heal|health|\bhp\b|sustain/, 'health'],
  [/position|range|spacing|distance|safety|retreat|dive|melee|escape/, 'positioning'], [/objective|walker|guardian|midboss|urn|rift|shrine|patron/, 'objective'],
  [/wave|lane|push|trooper|last.?hit/, 'wave'], [/jungle|camp|farm|sinner/, 'jungle'], [/map|minimap|missing|vision|aware|gank/, 'map'],
  [/team|group|regroup|fight|rotat|respawn|join|window/, 'team'],
];
const sentencesOf = (text: string) => text.match(/[^.!?]+[.!?]+(?=\s|$)|[^.!?]+$/g)?.map(x => x.trim()).filter(Boolean) || [];
/** Normalized topic of an answer: all-souls text is 'economy' whatever the model's tag says. */
export function topicOf(focus: string, text?: string | null): string {
  const parts = text ? sentencesOf(text) : [];
  if (parts.length && parts.every(p => ECONOMY_RE.test(p))) return 'economy';
  const f = (focus || '').toLowerCase();
  const tagged = TOPICS.find(([re]) => re.test(f))?.[1];
  if (tagged) return tagged;
  const t = (text || '').toLowerCase();
  return TOPICS.find(([re]) => re.test(t))?.[1] || 'general';
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

export type AdviceCheck = { ok: true; speak: string; topic: string; note?: string } | { ok: false; reason: string; topic: string };
/** Decide whether an automatic answer may be spoken. Pure: everything comes from the history. */
export function checkAdvice(advice: Advice, trigger: AiTrigger, souls: SoulsView, history: AiHistory, now: number): AdviceCheck {
  let speak = advice.speak || '';
  let topic = topicOf(advice.focus, speak);
  if (!speak) return { ok: false, reason: 'model passed', topic };
  if (trigger === 'manual') return { ok: true, speak, topic }; // the player asked: no gating
  let note: string | undefined;
  // A tier cheaper than the usual buy (alive) or than half of it (death screen) is poor advice late.
  const minTier = souls.dead ? Math.floor(souls.usual / 2) : souls.usual;
  const cheap = (x: string) => [...x.matchAll(/\b(800|1,?600|3,?200)\b(?:[- ]souls?)?[- ](?:items?|tier|upgrade)/gi)].some(m => Number(m[1].replace(',', '')) < minTier);
  const blocked = (x: string) => ECONOMY_RE.test(x) && (!souls.allowed || cheap(x));
  if (sentencesOf(speak).some(blocked)) {
    const rest = sentencesOf(speak).filter(x => !blocked(x)).join(' ').trim();
    const why = souls.allowed ? `names a tier cheaper than ${fmt(minTier)} at ${clockText(souls.clock)}` : souls.reason;
    if (!rest) return { ok: false, reason: `souls advice blocked: ${why}`, topic: 'economy' };
    note = `souls sentence dropped (${why})`; speak = rest;
    // The model's tag described the dropped part: re-derive the topic from what is left.
    topic = topicOf(topicOf(advice.focus) === 'economy' ? '' : advice.focus, speak);
  }
  const clock = history.samples.at(-1)?.clock ?? null;
  const ago = (r: AdviceRecord) => clock != null && r.clock != null ? clock - r.clock : (now - r.t) / 1000;
  const prev = history.advice.at(-1);
  const urgent = advice.priority === 'High' && topic !== 'economy';
  if (trigger === 'periodic' && prev && prev.topic === topic && ago(prev) <= REPEAT_WINDOW_SECS && !urgent)
    return { ok: false, reason: `same focus (${topic}) as the AI call at ${clockText(prev.clock)}`, topic };
  for (const r of history.advice) if (ago(r) <= DUPLICATE_WINDOW_SECS && overlap(speak, r.text) >= NEAR_DUPLICATE)
    return { ok: false, reason: `near-duplicate of the AI call at ${clockText(r.clock)}`, topic };
  for (const c of history.recent(history.spoken, now)) if (!c.ai && overlap(speak, c.text) >= NEAR_DUPLICATE)
    return { ok: false, reason: `repeats the rule call "${c.text.slice(0, 50)}"`, topic };
  return { ok: true, speak, topic, note };
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

export interface SummaryInput { snapshot: AiSnapshot; history: AiHistory; now: number; trigger: AiTrigger; name?: (id: string) => string;
  /** Player-profile text, used to mark COACHING ANGLES that match a known weakness. */
  profile?: string | null }
export function buildSummary({ snapshot: s, history, now, trigger, name = defaultName, profile }: SummaryInput, maxChars = SUMMARY_MAX_CHARS): string {
  // [weight, text]: lower weights are dropped first when the summary exceeds its budget.
  const lines: Array<[number, string]> = [];
  const add = (w: number, t: string | null | false | undefined) => { if (t) lines.push([w, t]); };
  const clock = num(s.game_clock_secs), dead = isDead(s);
  const phase = clock == null ? 'unknown phase' : clock < 480 ? 'laning phase' : clock < 1200 ? 'midgame' : 'lategame';
  add(10, `Trigger: ${trigger} · clock ${clockText(clock)} (${phase}) · screen ${s.screen || 'unknown'}${dead ? ' · YOU ARE DEAD' : ''}`);
  const kda = [s.kills, s.deaths, s.assists].every(v => num(v) != null) ? ` · K/D/A ${s.kills}/${s.deaths}/${s.assists}` : '';
  const hero = s.hero || history.hero; // the death screen hides the hero portrait
  add(10, `You: ${hero ? name(hero) : 'hero unknown'}${num(s.level) != null ? ` · level ${s.level}` : ''}${kda}`);
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
  if (s.items?.length) add(6, `Your items (${s.items.length}): ${s.items.slice(0, 12).map(name).join(', ')}`);
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
  if (s.enemy_items?.length) add(4, `Enemy items seen: ${s.enemy_items.slice(0, 10).map(name).join(', ')}`);
  if (num(s.ally_networth) != null && num(s.enemy_networth) != null) {
    const d = s.ally_networth! - s.enemy_networth!;
    add(6, `Team souls: yours ${fmt(s.ally_networth!)} vs enemy ${fmt(s.enemy_networth!)} (${d >= 0 ? '+' : ''}${fmt(d)})`);
  }
  // Minimap hero tracks relative to you.
  if (map?.heroes?.length) {
    const enemies = map.heroes.filter(h => !h.ally && h.hits >= 2), visible = enemies.filter(h => h.unseen_ms === 0);
    const missing = enemies.filter(h => h.unseen_ms > 0);
    const you = map.you;
    const near = you ? visible.filter(h => Math.hypot(h.pos.x - you.x, h.pos.y - you.y) < .12).length : null;
    const allyNear = you ? map.heroes.filter(h => h.ally && h.unseen_ms === 0 && h.hits >= 2 && Math.hypot(h.pos.x - you.x, h.pos.y - you.y) < .12).length : null;
    add(8, `Minimap: ${visible.length} enemies visible${near != null ? ` (${near} near you, ${allyNear} allies near you)` : ''}${missing.length ? `, ${missing.length} missing (last seen ${missing.map(h => `${Math.round(h.unseen_ms / 1000)}s ago${h.lane ? ` ${h.lane}` : ''}`).join(', ')})` : ''}`);
  }
  if (map) {
    const lanes = (['green', 'blue', 'yellow'] as const).map(k => {
      const l = map[k]; if (!l) return null;
      return `${k[0].toUpperCase()}${k.slice(1)} ${l.push == null ? 'front unknown' : `front ${Math.round(l.push * 100)}% up lane`} (troopers ${l.ally_troopers} v ${l.enemy_troopers})`;
    }).filter(Boolean);
    if (lanes.length) add(6, `Lanes: ${lanes.join('; ')}`);
    const urn = { Visible: 'marker seen', Ground: 'on the ground', CarriedByYou: 'carried by you', CarriedByAlly: 'carried by an ally (inferred)', CarriedByEnemy: 'carried by an enemy (inferred)' } as const;
    add(6, `Objectives: Midboss icon ${map.midboss_up ? 'visible (up)' : 'not confirmed'}${map.urn ? ` · Soul Urn ${urn[map.urn]}` : ''}`);
  }
  if (clock != null) { const next = upcomingObjectives(clock); if (next.length) add(6, `Upcoming: ${next.slice(0, 3).join('; ')}`); }
  // Deaths.
  const deaths = history.deaths;
  const recentDeaths = history.recent(deaths, now, trigger === 'death' ? 600 : HISTORY_WINDOW_SECS);
  const shown = trigger === 'death' && deaths.length && !recentDeaths.includes(deaths.at(-1)!) ? [...recentDeaths, deaths.at(-1)!] : recentDeaths;
  if (deaths.length) add(7, `Deaths observed this session: ${history.deathCount}${deaths.length > 1 ? ` (at ${deaths.slice(-4).map(d => clockText(d.clock)).join(', ')})` : ''}`);
  for (const d of shown.slice(-2)) {
    const need = deathSpend(d.clock);
    const soulsAt = Math.max(d.soulsBefore ?? -1, d.soulsDead ?? -1);
    add(trigger === 'death' ? 10 : 8, `Death at ${clockText(d.clock)}: cause ${d.cause || 'unknown'}${d.damage.length ? ` · damage taken: ${d.damage.map(x => `${x.source}${x.damage != null ? ` ${fmt(x.damage)}` : ''}`).join(', ')}` : ''}${d.killingBlow ? ` · killing blow ${d.killingBlow}` : ''}${d.status?.length ? ` · status effects before death: ${d.status.join(', ')}` : ''}${d.hpBefore != null ? ` · last alive HP ${d.hpBefore}%` : ''}${d.alliesNear != null ? ` · allies near you ${d.alliesNear}, enemies near ${d.enemiesNear ?? '?'}` : ''}${d.soulsBefore != null ? ` · unspent before death ${fmt(d.soulsBefore)}` : ''}${d.unsecuredBefore != null ? ` (unsecured ${fmt(d.unsecuredBefore)})` : ''}${soulsAt >= 0 ? ` · death-spend threshold ${fmt(need)}: ${soulsAt >= need ? 'reached' : 'not reached'}` : ''}${d.respawn != null ? ` · respawn ${d.respawn}s` : ''}`);
  }
  // The rule coach's death review for the latest death (names the killer hero and crowd control).
  const lastDeath = deaths.at(-1);
  const review = lastDeath ? history.spoken.filter(c => !c.ai && /^(death:review|active:(close|repeat|cc|debuff))/.test(c.key) && (lastDeath.clock == null || c.clock == null || c.clock >= lastDeath.clock)).slice(-2) : [];
  if (review.length && (trigger === 'death' || (lastDeath && history.recent([lastDeath], now, 300).length)))
    add(trigger === 'death' ? 10 : 7, `Rule death review (already spoken): ${review.map(c => c.text).join(' ')}`.slice(0, 400));
  // What the AI already said: the model must vary its focus (checkAdvice() enforces it).
  const mine = history.recent(history.advice, now, 600).slice(-4);
  if (mine.length) add(9, `RECENT AI ADVICE (yours; do not reuse a focus within 3 min):\n${mine.map(a => `- [${clockText(a.clock)}] ${a.topic}: ${a.text}`).join('\n')}`);
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
  if (s.minimap?.midboss_up && clock >= 600) t.push('midboss', 'rejuvenator');
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
    ...(profile ? ['', 'PLAYER PROFILE (from earlier matches; coach these weaknesses when the live situation matches)', profile] : []),
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
  return { provider: s.aiProvider, baseUrl: s.aiBaseUrl, model: s.aiModel, key: s.aiKey, maxTokens: s.aiMaxTokens, headers: s.aiHeaders };
}
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
export function buildRequest(cfg: AiConfig, system: string, user: string, maxTokens = cfg.maxTokens): GatewayRequest {
  const extra = parseExtraHeaders(cfg.headers);
  if (cfg.provider === 'anthropic') {
    const headers: Record<string, string> = { 'content-type': 'application/json', 'anthropic-version': '2023-06-01', 'anthropic-dangerous-direct-browser-access': 'true', ...extra };
    if (cfg.key) headers['x-api-key'] = cfg.key;
    return { url: endpoint(cfg), init: { method: 'POST', headers, body: JSON.stringify({ model: cfg.model, max_tokens: maxTokens, temperature: 0.3, system, messages: [{ role: 'user', content: user }] }) } };
  }
  const headers: Record<string, string> = { 'content-type': 'application/json', ...extra };
  if (cfg.key) headers.authorization = `Bearer ${cfg.key}`;
  return { url: endpoint(cfg), init: { method: 'POST', headers, body: JSON.stringify({ model: cfg.model, messages: [{ role: 'system', content: system }, { role: 'user', content: user }], max_tokens: maxTokens, temperature: 0.3 }) } };
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
export interface CallResult { text: string; ms: number; model: string }
export async function callGateway(cfg: AiConfig, system: string, user: string, opts: CallOptions = {}): Promise<CallResult> {
  validateConfig(cfg, opts.pageProtocol);
  const req = buildRequest(cfg, system, user, opts.maxTokens);
  const now = opts.now || (() => Date.now()), started = now();
  const secrets = secretsOf(cfg);
  const entry: AiExchange = { t: Date.now(), clock: opts.clock ?? null, trigger: opts.trigger || 'call', url: redact(req.url, secrets), model: cfg.model, status: null, ms: 0,
    system_chars: system.length, prompt: redact(user, secrets), reply: '', text: null, error: null };
  try { const r = await callGatewayInner(cfg, req, opts, secrets, entry, now, started); entry.text = r.text; entry.model = r.model; return r; }
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
    let text: string;
    try { text = extractText(cfg.provider, data); } catch (e) { throw new GatewayError('reply', redact((e as Error).message, secrets)); }
    // A reply cut off by the token limit (OpenRouter + Claude Haiku at 200 tokens stopped mid-JSON).
    { const dd = data as Record<string, any>; const fin = dd?.choices?.[0]?.finish_reason ?? dd?.stop_reason ?? dd?.incomplete_details?.reason; // eslint-disable-line @typescript-eslint/no-explicit-any
      if ((fin === 'length' || fin === 'max_tokens' || fin === 'max_output_tokens') && parseAdvice(text).ok === false)
        throw new GatewayError('truncated', `The reply was cut off at the ${opts.maxTokens ?? cfg.maxTokens}-token limit (finish_reason "${fin}"). Raise "Max reply tokens" in Settings.`); }
    const model = typeof (data as { model?: unknown }).model === 'string' ? (data as { model: string }).model : cfg.model;
    return { text, ms: Math.round(now() - started), model };
  } finally { clearTimeout(timer); opts.signal?.removeEventListener('abort', onAbort); }
}

// ---------------------------------------------------------------------------------------
// Reply parsing
// ---------------------------------------------------------------------------------------
export interface Advice { speak: string | null; priority: Priority; why: string; focus: string }
export type ParseResult = { ok: true; advice: Advice } | { ok: false; error: string };
function jsonCandidates(text: string): string[] {
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
/** Make advice speakable and enforce the word/sentence limit. */
export function cleanSpeech(text: string, maxWords = 30, maxSentences = 2): string {
  const clean = text.replace(/[*_#`~>|]/g, '').replace(/\s+/g, ' ').trim();
  const sentences = clean.match(/[^.!?]+[.!?]+(?=\s|$)|[^.!?]+$/g)?.map(s => s.trim()).filter(Boolean) || [];
  let out = '', words = 0;
  for (const s of sentences.slice(0, maxSentences)) {
    const w = s.split(' ').length;
    if (words + w > maxWords) { if (!out) out = s.split(' ').slice(0, maxWords).join(' ').replace(/[,;:]$/, '') + '.'; break; }
    out = out ? `${out} ${s}` : s; words += w;
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
  return { ok: true, advice: { speak, priority, why, focus } };
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
export interface AdviceMeta { trigger: AiTrigger; ms: number; model: string; tokens: number; live: boolean; clock: number | null }
export interface AiStatus {
  state: 'off' | 'idle' | 'thinking' | 'error'; message: string;
  last?: { advice: Advice; meta: AdviceMeta }; error?: string; retryAt?: number;
  requests: number; failures: number; stale: number; lastMs?: number;
  /** Answers not spoken because checkAdvice() rejected them (repeat, near-duplicate, souls gate). */
  suppressed?: number;
}
export interface AiCoachDeps {
  settings: () => Settings; isLive: () => boolean; generation: () => number;
  knowledge: () => Promise<Knowledge[]>; name?: (id: string) => string;
  onAdvice: (cue: Cue, advice: Advice, meta: AdviceMeta) => void; onStatus?: (s: AiStatus) => void;
  now?: () => number; fetchImpl?: typeof fetch; pageProtocol?: () => string;
}
export class AiCoach {
  readonly history = new AiHistory();
  status: AiStatus = { state: 'off', message: 'AI coach is off.', requests: 0, failures: 0, stale: 0 };
  private deps: AiCoachDeps; private inFlight?: { ctrl: AbortController; trigger: AiTrigger };
  private consecutive = 0; private backoffUntil = 0; private lastPeriodicAt = 0; private lastHighAt = -Infinity; private deathAsked = 0;
  private latest: AiSnapshot = {}; private sectionCache?: { docs: Knowledge[]; sections: KnowledgeSection[] };
  constructor(deps: AiCoachDeps) { this.deps = deps; this.refresh(); }
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
  /** Feed every analyzed snapshot (live or replay). Triggers only fire when `live`. */
  observe(s: AiSnapshot, cues: Cue[], live: boolean) {
    const now = this.now(); this.latest = s; this.history.push(s, cues, now);
    if (cues.some(c => c.priority === 'High' && !c.key.startsWith('ai:'))) this.lastHighAt = now;
    if (!live || !this.deps.settings().aiEnabled) return;
    if (isDead(s)) {
      const death = this.history.deaths.at(-1);
      if (!death || this.history.deathCount === this.deathAsked || now < this.backoffUntil) return;
      // Wait for the rule coach's death review (it names the killer hero and crowd control), or
      // 4 s after the death panel was read, or 8 s at most.
      const panel = !!(death.cause || death.damage.length || death.killingBlow);
      const reviewed = this.history.spoken.some(c => !c.ai && c.key.startsWith('death:review') && c.t >= death.t);
      const ready = (panel && (reviewed || now - death.t >= DEATH_WAIT_MS)) || now - death.t >= DEATH_WAIT_MAX_MS;
      if (!ready) return;
      if (this.inFlight?.trigger === 'periodic') this.cancel();
      if (this.inFlight) return;
      this.deathAsked = this.history.deathCount; void this.ask('death');
      return;
    }
    if (s.screen !== 'Gameplay') return;
    if (!this.lastPeriodicAt) { this.lastPeriodicAt = now; return; }
    const interval = Math.max(15, this.deps.settings().aiInterval) * 1000;
    if (now - this.lastPeriodicAt < interval || now - this.lastHighAt < 10000 || now < this.backoffUntil || this.inFlight) return;
    this.lastPeriodicAt = now; void this.ask('periodic');
  }
  cancel() { this.inFlight?.ctrl.abort(); this.inFlight = undefined; if (this.status.state === 'thinking') this.set({ state: this.deps.settings().aiEnabled ? 'idle' : 'off', message: 'Request canceled.' }); }
  reset() { this.cancel(); this.history.reset(); this.latest = {}; this.lastPeriodicAt = 0; this.deathAsked = 0; this.lastHighAt = -Infinity; }
  /** Prompt for the current state (exposed for diagnostics/tests). */
  async prompt(trigger: AiTrigger, retry?: PromptInput['retry']): Promise<Prompt> {
    const docs = await this.deps.knowledge().catch(() => [] as Knowledge[]);
    if (this.sectionCache?.docs !== docs) this.sectionCache = { docs, sections: sections(docs) };
    const name = this.deps.name || defaultName, s = this.latest, profile = profileText();
    const summary = buildSummary({ snapshot: s, history: this.history, now: this.now(), trigger, name, profile });
    const excerpts = retrieve(this.sectionCache.sections, situationTerms(s, this.history, trigger));
    const enemies = s.enemy_heroes?.length ? s.enemy_heroes : (s.hero_status || []).filter(p => !p.ally && p.hero).map(p => p.hero!);
    return buildPrompt({ trigger, summary, profile, excerpts, matchup: heroMatchup(docs, s.hero, enemies, name), retry });
  }
  async ask(trigger: AiTrigger): Promise<void> {
    const settings = this.deps.settings();
    if (!settings.aiEnabled) { this.set({ state: 'off', message: 'AI coach is off. Turn it on in Settings and configure a gateway.' }); return; }
    if (this.inFlight) { if (trigger === 'manual') this.set({ message: 'A request is already in flight; wait for it to finish.' }); return; }
    const ctrl = new AbortController(); this.inFlight = { ctrl, trigger };
    const generation = this.deps.generation(), started = this.now();
    this.set({ state: 'thinking', message: `Asking ${settings.aiModel || 'the model'} (${trigger})…`, requests: this.status.requests + 1 });
    try {
      let retry: PromptInput['retry'] = null;
      let truncated = false;
      for (let attempt = 0; attempt < 2; attempt++) {
        const prompt = await this.prompt(trigger, retry);
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
        const age = this.now() - started, sameGen = generation === this.deps.generation();
        if (!sameGen || (trigger !== 'manual' && (age > STALE_MS || !this.deps.isLive()))) {
          this.consecutive = 0; this.backoffUntil = 0;
          this.set({ state: 'idle', message: `Dropped a stale answer (${Math.round(age / 1000)} s old).`, stale: this.status.stale + 1, lastMs: res.ms }); return;
        }
        const parsed = parseAdvice(res.text, trigger === 'death' ? 45 : 30, trigger === 'death' ? 3 : 2);
        if (!parsed.ok) throw new GatewayError('reply', redact(parsed.error, secretsOf(configFrom(settings))));
        this.consecutive = 0; this.backoffUntil = 0;
        const meta: AdviceMeta = { trigger, ms: res.ms, model: res.model, tokens: prompt.tokens, live: this.deps.isLive() && sameGen, clock: num(this.latest.game_clock_secs) };
        // Quality gate: souls only when the souls gate is open, no repeated focus, no near-duplicates.
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
          this.history.noteAdvice({ t: this.now(), clock: meta.clock, trigger, topic: advice.focus, text: advice.speak });
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
    const parts = [`${s.aiProvider === 'anthropic' ? 'Anthropic' : 'OpenAI-compatible'} · ${host} · ${s.aiModel || 'no model'}`, `every ${s.aiInterval}s`,
      `${this.status.requests} requests`];
    if (this.status.lastMs != null) parts.push(`last ${this.status.lastMs} ms`);
    if (this.status.stale) parts.push(`${this.status.stale} stale dropped`);
    if (this.status.suppressed) parts.push(`${this.status.suppressed} repeats skipped`);
    if (this.status.failures) parts.push(`${this.status.failures} errors`);
    if (this.backoffUntil > this.now()) parts.push(`backing off ${Math.ceil((this.backoffUntil - this.now()) / 1000)} s`);
    return parts.join(' · ');
  }
}

/** Minimal connectivity check: one tiny request, success or the exact failure. */
export async function testConnection(cfg: AiConfig, opts: CallOptions = {}): Promise<{ ok: boolean; message: string }> {
  try {
    const res = await callGateway(cfg, 'You are a connectivity check. Reply with only this JSON: {"speak": null, "priority": "Low", "why": "connection test", "focus": "test"}', 'Connection test. Reply with the JSON only.', { ...opts, maxTokens: Math.min(64, cfg.maxTokens), timeoutMs: opts.timeoutMs ?? 30000 });
    const parsed = parseAdvice(res.text);
    return { ok: true, message: `Connected · ${res.model} · ${res.ms} ms · ${parsed.ok ? 'reply is valid coach JSON' : `reply received but not coach JSON: "${redact(res.text, secretsOf(cfg)).replace(/\s+/g, ' ').slice(0, 100)}"`}` };
  } catch (e) {
    const kind = e instanceof GatewayError ? e.kind : 'error';
    const label: Record<string, string> = { config: 'Configuration', network: 'Network/CORS', http: 'Gateway error', timeout: 'Timeout', aborted: 'Canceled', reply: 'Unexpected reply', truncated: 'Reply cut off', error: 'Error' };
    return { ok: false, message: `${label[kind] || 'Error'}: ${e instanceof Error ? e.message : String(e)}` };
  }
}
