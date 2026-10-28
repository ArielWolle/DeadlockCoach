// Optional AI coach: DOM-free logic (history, state summary, knowledge retrieval, prompt,
// provider request shapes, reply parsing) plus a small controller that owns triggers,
// the single in-flight request, staleness and error backoff. The rule-based coach is
// unchanged; this module only adds advice on top of it. Nothing here touches the DOM, so
// tests/unit.test.ts can import it directly. Only `import type` is used for the same reason.
//
// Input budget (documented in docs/ai-coach.md): ~2,500 tokens, measured as chars / 4.
//   system prompt ≈ 1,600 chars · coach brief ≈ 1,500 · state summary ≤ 3,000
//   player profile ≤ 900 · retrieved excerpts ≤ 2,100 (≤ 3 × 700) · task/framing ≈ 400
// buildPrompt() enforces MAX_INPUT_CHARS by shrinking excerpts, then profile, then summary.
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
export const estimateTokens = (chars: number) => Math.ceil(chars / 4);

export const PROVIDER_PRESETS: Record<AiProvider, { label: string; baseUrl: string; modelHint: string }> = {
  openai: { label: 'OpenAI-compatible (OpenAI, OpenRouter, LiteLLM, Azure proxy, Ollama, LM Studio)', baseUrl: 'https://api.openai.com/v1', modelHint: 'e.g. gpt-4o-mini · openai/gpt-4o-mini · llama3.1' },
  anthropic: { label: 'Anthropic Messages', baseUrl: 'https://api.anthropic.com', modelHint: 'e.g. claude-3-5-haiku-latest' },
};

// ---------------------------------------------------------------------------------------
// Prompt text
// ---------------------------------------------------------------------------------------
export const SYSTEM_PROMPT = `You are an elite Deadlock coach (Valve's 6v6 hero-shooter MOBA) speaking into the player's ear during a live match.
Rules:
1. Use ONLY the evidence in STATE, PLAYER PROFILE and KNOWLEDGE. Never invent enemy positions, cooldowns, items, health or timers. "unseen", "missing" or "unknown" means unknown, never safe.
2. Pick the single most impactful fix for the next 30 seconds. One idea only.
3. Be concrete and imperative: what to do, where and when (for example "Spend your 3,200 souls at the Blue shop after this wave."). No greetings, hedging or stat lectures.
4. Live advice: at most 2 short sentences and 30 words, natural for text-to-speech (no lists, symbols or markdown).
5. Be hero-specific when the player's hero or enemy heroes are known, but never guess ability details you are not certain of.
6. RECENT COACH CALLS were already spoken. Do not repeat them unless you add a concrete how-to.
7. If nothing important is happening or the evidence is too thin, set "speak" to null (PASS).
Reply with ONLY one JSON object, no code fence, no other text:
{"speak": string or null, "priority": "High" | "Normal" | "Low", "why": "one line citing the evidence you used", "focus": "short tag such as souls, wave, death, objective, positioning, jungle, items"}`;

export const TASKS: Record<AiTrigger, string> = {
  periodic: 'TASK: Give the one most useful call for right now, or PASS if the rule-based coach already covers it.',
  death: 'TASK: The player just died. Using the death evidence, say the most likely reason and the one change for the next life (what to buy or do on respawn). Up to 3 short sentences, at most 45 words.',
  manual: 'TASK: The player pressed "Ask the coach now". Give the most useful call for this moment; only PASS if there is no usable evidence.',
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
export interface Sample { t: number; clock: number | null; hp: number | null; maxHp: number | null; dead: boolean; souls: number | null; unsecured: number | null; screen: string | null }
export interface DeathRecord { t: number; clock: number | null; cause: string | null; damage: Array<{ source: string; damage: number | null }>; killingBlow: string | null;
  soulsBefore: number | null; unsecuredBefore: number | null; hpBefore: number | null; respawn: number | null }
export interface SpokenCue { t: number; clock: number | null; key: string; text: string; priority: Priority; ai: boolean }

export const isDead = (s: Snapshot) => s.dead === true || s.screen === 'Dead';
const num = (v: unknown): number | null => typeof v === 'number' && Number.isFinite(v) ? v : null;
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

export class AiHistory {
  samples: Sample[] = []; deaths: DeathRecord[] = []; spoken: SpokenCue[] = [];
  deathCount = 0;
  reset() { this.samples = []; this.deaths = []; this.spoken = []; this.deathCount = 0; }
  push(s: AiSnapshot, cues: Cue[], now: number) {
    const prev = this.samples.at(-1), dead = isDead(s), clock = num(s.game_clock_secs);
    // A clock that jumps backwards is a new match/replay: forget the old one.
    if (prev && clock != null && prev.clock != null && clock + 30 < prev.clock) this.reset();
    const last = this.samples.at(-1);
    this.samples.push({ t: now, clock, hp: num(s.hp), maxHp: num(s.max_hp), dead, souls: num(s.souls), unsecured: num(s.unsecured_souls), screen: str(s.screen, 20) });
    if (this.samples.length > 400) this.samples.splice(0, this.samples.length - 400);
    if (dead && (!last || !last.dead)) {
      const alive = [...this.samples].reverse().find(x => !x.dead);
      this.deathCount++;
      this.deaths.push({ t: now, clock, cause: null, damage: [], killingBlow: null, soulsBefore: alive?.souls ?? null, unsecuredBefore: alive?.unsecured ?? null,
        hpBefore: alive?.hp != null && alive.maxHp ? Math.round(alive.hp / alive.maxHp * 100) : null, respawn: null });
      if (this.deaths.length > 20) this.deaths.shift();
    }
    const death = dead ? this.deaths.at(-1) : undefined;
    if (death) {
      death.cause = str(s.death_cause) ?? death.cause;
      death.respawn = num(s.respawn_secs) ?? death.respawn;
      const dmg = Array.isArray(s.death_damage) ? s.death_damage.map(d => ({ source: str(d?.source, 40) || 'unknown', damage: num(d?.damage) })).slice(0, 6) : [];
      if (dmg.length) death.damage = dmg;
      death.killingBlow = describe(s.death_killing_blow) ?? death.killingBlow;
    }
    for (const c of cues) {
      this.spoken.push({ t: now, clock, key: c.key, text: c.text.slice(0, 200), priority: c.priority, ai: c.key.startsWith('ai:') });
    }
    if (this.spoken.length > 30) this.spoken.splice(0, this.spoken.length - 30);
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
const TIERS = [6400, 3200, 1600, 800];

/** Upcoming objective events (seconds) within `horizon` of `clock`, from docs/timings/match-timeline.md. */
export function upcomingObjectives(clock: number, horizon = 180): string[] {
  const events: Array<[number, string]> = [[120, 'easy camps spawn'], [300, 'medium camps spawn'], [360, 'Zipline Boost unlocks'], [480, "Sinner's Sacrifices and hard camps spawn"],
    [660, 'first Unstable Rift (about)'], [720, 'Guardians reach minimum resist'], [1080, 'Walkers reach minimum resist']];
  for (let t = 300; t <= 3600; t += 300) events.push([t, 'Powerups spawn']);
  for (let t = 600; t <= 3600; t += 300) events.push([t, 'Soul Urn spawns']);
  return events.filter(([t]) => t > clock && t - clock <= horizon).sort((a, b) => a[0] - b[0] || a[1].localeCompare(b[1]))
    .map(([t, what]) => `${what} at ${clockText(t)} (in ${clockText(t - clock)})`);
}

export interface SummaryInput { snapshot: AiSnapshot; history: AiHistory; now: number; trigger: AiTrigger; name?: (id: string) => string }
export function buildSummary({ snapshot: s, history, now, trigger, name = defaultName }: SummaryInput, maxChars = SUMMARY_MAX_CHARS): string {
  // [weight, text]: lower weights are dropped first when the summary exceeds its budget.
  const lines: Array<[number, string]> = [];
  const add = (w: number, t: string | null | false | undefined) => { if (t) lines.push([w, t]); };
  const clock = num(s.game_clock_secs), dead = isDead(s);
  const phase = clock == null ? 'unknown phase' : clock < 480 ? 'laning phase' : clock < 1200 ? 'midgame' : 'lategame';
  add(10, `Trigger: ${trigger} · clock ${clockText(clock)} (${phase}) · screen ${s.screen || 'unknown'}${dead ? ' · YOU ARE DEAD' : ''}`);
  const kda = [s.kills, s.deaths, s.assists].every(v => num(v) != null) ? ` · K/D/A ${s.kills}/${s.deaths}/${s.assists}` : '';
  add(10, `You: ${s.hero ? name(s.hero) : 'hero unknown'}${num(s.level) != null ? ` · level ${s.level}` : ''}${kda}`);
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
  // Souls: amount, unsecured part, how long a similar balance has been held, affordable tier.
  if (num(s.souls) != null) {
    const souls = s.souls!; let held = '';
    if (souls >= 800) {
      const floor = souls * .75; let first: Sample | undefined;
      for (let i = samples.length - 1; i >= 0 && samples[i].souls != null && samples[i].souls! >= floor; i--) first = samples[i];
      const last = samples.at(-1);
      const secs = first && last ? (first.clock != null && last.clock != null ? last.clock - first.clock : Math.round((last.t - first.t) / 1000)) : 0;
      if (secs >= 20) held = ` · held ≥${fmt(floor)} for ${secs}s`;
    }
    const tier = TIERS.find(t => souls >= t);
    add(9, `Unspent souls ${fmt(souls)}${num(s.unsecured_souls) != null ? ` (unsecured ${fmt(s.unsecured_souls!)})` : ''}${held}${tier ? ` · can afford a ${fmt(tier)} item` : ''}`);
  }
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
    add(trigger === 'death' ? 10 : 8, `Death at ${clockText(d.clock)}: cause ${d.cause || 'unknown'}${d.damage.length ? ` · damage taken: ${d.damage.map(x => `${x.source}${x.damage != null ? ` ${fmt(x.damage)}` : ''}`).join(', ')}` : ''}${d.killingBlow ? ` · killing blow ${d.killingBlow}` : ''}${d.hpBefore != null ? ` · last alive HP ${d.hpBefore}%` : ''}${d.soulsBefore != null ? ` · unspent before death ${fmt(d.soulsBefore)}` : ''}${d.unsecuredBefore != null ? ` (unsecured ${fmt(d.unsecuredBefore)})` : ''}${d.respawn != null ? ` · respawn ${d.respawn}s` : ''}`);
  }
  // Recently spoken calls, so the model does not repeat them.
  const spoken = history.recent(history.spoken, now).slice(-6);
  if (spoken.length) add(9, `RECENT COACH CALLS (already spoken):\n${spoken.map(c => `- [${clockText(c.clock)}] ${c.ai ? 'AI' : 'rule'}: ${c.text}`).join('\n')}`);
  else add(3, 'RECENT COACH CALLS: none');
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
    t.push('dying', 'dying', 'death', 'die', 'haunted', 'unsecured', 'alone');
    const cause = (history.deaths.at(-1)?.cause || s.death_cause || '').toLowerCase();
    if (/guardian|walker|tower|patron|shrine/.test(cause)) t.push('guardian', 'guardian', 'dive');
    if (/trooper|camp|neutral|haunt/.test(cause)) t.push('camp', 'jungle');
    if (souls >= 800) t.push('unspent', 'spend');
  } else if (souls >= 1600) t.push('unspent', 'unspent', 'hoarding', 'buy', 'shop');
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
export interface PromptInput { trigger: AiTrigger; summary: string; profile?: string | null; excerpts?: Array<{ title: string; heading: string; text: string }>; matchup?: string | null; brief?: string }
export interface Prompt { system: string; user: string; chars: number; tokens: number }
export function buildPrompt(input: PromptInput, maxChars = MAX_INPUT_CHARS): Prompt {
  const brief = input.brief ?? COACH_BRIEF;
  let summary = input.summary.slice(0, SUMMARY_MAX_CHARS);
  let profile = (input.profile || '').trim().slice(0, PROFILE_MAX_CHARS);
  let excerpts = (input.excerpts || []).slice(0, EXCERPTS_MAX).map(e => ({ ...e, text: e.text.length > EXCERPT_MAX_CHARS ? e.text.slice(0, EXCERPT_MAX_CHARS - 1) + '…' : e.text }));
  const render = () => [TASKS[input.trigger], '', 'STATE', summary,
    ...(profile ? ['', 'PLAYER PROFILE (from earlier matches)', profile] : []),
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
export function extractText(provider: AiProvider, data: unknown): string {
  const d = data as Record<string, any> | null; // eslint-disable-line @typescript-eslint/no-explicit-any
  if (!d || typeof d !== 'object') throw new Error('Gateway reply is not a JSON object');
  if (provider === 'anthropic') {
    if (!Array.isArray(d.content)) throw new Error(`Gateway reply has no "content" array${d.error ? `: ${JSON.stringify(d.error).slice(0, 160)}` : ''}`);
    return d.content.filter((c: { type?: string }) => c?.type === 'text').map((c: { text?: string }) => c.text || '').join('');
  }
  const content = d.choices?.[0]?.message?.content;
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) return content.map((c: { text?: string }) => c?.text || '').join('');
  throw new Error(`Gateway reply has no choices[0].message.content${d.error ? `: ${JSON.stringify(d.error).slice(0, 160)}` : ''}`);
}

/** Remove secrets from text that may be shown to the user (gateways sometimes echo keys). */
export function redact(text: string, secrets: string[] = []): string {
  let out = text;
  for (const secret of secrets) if (secret && secret.length >= 4) out = out.split(secret).join('[redacted]');
  return out.replace(/\b(sk-[A-Za-z0-9_-]{6,}|sk_[A-Za-z0-9_-]{6,}|Bearer\s+[A-Za-z0-9._~+/-]{8,})/g, '[redacted]');
}
function secretsOf(cfg: AiConfig): string[] {
  try { return [cfg.key, ...Object.values(parseExtraHeaders(cfg.headers))]; } catch { return [cfg.key]; }
}

export type GatewayErrorKind = 'config' | 'network' | 'http' | 'timeout' | 'aborted' | 'reply';
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

export interface CallOptions { signal?: AbortSignal; maxTokens?: number; timeoutMs?: number; fetchImpl?: typeof fetch; pageProtocol?: string; now?: () => number }
export interface CallResult { text: string; ms: number; model: string }
export async function callGateway(cfg: AiConfig, system: string, user: string, opts: CallOptions = {}): Promise<CallResult> {
  validateConfig(cfg, opts.pageProtocol);
  const req = buildRequest(cfg, system, user, opts.maxTokens);
  const now = opts.now || (() => Date.now()), started = now();
  const ctrl = new AbortController(); let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; ctrl.abort(); }, opts.timeoutMs ?? 20000);
  const onAbort = () => ctrl.abort();
  opts.signal?.addEventListener('abort', onAbort, { once: true });
  if (opts.signal?.aborted) ctrl.abort();
  const secrets = secretsOf(cfg);
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
    if (!res.ok) throw new GatewayError('http', redact(`HTTP ${res.status}${res.statusText ? ` ${res.statusText}` : ''}: ${body.replace(/\s+/g, ' ').slice(0, 300) || '(empty body)'}`, secrets), res.status);
    let data: unknown;
    try { data = JSON.parse(body); } catch { throw new GatewayError('reply', redact(`Gateway answered HTTP ${res.status} but not JSON: ${body.slice(0, 160)}`, secrets)); }
    let text: string;
    try { text = extractText(cfg.provider, data); } catch (e) { throw new GatewayError('reply', redact((e as Error).message, secrets)); }
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
      const ready = death.cause || death.damage.length || death.killingBlow || now - death.t >= 2500;
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
  async prompt(trigger: AiTrigger): Promise<Prompt> {
    const docs = await this.deps.knowledge().catch(() => [] as Knowledge[]);
    if (this.sectionCache?.docs !== docs) this.sectionCache = { docs, sections: sections(docs) };
    const name = this.deps.name || defaultName, s = this.latest;
    const summary = buildSummary({ snapshot: s, history: this.history, now: this.now(), trigger, name });
    const excerpts = retrieve(this.sectionCache.sections, situationTerms(s, this.history, trigger));
    const enemies = s.enemy_heroes?.length ? s.enemy_heroes : (s.hero_status || []).filter(p => !p.ally && p.hero).map(p => p.hero!);
    return buildPrompt({ trigger, summary, profile: profileText(), excerpts, matchup: heroMatchup(docs, s.hero, enemies, name) });
  }
  async ask(trigger: AiTrigger): Promise<void> {
    const settings = this.deps.settings();
    if (!settings.aiEnabled) { this.set({ state: 'off', message: 'AI coach is off. Turn it on in Settings and configure a gateway.' }); return; }
    if (this.inFlight) { if (trigger === 'manual') this.set({ message: 'A request is already in flight; wait for it to finish.' }); return; }
    const ctrl = new AbortController(); this.inFlight = { ctrl, trigger };
    const generation = this.deps.generation(), started = this.now();
    this.set({ state: 'thinking', message: `Asking ${settings.aiModel || 'the model'} (${trigger})…`, requests: this.status.requests + 1 });
    try {
      const prompt = await this.prompt(trigger);
      if (ctrl.signal.aborted) return;
      const maxTokens = trigger === 'death' ? Math.round(settings.aiMaxTokens * 1.5) : settings.aiMaxTokens;
      const res = await callGateway(configFrom(settings), prompt.system, prompt.user, { signal: ctrl.signal, maxTokens, timeoutMs: trigger === 'manual' ? 45000 : STALE_MS,
        fetchImpl: this.deps.fetchImpl, pageProtocol: this.deps.pageProtocol?.(), now: this.deps.now });
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
      this.set({ state: 'idle', message: parsed.advice.speak ? 'Advice received.' : 'Model passed: nothing more important than the current calls.', last: { advice: parsed.advice, meta }, error: undefined, retryAt: undefined, lastMs: res.ms });
      if (parsed.advice.speak) {
        // AI advice never outranks the rule engine's urgent safety calls in the speech queue.
        const cue: Cue = { key: `ai:${trigger}:${parsed.advice.focus}`, text: parsed.advice.speak, priority: parsed.advice.priority === 'High' ? 'Normal' : parsed.advice.priority, cooldown_secs: trigger === 'periodic' ? 30 : 0 };
        this.deps.onAdvice(cue, parsed.advice, meta);
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
    const label: Record<string, string> = { config: 'Configuration', network: 'Network/CORS', http: 'Gateway error', timeout: 'Timeout', aborted: 'Canceled', reply: 'Unexpected reply', error: 'Error' };
    return { ok: false, message: `${label[kind] || 'Error'}: ${e instanceof Error ? e.message : String(e)}` };
  }
}
