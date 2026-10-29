/** The Player page: strengths (last match's plays and profile habits), "Improved since last
 *  time", weaknesses, per-match history and profile management. */
import type { MatchRow, PlayerProfile, Trait } from './player-profile';
import type { Snapshot } from './types';

export interface PlayerPageDeps {
  profile: PlayerProfile;
  notice: (message: string, error?: boolean) => void;
  download: (name: string, data: Blob) => void;
  /** Snapshots analyzed by the last completed replay, if any. */
  replay: () => { snapshots: Snapshot[]; label: string } | null;
}
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string) => {
  const e = document.createElement(tag); if (text != null) e.textContent = text; if (className) e.className = className; return e;
};
const TREND: Record<Trait['trend'], string> = { improving: 'Improving', worse: 'Getting worse', same: 'Steady', new: 'New' };

export class PlayerPage {
  private deps: PlayerPageDeps;
  constructor(deps: PlayerPageDeps) {
    this.deps = deps;
    $('player-hero').onchange = () => this.render();
    $('player-add-replay').onclick = () => this.addReplay();
    $('replay-add-profile').onclick = () => this.addReplay();
    $('player-export').onclick = () => deps.download('deadlock-player-profile.json', new Blob([deps.profile.exportJSON()], { type: 'application/json' }));
    $<HTMLInputElement>('player-import-file').onchange = async () => {
      const input = $<HTMLInputElement>('player-import-file'); const file = input.files?.[0]; input.value = '';
      if (!file) return;
      try {
        if (file.size > 5 * 1024 * 1024) throw new Error('Profile exports are at most 5 MB');
        const added = deps.profile.importJSON(await file.text());
        deps.notice(`Imported ${added} match${added === 1 ? '' : 'es'} into your player profile.`); this.render();
      } catch (e) { deps.notice(`Import failed: ${e instanceof Error ? e.message : String(e)}`, true); }
    };
    $('player-reset').onclick = () => {
      if (!confirm('Delete your player profile from this browser? Export it first to keep a copy.')) return;
      deps.profile.reset(); this.render(); deps.notice('Player profile reset.');
    };
  }
  /** Enable the replay buttons once a replay has finished. */
  replayReady() {
    const r = this.deps.replay(); const ready = !!r?.snapshots.length;
    for (const id of ['player-add-replay', 'replay-add-profile']) $<HTMLButtonElement>(id).disabled = !ready;
    $('player-replay-note').textContent = ready ? `Last replay: ${r!.snapshots.length} analyzed snapshots${r!.label ? ` (${r!.label})` : ''}.` : 'Run a replay in Replay lab to add it here.';
  }
  addReplay() {
    const r = this.deps.replay();
    if (!r?.snapshots.length) { this.deps.notice('Run a replay first: its results can then be added to your profile.'); return; }
    const added = this.deps.profile.addReplay(r.snapshots, r.label);
    this.deps.notice(added ? `Added ${added} match${added === 1 ? '' : 'es'} from the replay to your player profile.` : 'Nothing added: the replay was already in your profile, or it had less than two minutes of readable gameplay.');
    this.render();
  }
  render() {
    const p = this.deps.profile;
    const select = $<HTMLSelectElement>('player-hero'); const chosen = select.value;
    const all = p.summary();
    select.replaceChildren(new Option('All heroes', ''), ...all.heroes.map(h => new Option(`${h.name} (${h.matches})`, h.hero)));
    select.value = all.heroes.some(h => h.hero === chosen) ? chosen : '';
    const s = select.value ? p.summary(select.value) : all;
    $('player-summary').textContent = s.matches
      ? `${s.matches} match${s.matches === 1 ? '' : 'es'}${s.heroName ? ` · mostly ${s.heroName}` : ''} · ${s.observedMinutes} min of game time observed (last 10 counted matches)${s.deadPct != null ? ` · dead ${s.deadPct}% of it · ${s.deathsPer10} deaths per 10 min` : ''}${s.farmPerMin != null ? ` · ${s.farmPerMin} souls/min` : ''}${s.excluded ? ` · ${s.excluded} not counted (too short, no gameplay or misread)` : ''}.`
      : 'No matches yet. Coach a match, or add a replay, to start learning your habits.';
    // Strengths first: what to keep doing is as useful as what to fix.
    this.traits('player-strengths', s.strengths, 'No clear strengths yet: they appear as evidence builds up.');
    this.traits('player-improved', s.improvements, s.matches >= 2 ? 'No clear improvement over the matches before the last one.' : 'Shown once there are two matches to compare.');
    this.traits('player-weaknesses', s.weaknesses, 'No clear weaknesses in the evidence so far.');
    this.goals(s.rows);
    const body = $('player-matches'); body.replaceChildren();
    if (!s.rows.length) { const tr = el('tr'); const td = el('td', 'No matches recorded.'); td.colSpan = 9; tr.append(td); body.append(tr); }
    for (const r of s.rows) {
      const tr = el('tr');
      const date = new Date(r.date);
      const cells = [`${date.toLocaleDateString()} ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}${r.source === 'replay' ? ' · replay' : ''}${r.partial ? ' · partial' : ''}${r.excluded ? ` · not counted: ${r.excluded}` : ''}`,
        `${r.heroName}${r.result ? ` · ${r.result}` : ''}`, `${r.minutes} min`, String(r.deaths), r.deadPct == null ? (r.legacy ? '— (older version)' : '—') : `${r.deadPct}%`,
        `${r.fightsJoined} / ${r.fightsMissed}`, r.unspentAtDeath, r.topKiller || '—',
        r.goals.length ? r.goals.map(g => `${g.met === true ? 'Met' : g.met === false ? 'Missed' : '?'}: ${g.text}`).join(' · ') : '—'];
      for (const c of cells) tr.append(el('td', c));
      body.append(tr);
    }
    $('player-prompt').textContent = p.promptText(select.value || null) || 'Nothing learned yet.';
    this.replayReady();
  }
  /** The match focus goal (this match's or the next one's) and its history: met / missed per match. */
  private goals(rows: MatchRow[]) {
    const next = this.deps.profile.nextGoals();
    $('player-goal-next').textContent = next.goals.length
      ? `${next.current ? 'This match' : 'Next match'}: ${next.goals.map(g => g.text).join(' · ')}. Said once at the match start, checked at each death, scored at the end.`
      : 'No focus goal yet: it is chosen from the weaknesses of your last 3 matches.';
    const list = $('player-goal-history'); list.replaceChildren();
    const scored = rows.filter(r => r.goals.length).slice(0, 10);
    if (!scored.length) { list.append(el('li', 'No scored goals yet: they are scored when a coached match ends.', 'empty-row')); return; }
    for (const r of scored) for (const g of r.goals) {
      const li = el('li', undefined, `player-goal ${g.met === true ? 'met' : g.met === false ? 'missed' : 'unknown'}`);
      li.append(el('strong', g.met === true ? 'Met' : g.met === false ? 'Missed' : 'Not measurable'), document.createTextNode(` · ${new Date(r.date).toLocaleDateString()} ${r.heroName}: ${g.text} (${g.detail})`));
      list.append(li);
    }
    const met = scored.flatMap(r => r.goals).filter(g => g.met === true).length, all = scored.flatMap(r => r.goals).filter(g => g.met != null).length;
    if (all) list.prepend(el('li', `${met} of ${all} goals met in the last ${scored.length} scored match${scored.length === 1 ? '' : 'es'}.`, 'player-goal-total'));
  }
  private traits(id: string, traits: Trait[], empty: string) {
    const list = $(id); list.replaceChildren();
    if (!traits.length) { list.append(el('li', empty, 'empty-row')); return; }
    for (const t of traits) {
      const li = el('li', undefined, `player-trait ${t.kind}`);
      const head = el('div', undefined, 'player-trait-head');
      head.append(el('strong', t.label));
      // Last-match highlights and improvements are single-match facts: no trend / confidence tags.
      if (t.scope === 'last') head.append(el('span', 'Last match', 'tag'));
      else if (t.scope === 'improved') head.append(el('span', TREND.improving, 'tag player-trend improving'));
      else head.append(el('span', TREND[t.trend], `tag player-trend ${t.trend}`), el('span', `${t.confidenceLabel} confidence`, 'tag'));
      if (t.kind === 'weakness') head.append(el('span', ['', 'minor', 'notable', 'major'][t.severity], `tag player-severity s${t.severity}`));
      li.append(head, el('p', t.evidence));
      const tip = el('p', undefined, 'player-tip');
      if (t.kind === 'weakness') { tip.append(el('strong', 'Practice: '), document.createTextNode(t.tip)); li.append(tip); }
      else if (t.focus) { tip.append(el('strong', 'Coach says: '), document.createTextNode(t.focus)); li.append(tip); }
      list.append(li);
    }
  }
}
