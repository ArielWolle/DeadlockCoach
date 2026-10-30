// DOM wiring for the start-of-game pep talk (logic: pregame.ts; docs/ai-coach.md "Start-of-game pep
// talk"): the panel on the Coach page (text, Replay, Stop; the last briefing is kept across reloads),
// the Settings checkbox, reading aloud sentence by sentence (ReviewReader from ai-review-ui.ts), and
// the speech gate: a High safety call stops the pep talk at once, other calls wait until it ends.
import plansJson from '../../data/coaching/plans.json';
import reviewJson from '../../data/coaching/review.json';
import { configFrom, heroMatchup, validateConfig } from './ai-coach';
import { ReviewReader } from './ai-review-ui';
import { PREGAME, PregameStore, PregameWatch, buildPregameDigest, buildPregamePrompt, requestPregame, storedBriefing, templateBriefing, threatData, type PregameDigest, type PregameEvent, type StoredBriefing } from './pregame';
import type { MatchGoal, MatchRecord } from './player-profile';
import type { Cue, Knowledge, Settings, Snapshot } from './types';

export interface PregameUiDeps {
  settings: () => Settings;
  /** Completed matches (the Player profile, newest first; empty when the profile is off). */
  previous: () => MatchRecord[];
  /** This match's focus goals (the Player profile chose them at the match start). */
  goals: () => MatchGoal[];
  knowledge: () => Promise<Knowledge[]>; name: (id: string) => string;
  /** The skill tracker (mastery.ts): a skill's state, and the level text for the prompt (both optional; null when the profile is off). */
  skillState?: (skill: string) => string | null; level?: () => string | null;
  /** Stop the coach's own speech before the pep talk is read. */
  stopSpeech: () => void;
  /** Speak a coach call that waited for the pep talk to finish (bypassing the gate). */
  say: (cue: Cue) => void;
  /** Add the pep talk to the Recent calls list / cue log. */
  log: (cue: Cue) => void;
}
export interface PregameUi {
  /** A live full-scan frame (never replays). */
  live: (s: Snapshot, wall: number) => void;
  /** Called for every coach call about to be spoken: true = held until the pep talk ends. A High
   *  call stops the pep talk and is spoken at once (returns false). */
  intercept: (cue: Cue) => boolean;
  stop: () => void; render: () => void;
  populate: (s: Settings) => void; read: (next: Settings) => void;
  store: PregameStore; readonly speaking: boolean;
}
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string) => {
  const e = document.createElement(tag); if (text != null) e.textContent = text; if (className) e.className = className; return e;
};
const clock = (c: number) => `${Math.floor(c / 60)}:${String(Math.round(c) % 60).padStart(2, '0')}`;

export function mountPregame(deps: PregameUiDeps): PregameUi {
  const store = new PregameStore((() => { try { return localStorage; } catch { return null; } })());
  const watch = new PregameWatch();
  const threats = threatData(plansJson, reviewJson);
  const reader = new ReviewReader(deps.settings);
  const done = new Set<string>();
  let held: { cue: Cue; at: number } | null = null, latestClock: number | null = null, busy = false;
  const state = (text: string, cls = '') => { const t = $('pregame-state'); t.textContent = text; t.className = `tag${cls ? ` ${cls}` : ''}`; };
  const status = (text: string, error = false) => { const p = $('pregame-status'); p.textContent = text; p.classList.toggle('error', error); };
  reader.onChange = () => {
    const on = reader.speaking != null;
    $<HTMLButtonElement>('pregame-stop').disabled = !on;
    const replay = $<HTMLButtonElement>('pregame-replay'); replay.disabled = on || !store.last; replay.textContent = on ? 'Speaking…' : 'Replay';
    if (!on && held) { const h = held; held = null; if (Date.now() - h.at < 15000) deps.say(h.cue); }
  };
  function render() {
    const b = store.last;
    const text = $('pregame-text'), details = $('pregame-details'), meta = $('pregame-meta');
    details.replaceChildren();
    if (!b) { text.hidden = true; meta.textContent = ''; $<HTMLButtonElement>('pregame-replay').disabled = true; return; }
    text.hidden = false; text.textContent = b.spoken;
    if (b.focus) { const p = el('p', undefined, 'pregame-focus'); p.append(el('strong', 'Focus: '), b.focus); details.append(p); }
    if (b.matchup_tips.length) { details.append(el('h4', 'Matchup')); const ul = el('ul'); for (const t of b.matchup_tips) ul.append(el('li', t)); details.append(ul); }
    if (b.plan) { const p = el('p'); p.append(el('strong', 'Plan: '), b.plan); details.append(p); }
    meta.textContent = `${b.source === 'ai' ? `AI coach${b.model ? ` (${b.model})` : ''}` : 'Rule coach template'} · ${b.heroName ?? 'hero not read'}${b.enemies.length ? ` vs ${b.enemies.map(deps.name).join(', ')}` : ''} · at ${clock(b.clock)}${b.note ? ` · ${b.note}` : ''}`;
    $<HTMLButtonElement>('pregame-replay').disabled = reader.speaking != null;
  }
  function aiUsable(s: Settings): string | null {
    if (!s.aiEnabled) return 'AI coach off';
    try { validateConfig(configFrom(s), location.protocol); return null; } catch (e) { return `AI coach settings: ${e instanceof Error ? e.message : String(e)}`; }
  }
  /** Read the briefing once the coach's current call has finished (at most ~8 s later). */
  function speak(b: StoredBriefing, attempt = 0) {
    const s = deps.settings();
    if (s.muted || store.last?.key !== b.key) return;
    if (latestClock != null && latestClock > PREGAME.late_secs + 30) return; // too late to be useful: shown only
    if (window.speechSynthesis?.speaking && attempt < 8) { setTimeout(() => speak(b, attempt + 1), 1000); return; }
    deps.stopSpeech(); held = null;
    if (!reader.speak(b.spoken, b.key)) status('Speech is not available in this browser: the pep talk is shown here.', true);
  }
  async function brief(ev: Extract<PregameEvent, { type: 'ready' }>) {
    const s = deps.settings();
    if (!s.pregame || done.has(ev.start.key) || busy) return;
    done.add(ev.start.key); busy = true;
    try {
      const docs = await Promise.race([deps.knowledge().catch(() => [] as Knowledge[]), new Promise<Knowledge[]>(r => setTimeout(() => r([]), 3000))]);
      const matchup = ev.roster.hero ? heroMatchup(docs, ev.roster.hero, ev.roster.enemies, deps.name) : null;
      const level = (() => { try { return deps.level?.() ?? null; } catch { return null; } })();
      const digest: PregameDigest = buildPregameDigest({ key: ev.start.key, clock: ev.clock, roster: ev.roster, previous: deps.previous(), goals: deps.goals(), threats, name: deps.name, matchup, skillState: deps.skillState ?? null, level });
      const template = templateBriefing(digest);
      const problem = aiUsable(s);
      let entry: StoredBriefing, speakIt = true;
      if (problem) entry = storedBriefing(digest, template, 'template');
      else {
        state('WRITING', 'teal'); status('Match started: the AI coach is writing your pep talk…');
        try {
          const allowed = [...digest.allies, ...digest.enemies, ...(digest.hero ? [digest.hero] : [])];
          const r = await requestPregame(configFrom(s), buildPregamePrompt(digest, s.aiMaxTokens), allowed.length ? allowed : null, { pageProtocol: location.protocol, clock: ev.clock });
          entry = storedBriefing(digest, r.briefing, 'ai', { model: r.model });
          speakIt = s.aiSpeak;
        } catch (e) {
          const message = e instanceof Error ? e.message : String(e);
          entry = storedBriefing(digest, template, 'template', { note: `AI unavailable (${message.slice(0, 160)}): rule coach template` });
        }
      }
      store.put(entry); render();
      state(entry.source === 'ai' ? 'AI' : 'READY', 'teal');
      status(`Pep talk for this match (${digest.heroName ?? 'hero not read yet'}${digest.enemies.length ? `, ${digest.enemies.length} enemies read` : ', enemies not read yet'}).`);
      deps.log({ key: `pregame:${entry.source}`, text: entry.spoken, priority: 'Normal', cooldown_secs: 0 });
      if (speakIt) speak(entry);
    } finally { busy = false; }
  }
  function skipped(ev: Extract<PregameEvent, { type: 'skipped' }>) {
    if (!deps.settings().pregame) return;
    state('SKIPPED');
    status(ev.reason === 'late_start' ? `Joined this match at ${clock(ev.clock)}: no pep talk (only for matches seen from the start).` : `No pep talk this match: the coach had no readable frames before ${clock(PREGAME.late_secs)}.`);
  }
  $('pregame-replay').onclick = () => {
    const b = store.last; if (!b) return;
    if (deps.settings().muted) { status('The voice is muted: unmute it to hear the pep talk.'); return; }
    deps.stopSpeech(); if (!reader.speak(b.spoken, b.key)) status('Speech is not available in this browser.', true);
  };
  $('pregame-stop').onclick = () => reader.stop();
  render(); reader.onChange();
  return {
    store, render, stop: () => reader.stop(), get speaking() { return reader.speaking != null; },
    live(s, wall) {
      if (typeof s.game_clock_secs === 'number') latestClock = s.game_clock_secs;
      const ev = watch.frame(s, wall);
      if (ev?.type === 'ready') void brief(ev); else if (ev?.type === 'skipped') skipped(ev);
      else if (watch.waiting && deps.settings().pregame && $('pregame-state').textContent !== 'WAITING') { state('WAITING'); status('Match started: your pep talk comes as soon as the heroes are read.'); }
    },
    intercept(cue) {
      if (reader.speaking == null) return false;
      if (cue.priority === 'High') { held = null; reader.stop(); return false; }
      held = { cue, at: Date.now() }; return true;
    },
    populate(s) { $<HTMLInputElement>('pregame-enabled').checked = s.pregame; },
    read(next) { next.pregame = $<HTMLInputElement>('pregame-enabled').checked; },
  };
}
