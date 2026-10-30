// Start-of-game pep talk (docs/ai-coach.md "Start-of-game pep talk"). Pure and DOM-free: the
// match-start watch (once per match, after the roster latched, never for a match joined late),
// the pre-game digest (last game, this match's focus goals, the enemy heroes that threaten the
// player most), the rule-based template briefing every player gets, and the optional AI prompt
// with its strict-JSON reply parser. The UI (panel, reading aloud, Stop / Replay) is pregame-ui.ts.
import { CLOSE_RANGE_HEROES, GatewayError, HERO_NAMES, HERO_ROLES, aiExchanges, callGateway, extractText, jsonCandidates, type AiConfig, type CallOptions } from './ai-coach.ts';
import { repairJson, speakable } from './ai-review.ts';
import { PlayerProfileModel, deadOf, excludedReason, spanOf, spendTarget, type KeyValueStore, type MatchGoal, type MatchRecord, type PlayerSummary, type Trait } from './player-profile.ts';
import type { Snapshot } from './types';

const finite = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);
const words = (s: string) => s.split(/\s+/).filter(Boolean).length;
const cap = (s: string) => s ? s[0].toUpperCase() + s.slice(1) : s;
const end = (s: string) => /[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`;
const grouped = (n: number) => Math.round(n).toLocaleString('en-US');
const fallbackName = (id: string) => HERO_NAMES[id] || id.replace(/^hero_/, '').split('_').map(cap).join(' ');

// ---------------------------------------------------------------------------------------------
// When: once per match, after the roster latched (rec12: the vision kept the previous match's
// roster for 0:31-0:42 and the hero only settled later), never for a match joined late (rec13
// starts at 5:52).
// ---------------------------------------------------------------------------------------------
export const PREGAME = {
  /** No briefing for a match first seen after this game clock (joined late, rec13 5:52). */
  max_start_secs: 180,
  /** Earliest game clock for the briefing once the roster is known. */
  min_clock_secs: 35,
  /** Speak at this clock even when the roster is not known yet (then without matchup lines). */
  deadline_secs: 90,
  /** Missed the window (no frames, a long death screen): no briefing after this clock. */
  late_secs: 150,
  /** Fresh roster reads needed, and votes a hero needs to count as read. */
  min_reads: 3, min_votes: 2,
  /** A clock this far below the last read, or this long without a frame, is a new match. */
  clock_back_secs: 120, wall_gap_ms: 5 * 60_000,
  /** Spoken length budget. */
  max_words: 120,
} as const;

export interface Roster { hero: string | null; heroSure: boolean; allies: string[]; enemies: string[] }
export interface MatchStart { key: string; clock: number; wall: number | null }
export type PregameEvent = { type: 'ready'; start: MatchStart; roster: Roster; clock: number }
  | { type: 'skipped'; start: MatchStart; reason: 'late_start' | 'missed_window'; clock: number };

const rosterKey = (xs: string[] | undefined) => (xs || []).filter(h => /^hero_/.test(h)).slice().sort().join(',');
const top = (votes: Map<string, number>, min: number, n: number) => [...votes.entries()].filter(([, v]) => v >= min).sort((a, b) => b[1] - a[1]).slice(0, n).map(([h]) => h);
const bump = (m: Map<string, number>, k: string) => m.set(k, (m.get(k) || 0) + 1);

/** Detects match starts from the live frames and says when the briefing is due. Feed every live
 *  full-scan frame in order; at most one event per match. */
export class PregameWatch {
  private lastClock: number | null = null; private lastWall: number | null = null;
  private start: MatchStart | null = null; private state: 'idle' | 'waiting' | 'done' = 'idle';
  private reads = 0; private enemyVotes = new Map<string, number>(); private allyVotes = new Map<string, number>(); private heroVotes = new Map<string, number>();
  /** Rosters of the previous match: the vision can hold them into the next match's first reads. */
  private stale = new Set<string>(); private seen = new Map<string, number>();
  private countdown = false; private n = 0;
  get waiting() { return this.state === 'waiting'; }
  get current(): MatchStart | null { return this.start; }

  frame(s: Snapshot, wall: number | null = null): PregameEvent | null {
    if (!s || typeof s !== 'object') return null;
    const w = finite(wall) ? wall : null;
    // The pre-match countdown (rec12 7:33): the next early clock is a new match even when the
    // previous one was short. Loading screens also occur mid-match (rec12 9:36): not a boundary.
    if (s.screen === 'Countdown') { this.countdown = true; return null; }
    const read = finite(s.game_clock_secs) ? Math.round(s.game_clock_secs) : null;
    const hud = s.screen === 'Gameplay' || s.screen === 'Dead' || s.screen === 'Scoreboard' || (s.screen == null && read != null);
    if (read == null || !hud) return null;
    const gap = w != null && this.lastWall != null ? w - this.lastWall : null;
    const fresh = !this.start || (this.lastClock != null && read + PREGAME.clock_back_secs < this.lastClock) || (gap != null && gap > PREGAME.wall_gap_ms)
      || (this.countdown && read < 60 && (this.lastClock == null || read < this.lastClock));
    this.countdown = false;
    this.lastClock = read; if (w != null) this.lastWall = w;
    if (fresh) {
      if (this.start) this.close();
      this.start = { key: `pregame:${w != null ? Math.round(w / 1000) : `n${++this.n}`}:${read}`, clock: read, wall: w };
      this.reads = 0; this.enemyVotes.clear(); this.allyVotes.clear(); this.heroVotes.clear();
      if (read > PREGAME.max_start_secs) { this.state = 'done'; return { type: 'skipped', start: this.start, reason: 'late_start', clock: read }; }
      this.state = 'waiting';
    }
    const ek = rosterKey(s.enemy_heroes), ak = rosterKey(s.ally_heroes);
    if (ek) bump(this.seen, `e:${ek}`); if (ak) bump(this.seen, `a:${ak}`);
    if (this.state !== 'waiting' || !this.start) return null;
    // Votes from fresh reads only (a roster identical to the previous match's is the vision's cache).
    if (ek && !this.stale.has(`e:${ek}`)) {
      this.reads++;
      for (const h of new Set(s.enemy_heroes)) if (/^hero_/.test(h)) bump(this.enemyVotes, h);
      if (ak && !this.stale.has(`a:${ak}`)) for (const h of new Set(s.ally_heroes)) if (/^hero_/.test(h)) bump(this.allyVotes, h);
      if (s.hero && /^hero_/.test(s.hero)) bump(this.heroVotes, s.hero);
    }
    if (read > PREGAME.late_secs) { this.state = 'done'; return { type: 'skipped', start: this.start, reason: 'missed_window', clock: read }; }
    const roster = this.roster();
    const known = this.reads >= PREGAME.min_reads && roster.enemies.length >= 5;
    if ((read >= PREGAME.min_clock_secs && known) || read >= PREGAME.deadline_secs) { this.state = 'done'; return { type: 'ready', start: this.start, roster, clock: read }; }
    return null;
  }
  /** The roster voted so far (the most-read heroes, at least `min_votes` reads each). */
  roster(): Roster {
    const enemies = top(this.enemyVotes, PREGAME.min_votes, 6), allies = top(this.allyVotes, PREGAME.min_votes, 6).filter(h => !enemies.includes(h));
    const [hero, votes] = [...this.heroVotes.entries()].sort((a, b) => b[1] - a[1])[0] ?? [null, 0];
    const sure = !!hero && votes >= PREGAME.min_reads && votes >= this.reads * 0.6 && (!allies.length || allies.includes(hero)) && !enemies.includes(hero);
    return { hero: sure ? hero : null, heroSure: sure, allies, enemies };
  }
  /** The match ended (a new one began): its rosters are stale for the next match's first reads. */
  private close() {
    this.stale = new Set([...this.seen.entries()].filter(([, v]) => v >= 2).map(([k]) => k));
    this.seen.clear(); this.start = null; this.state = 'idle';
  }
}

// ---------------------------------------------------------------------------------------------
// Threat table (data/coaching/plans.json `threats`, review.json `threats` / `counters` / `roles`)
// ---------------------------------------------------------------------------------------------
export interface HeroThreat { hero: string; ability: string; danger: string; counter: { long_range: string; default: string }; need: string | null }
export interface ThreatData { heroes: Record<string, HeroThreat>; longRange: Set<string>; counterItems: Record<string, { item: string; why: string }>;
  /** Ability-level lessons (review.json threats): hero -> the next-life lesson (default / ranged). */
  lessons: Record<string, { source: string; next: string; nextRanged: string; need: string | null; close: boolean }> }
const obj = (v: unknown) => v && typeof v === 'object' ? v as Record<string, any> : {}; // eslint-disable-line @typescript-eslint/no-explicit-any
const s = (v: unknown) => typeof v === 'string' ? v.trim() : '';
/** Build the table from the raw coaching JSON (tolerant of missing parts). */
export function threatData(plansJson: unknown, reviewJson: unknown): ThreatData {
  const plans = obj(plansJson), review = obj(reviewJson);
  const heroes: ThreatData['heroes'] = {};
  for (const t of Array.isArray(plans.threats) ? plans.threats : []) {
    const o = obj(t); if (!/^hero_/.test(s(o.hero)) || !s(o.danger)) continue;
    const c = obj(o.counter); heroes[o.hero] = { hero: o.hero, ability: s(o.ability), danger: s(o.danger), counter: { long_range: s(c.long_range) || s(c.default), default: s(c.default) || s(c.long_range) }, need: s(o.need) || null };
  }
  const longRange = new Set<string>([...(Array.isArray(plans.roles) ? plans.roles : []).filter((r: unknown) => obj(r).role === 'long_range').map((r: unknown) => s(obj(r).hero)),
    ...(Array.isArray(obj(review.roles).ranged) ? obj(review.roles).ranged.map(s) : [])].filter(Boolean));
  const counterItems: ThreatData['counterItems'] = {};
  for (const c of Array.isArray(review.counters) ? review.counters : []) { const o = obj(c); const item = Array.isArray(o.items) ? s(o.items[0]) : ''; if (s(o.need) && item) counterItems[o.need] = { item, why: s(o.why) }; }
  const lessons: ThreatData['lessons'] = {};
  for (const t of Array.isArray(review.threats) ? review.threats : []) {
    const o = obj(t); if (!/^hero_/.test(s(o.hero)) || lessons[o.hero] || !s(o.next)) continue;
    lessons[o.hero] = { source: s(o.source), next: s(o.next), nextRanged: s(o.next_ranged), need: s(o.counter) || null, close: o.close === true };
  }
  return { heroes, longRange, counterItems, lessons };
}
export const EMPTY_THREATS: ThreatData = { heroes: {}, longRange: new Set(['hero_orion', 'hero_hornet']), counterItems: {}, lessons: {} };

// ---------------------------------------------------------------------------------------------
// Pre-game digest
// ---------------------------------------------------------------------------------------------
export interface GoalBrief { id: string; text: string; spoken: string }
export interface ThreatBrief { hero: string; name: string; why: string; counter: string; item: string | null; itemWhy: string | null; killedYou: number; score: number }
export interface LastGame { heroName: string; result: 'win' | 'loss' | null; minutes: number; deaths: number; deadPct: number | null; improved: boolean;
  /** One specific positive, e.g. "you were dead 7% of the time, down from 17%". */
  praise: string | null;
  /** The profile's top weakness (plain words), when the last game was rough. */
  issue: string | null; rough: boolean }
export interface PregameDigest { key: string; clock: number; hero: string | null; heroName: string | null; heroSure: boolean; role: 'long_range' | 'default';
  allies: string[]; enemies: string[]; last: LastGame | null; goals: GoalBrief[]; threats: ThreatBrief[]; matchup: string | null; text: string; chars: number }
export interface DigestInput { key: string; clock: number; roster: Roster;
  /** Completed matches, newest first (the Player profile). */
  previous: MatchRecord[];
  /** This match's focus goals (PlayerProfile.goals, chosen at its start). */
  goals: MatchGoal[];
  threats?: ThreatData; name?: (id: string) => string;
  /** Correlational matchup line (ai-coach heroMatchup), optional. */
  matchup?: string | null }

/** Each goal in plain words with a concrete trigger ("when X, do Y"). */
export const GOAL_TRIGGERS: Record<string, (clock: number) => string> = {
  won_fight_low: () => 'after we win a fight, if you are under a third of your health, heal first. The objective can wait twenty seconds',
  close_range: () => 'when a diver jumps in, step back behind your front line before you shoot',
  unspent_souls: c => `spend your souls. When you have ${grouped(spendTarget(c))}, buy at the next safe moment, never walk into a fight with an item in your pocket`,
  respawn_deaths: () => 'when you respawn, walk out with a teammate or to the closest safe wave, not straight into the fight',
  alone_deaths: () => 'when your team moves, move with them. Do not farm the far side alone',
  weak_entry: () => 'when you are under sixty percent health, heal to full before you go back in',
  forward_unseen: () => 'when three enemies are missing from the map, stay on our side of the wave',
  unused_windows: () => 'when two enemies are dead, we push the nearest tower together. Don\'t farm',
};
function goalBrief(g: MatchGoal, clock: number): GoalBrief {
  const t = GOAL_TRIGGERS[g.id]?.(clock) ?? (g.id === 'deaths' ? `at most ${g.target ?? 'a few'} deaths: when in doubt, back off, a death costs more than a kill gains` : g.text.toLowerCase());
  return { id: g.id, text: g.text, spoken: t };
}
/** The positive of an improvement in plain words. */
function improvedText(t: Trait): string | null {
  switch (t.id) {
    case 'improved_dead': return `you were dead ${t.value}% of the time, down from ${t.threshold}%`;
    case 'improved_deaths': return `you died ${t.value} times per ten minutes, down from ${t.threshold}`;
    case 'improved_windows': return 'you pushed more enemy-dead windows than before';
    case 'improved_souls': return 'you died with far fewer souls in your pocket';
    case 'improved_streak': return `you went ${Math.floor(t.value / 60)} minutes without dying`;
    default: return null;
  }
}
const highlightText = (t: Trait) => t.focus ? t.focus.replace(/^keep it up:\s*/i, '').replace(/^last game\s*/i, '').replace(/\.$/, '') : null;
/** Plain-word weakness for the honest line. */
const ISSUE: Record<string, string> = { dying: 'too much time dead', unspent_souls: 'dying with souls unspent', close_range: 'dying up close', repeat_killer: 'the same hero kept catching you',
  alone_deaths: 'dying alone', unused_windows: 'not pushing when enemies were dead', forward_unseen: 'walking forward with enemies missing', burst: 'getting burst down',
  respawn_deaths: 'dying right after respawns', weak_entry: 'going back in hurt', missed_fights: 'missing team fights', fight_lost: 'losing fights', low_hp: 'staying in on low health' };

function lastGame(previous: MatchRecord[], summary: PlayerSummary | null, name: (id: string) => string): LastGame | null {
  const m = previous.find(x => !excludedReason(x)) ?? previous.find(x => spanOf(x) >= 300);
  if (!m) return null;
  const span = spanOf(m), deadPct = span >= 300 && m.deadSecs != null ? Math.round(deadOf(m) * 100 / span) : null;
  const counted = !excludedReason(m);
  const improved = counted && !!summary?.improved;
  const imp = counted ? (summary?.improvements ?? []).map(improvedText).find(Boolean) ?? null : null;
  const high = counted ? (summary?.strengths ?? []).filter(t => t.scope === 'last').map(highlightText).find(Boolean) ?? null : null;
  const per10 = span >= 300 ? m.deaths.length * 600 / span : 0;
  const rough = !improved && ((deadPct != null && deadPct >= 15) || per10 >= 2.5 || m.deaths.length >= 7);
  const weak = summary?.weaknesses?.[0];
  return { heroName: name(m.hero || '') || 'your hero', result: m.result, minutes: Math.round(span / 60), deaths: m.deaths.length, deadPct, improved,
    praise: improved ? imp ?? high : high ?? imp, issue: rough && weak ? ISSUE[weak.id] ?? weak.label.toLowerCase() : null, rough };
}
/** Counter-play for heroes outside the coach table, from the kit role (ai-coach HERO_ROLES). */
function roleCounter(role: string | undefined, longRange: boolean): string | null {
  if (!role) return null;
  if (/melee|diver|assassin|brawler/.test(role)) return longRange ? 'keep your distance and save your escape for the dive' : 'stay next to a teammate so the dive costs them';
  if (/hook|pick/.test(role)) return 'stay behind your troopers so the hook has no clean line';
  if (/sniper|marksman/.test(role)) return 'use cover when you cross open ground';
  if (/damage-over-time|spirit/.test(role)) return 'break line of sight when the damage starts ticking';
  return null;
}

/** The 1-2 enemy heroes that threaten the player most: the coach's threat table, close-range divers
 *  against a long-range hero, and the heroes that killed the player in recent matches. */
export function threatsFor(enemies: string[], hero: string | null, previous: MatchRecord[], data: ThreatData, name: (id: string) => string, max = 2): ThreatBrief[] {
  const longRange = !!hero && data.longRange.has(hero);
  const killed = new Map<string, number>();
  for (const m of previous.slice(0, 5)) for (const d of m.deaths) if (d.killer) killed.set(d.killer, (killed.get(d.killer) || 0) + 1);
  const out: ThreatBrief[] = [];
  enemies.forEach((h, order) => {
    const t = data.heroes[h], lesson = data.lessons[h], close = CLOSE_RANGE_HEROES.has(h) || lesson?.close === true;
    const k = killed.get(h) || 0;
    const score = (t ? 2 : 0) + (lesson ? 1 : 0) + (close ? (longRange ? 2 : 1) : 0) + 3 * Math.min(k, 3) - order * 0.01;
    let why = '', counter = '';
    if (t) { why = t.danger; counter = longRange ? t.counter.long_range : t.counter.default; }
    else if (lesson) { why = `${lesson.source} is the threat`; counter = (longRange && lesson.nextRanged ? lesson.nextRanged.replace(/^as \{you\},\s*/i, '') : lesson.next); }
    else { const r = HERO_ROLES[h]; const c = roleCounter(r, longRange); if (r && c) { why = `they are a ${r}`; counter = c; } }
    if (!why || !counter || score < 2) return;
    const need = t?.need ?? lesson?.need ?? null, item = need ? data.counterItems[need] ?? null : null;
    out.push({ hero: h, name: name(h), why: why.replace(/\{hero\}/g, name(h)), counter, item: item?.item ?? null, itemWhy: item ? item.why.replace(/\{hero\}/g, name(h)).replace(/\{threat\}/g, t?.ability || lesson?.source || 'it') : null, killedYou: k, score });
  });
  return out.sort((a, b) => b.score - a.score).slice(0, max);
}

export function buildPregameDigest(input: DigestInput): PregameDigest {
  const name = input.name ?? fallbackName, data = input.threats ?? EMPTY_THREATS;
  const { roster } = input;
  const model = new PlayerProfileModel();
  const previous = input.previous.filter(m => m && Array.isArray(m.deaths));
  const summary = previous.length ? model.summary(previous) : null;
  const hero = roster.heroSure ? roster.hero : null;
  const role: PregameDigest['role'] = hero && data.longRange.has(hero) ? 'long_range' : 'default';
  const last = lastGame(previous, summary, name);
  const goals = input.goals.slice(0, 2).map(g => goalBrief(g, input.clock));
  const threats = threatsFor(roster.enemies, hero, previous, data, name);
  const lines: string[] = [`PRE-GAME DIGEST (the match just started; game clock ${Math.floor(input.clock / 60)}:${String(input.clock % 60).padStart(2, '0')}; read from the screen, "not read" = unknown)`];
  lines.push(`Your hero: ${hero ? `${name(hero)}${HERO_ROLES[hero] ? ` (${HERO_ROLES[hero]})` : ''}` : 'not read yet (do not name it)'}`);
  lines.push(`Allies: ${roster.allies.length ? roster.allies.filter(h => h !== hero).map(name).join(', ') : 'not read'}`);
  lines.push(`Enemies: ${roster.enemies.length ? roster.enemies.map(h => `${name(h)}${HERO_ROLES[h] ? ` (${HERO_ROLES[h]})` : ''}`).join(', ') : 'not read'}`);
  lines.push('', 'LAST GAME');
  if (last) {
    lines.push(`${last.heroName}, ${last.result ?? 'result not read'}, ${last.minutes} min observed, ${last.deaths} death${last.deaths === 1 ? '' : 's'}${last.deadPct != null ? `, dead ${last.deadPct}% of the time` : ''}`);
    lines.push(`Verdict: ${last.improved ? 'clearly improved on the games before it (praise first)' : last.rough ? 'a rough game (be honest, then turn it into this match\'s focus)' : 'steady'}`);
    if (last.praise) lines.push(`Best positive: ${last.praise}`);
    if (last.issue) lines.push(`Main issue: ${last.issue}`);
  } else lines.push('none on the profile yet (first coached match: no comparison)');
  if (summary && summary.matches >= 2) {
    const weak = summary.weaknesses.slice(0, 2).map(t => t.label), strong = summary.strengths.slice(0, 2).map(t => t.label);
    lines.push(`Profile (${summary.matches} matches): weak ${weak.length ? weak.join('; ') : 'none clear'}${strong.length ? `; strong ${strong.join('; ')}` : ''}`);
  }
  lines.push('', 'FOCUS GOALS THIS MATCH (chosen from the last matches; the trigger is how to say it)');
  if (goals.length) for (const g of goals) lines.push(`- ${g.text} | trigger: ${g.spoken}`);
  else lines.push('none: no clear pattern yet (give one simple habit for the hero, or none)');
  lines.push('', 'THREATS TO YOU (coach threat table + who killed you in recent matches)');
  if (threats.length) for (const t of threats) lines.push(`- ${t.name}: ${t.why}; counter-play: ${t.counter}${t.killedYou ? `; killed you ${t.killedYou} time${t.killedYou === 1 ? '' : 's'} in recent matches` : ''}${t.item ? `; counter item if needed: ${t.item} (${t.itemWhy})` : ''}`);
  else lines.push(roster.enemies.length ? 'none of the enemy heroes is in the coach threat table' : 'enemy roster not read yet: no matchup lines');
  if (input.matchup) lines.push('', input.matchup.slice(0, 300));
  const text = lines.join('\n');
  return { key: input.key, clock: input.clock, hero, heroName: hero ? name(hero) : null, heroSure: !!hero, role, allies: roster.allies, enemies: roster.enemies, last, goals, threats, matchup: input.matchup ?? null, text, chars: text.length };
}

// ---------------------------------------------------------------------------------------------
// Rule-based briefing (no AI, or the AI failed)
// ---------------------------------------------------------------------------------------------
export interface Briefing { spoken: string; focus: string; matchup_tips: string[]; plan: string }
const CLOSES = ['Play your game, stay patient, and let\'s take this one.', 'Calm hands, smart fights. Let\'s go win it.', 'You know what to do. Let\'s have a great game.'];
const hash = (x: string) => [...x].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7);
const ORD = ['Focus one', 'Focus two'];
function lastSentence(l: LastGame | null, short = false): string | null {
  if (!l) return 'First coached match on your profile, so let\'s set a strong baseline.';
  if (l.improved && l.praise) return `Last game was a real step up: ${l.praise}. That's the standard now.`;
  if (l.rough) {
    const facts = `${l.deaths} death${l.deaths === 1 ? '' : 's'}${l.deadPct != null ? ` and ${l.deadPct}% of the game spent dead` : ''}`;
    return short || !l.praise ? `Honest look at last game: ${facts}. Today we clean that up.` : `Honest look at last game: ${facts}, but ${l.praise}. Today we clean up the rest.`;
  }
  if (l.praise) return `Last game ${l.result === 'win' ? 'was a win' : 'was solid'}: ${l.praise}. Keep that.`;
  return `Last game ${l.result === 'win' ? 'was a win' : l.result === 'loss' ? 'was a loss, but a steady one' : 'was steady'}. Let's build on it.`;
}
function threatSentence(t: ThreatBrief, withItem: boolean): string {
  const caught = t.killedYou >= 2 ? `${t.name} caught you ${t.killedYou === 2 ? 'twice' : `${t.killedYou} times`} recently` : `Watch ${t.name}`;
  const s1 = t.why.startsWith('they are ') ? `${caught}, ${t.why.slice(9)}: ${t.counter}.` : `${caught}: ${t.why}, so ${t.counter}.`;
  return withItem && t.item ? `${s1} If ${t.name} gets ahead, ${t.item} is your counter item.` : s1;
}
/** The template briefing from the digest, at most `PREGAME.max_words` spoken words. */
export function templateBriefing(d: PregameDigest, maxWords: number = PREGAME.max_words): Briefing {
  const close = d.last?.improved ? 'Same discipline as last game. Let\'s go.' : CLOSES[hash(d.key) % CLOSES.length];
  const goalLines = d.goals.map((g, i) => `${d.goals.length > 1 ? ORD[i] : 'Your focus this match'}: ${g.spoken}.`);
  const build = (o: { short: boolean; item: boolean; threats: number; goals: number }) => [lastSentence(d.last, o.short), ...goalLines.slice(0, o.goals), ...d.threats.slice(0, o.threats).map((t, i) => threatSentence(t, o.item && i === 0)), close]
    .filter((x): x is string => !!x).map(end).join(' ');
  const options = [{ short: false, item: true, threats: 2, goals: 2 }, { short: false, item: false, threats: 2, goals: 2 }, { short: true, item: false, threats: 2, goals: 2 },
    { short: true, item: false, threats: 1, goals: 2 }, { short: true, item: false, threats: 1, goals: 1 }, { short: true, item: false, threats: 0, goals: 1 }];
  let spoken = build(options[options.length - 1]);
  for (const o of options) { const t = build(o); if (words(t) <= maxWords) { spoken = t; break; } }
  spoken = speakable(spoken, maxWords);
  const focus = d.goals.length ? d.goals.map(g => end(cap(g.spoken))).join(' ') : 'Stay alive, farm your lane, and group up when enemies die.';
  const matchup_tips = d.threats.map(t => `${t.name}: ${t.counter}${t.item ? ` (counter item: ${t.item})` : ''}`);
  const plan = d.role === 'long_range' ? 'Laning: farm from max range behind your troopers; after 8 minutes, group for objectives and shoot from behind the front line.'
    : 'Laning: secure your wave and trade only when it is safe; after 8 minutes, group up and turn kills into towers.';
  return { spoken, focus, matchup_tips, plan };
}

// ---------------------------------------------------------------------------------------------
// AI prompt and reply
// ---------------------------------------------------------------------------------------------
export const PREGAME_SYSTEM_PROMPT = `You are an elite Deadlock coach (Valve's 6v6 hero-shooter MOBA: three lanes with Guardians, Walkers, Shrines and the Patron; souls and item tiers of 800 / 1,600 / 3,200 / 6,400; the Midboss; the Soul Urn). A new match just started. Give the player a short, personal, motivating start-of-game pep talk in their ear: what to focus on THIS game.
You get a PRE-GAME DIGEST read from the screen by an automatic coach. Rules:
- Use only facts in the digest. Never invent stats, items, abilities or events. Heroes: only the ones listed for THIS match (allies, enemies, your hero); if your hero is "not read yet", do not name it.
- Structure of "spoken", in this order: (1) one line about the last game: praise first when the Verdict says improved (name the specific positive), honest when it was rough (one fact, no lecture); (2) the 1-2 FOCUS GOALS in plain words, each with its concrete trigger ("when two enemies are dead, we push, don't farm"); (3) the 1-2 enemy heroes from THREATS TO YOU with ONE counter-play each (a counter item only if the digest lists one, at most one item); (4) a short motivating close.
- Plain, warm, direct, like a coach before the game: "you" for the player, "we" for the team. No jargon walls, no lists, no symbols, no abbreviations, no numbers you were not given.
- "spoken": at most 110 words (20-40 seconds of speech), short sentences.
Reply with ONE JSON object only (no markdown, no code fence):
{"spoken": "the pep talk for text-to-speech", "focus": "one sentence: this match's focus", "matchup_tips": ["Hero: one counter-play"], "plan": "one sentence: the plan for the first ten minutes"}`;
export const PREGAME_TIMEOUT_MS = 30_000;
export interface PregamePrompt { system: string; user: string; chars: number; maxTokens: number }
export function buildPregamePrompt(d: PregameDigest, aiMaxTokens = 1024): PregamePrompt {
  const user = ['Write the start-of-game pep talk for this player.', '', d.text, '', 'Reply with the JSON object only.'].join('\n');
  return { system: PREGAME_SYSTEM_PROMPT, user, chars: PREGAME_SYSTEM_PROMPT.length + user.length, maxTokens: Math.min(4096, Math.max(1024, Math.round(aiMaxTokens))) };
}
export type PregameParse = { ok: true; briefing: Briefing; partial: boolean } | { ok: false; error: string };
const clean = (v: unknown, n = 400) => typeof v === 'string' ? v.replace(/\s+/g, ' ').trim().slice(0, n) : '';
/** Heroes a reply may name: this match's roster (and the player's hero). 'Seven' is also a word. */
export function foreignHeroes(text: string, allowed: string[]): string[] {
  const ok = new Set(allowed.map(h => HERO_NAMES[h]).filter(Boolean));
  return [...new Set(Object.entries(HERO_NAMES).map(([, n]) => n))].filter(n => n !== 'Seven' && !ok.has(n) && new RegExp(`(?<![\\w'])${n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w])`).test(text));
}
/** Parse the strict-JSON reply (fenced, prefixed, or cut off); `allowed` = this match's heroes. */
export function parsePregame(raw: string, allowed: string[] | null = null, maxWords: number = PREGAME.max_words): PregameParse {
  const text = raw.replace(/<think>[\s\S]*?(<\/think>|$)/gi, '').replace(/```(?:json)?/gi, '').trim();
  let o: Record<string, unknown> | null = null, partial = false;
  for (const c of jsonCandidates(text)) { try { const v = JSON.parse(c); if (v && typeof v === 'object' && !Array.isArray(v) && 'spoken' in v) { o = v; break; } } catch { /* next */ } }
  if (!o) { const v = repairJson(text); if (v && 'spoken' in v) { o = v; partial = true; } }
  if (!o) return { ok: false, error: `Reply was not the requested JSON: "${text.replace(/\s+/g, ' ').slice(0, 120)}"` };
  const spoken = speakable(clean(o.spoken, 2000), maxWords);
  if (words(spoken) < 8) return { ok: false, error: 'The reply had no usable "spoken" text.' };
  const tips = (Array.isArray(o.matchup_tips) ? o.matchup_tips : typeof o.matchup_tips === 'string' ? [o.matchup_tips] : []).map(x => clean(x, 200)).filter(Boolean).slice(0, 3);
  const briefing: Briefing = { spoken, focus: clean(o.focus, 300), matchup_tips: tips, plan: clean(o.plan, 300) };
  if (allowed) { const bad = foreignHeroes([briefing.spoken, ...briefing.matchup_tips].join(' '), allowed); if (bad.length) return { ok: false, error: `The reply named heroes who are not in this match (${bad.join(', ')}).` }; }
  return { ok: true, briefing, partial };
}
export interface PregameResult { briefing: Briefing; model: string; ms: number; raw: string; partial: boolean }
/** Ask the gateway; throws on network / HTTP / parse failure (the caller falls back to the template). */
export async function requestPregame(cfg: AiConfig, prompt: PregamePrompt, allowed: string[] | null, opts: Omit<CallOptions, 'maxTokens'> = {}): Promise<PregameResult> {
  const started = Date.now();
  let raw = '', model = cfg.model;
  try { const r = await callGateway(cfg, prompt.system, prompt.user, { timeoutMs: PREGAME_TIMEOUT_MS, trigger: 'pregame', ...opts, maxTokens: prompt.maxTokens }); raw = r.text; model = r.model; }
  catch (e) {
    if (!(e instanceof GatewayError && e.kind === 'truncated')) throw e;
    const x = aiExchanges().at(-1); try { raw = x?.reply ? extractText(cfg.provider, JSON.parse(x.reply)) : ''; } catch { raw = x?.reply || ''; }
    if (!raw) throw e;
  }
  const p = parsePregame(raw, allowed);
  if (!p.ok) throw new Error(p.error);
  return { briefing: p.briefing, model, ms: Date.now() - started, raw, partial: p.partial };
}

// ---------------------------------------------------------------------------------------------
// The last briefing (kept across reloads)
// ---------------------------------------------------------------------------------------------
export const PREGAME_STORE_KEY = 'deadlock-coach-pregame-v1';
export interface StoredBriefing extends Briefing { key: string; createdAt: number; clock: number; heroName: string | null; enemies: string[]; source: 'ai' | 'template'; model: string | null;
  /** Why the template was used with the AI coach on (the AI failed), else null. */
  note: string | null }
export function sanitizeBriefing(v: unknown): StoredBriefing | null {
  const o = obj(v); if (typeof o.key !== 'string' || !o.key.startsWith('pregame:') || typeof o.spoken !== 'string' || !o.spoken.trim()) return null;
  return { key: o.key.slice(0, 80), createdAt: finite(o.createdAt) ? o.createdAt : 0, clock: finite(o.clock) ? o.clock : 0, heroName: typeof o.heroName === 'string' ? o.heroName.slice(0, 40) : null,
    enemies: Array.isArray(o.enemies) ? o.enemies.filter((x: unknown) => typeof x === 'string').slice(0, 6).map((x: string) => x.slice(0, 40)) : [], source: o.source === 'ai' ? 'ai' : 'template',
    model: typeof o.model === 'string' ? o.model.slice(0, 120) : null, note: typeof o.note === 'string' ? o.note.slice(0, 300) : null,
    spoken: speakable(clean(o.spoken, 2000), 160), focus: clean(o.focus, 300), matchup_tips: Array.isArray(o.matchup_tips) ? o.matchup_tips.map((x: unknown) => clean(x, 200)).filter(Boolean).slice(0, 3) : [], plan: clean(o.plan, 300) };
}
export class PregameStore {
  last: StoredBriefing | null = null;
  private store: KeyValueStore | null; private key: string;
  constructor(store: KeyValueStore | null = null, key = PREGAME_STORE_KEY) {
    this.store = store; this.key = key;
    try { const t = store?.getItem(key); if (t) this.last = sanitizeBriefing(JSON.parse(t)?.last); } catch { this.last = null; }
  }
  put(b: StoredBriefing) { this.last = b; try { this.store?.setItem(this.key, JSON.stringify({ schema: 1, last: b })); } catch { /* quota: memory only */ } }
  clear() { this.last = null; try { this.store?.removeItem(this.key); } catch { /* ignore */ } }
}
export function storedBriefing(d: PregameDigest, b: Briefing, source: 'ai' | 'template', extra: { model?: string | null; note?: string | null; now?: number } = {}): StoredBriefing {
  return { ...b, key: d.key, createdAt: extra.now ?? Date.now(), clock: d.clock, heroName: d.heroName, enemies: d.enemies, source, model: extra.model ?? null, note: extra.note ?? null };
}
