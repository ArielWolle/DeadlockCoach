/** The Player page: strengths, weaknesses, per-match history and profile management. */
import type { PlayerProfile, Trait } from './player-profile';
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
      ? `${s.matches} match${s.matches === 1 ? '' : 'es'}${s.heroName ? ` · mostly ${s.heroName}` : ''} · ${s.observedMinutes} min observed (last 10 matches)${s.deadPct != null ? ` · ${s.deadPct}% dead · ${s.deathsPer10} deaths per 10 min` : ''}${s.farmPerMin != null ? ` · ${s.farmPerMin} souls/min` : ''}.`
      : 'No matches yet. Coach a match, or add a replay, to start learning your habits.';
    this.traits('player-weaknesses', s.weaknesses, 'No clear weaknesses in the evidence so far.');
    this.traits('player-strengths', s.strengths, 'No clear strengths yet: they appear as evidence builds up.');
    const body = $('player-matches'); body.replaceChildren();
    if (!s.rows.length) { const tr = el('tr'); const td = el('td', 'No matches recorded.'); td.colSpan = 8; tr.append(td); body.append(tr); }
    for (const r of s.rows) {
      const tr = el('tr');
      const date = new Date(r.date);
      const cells = [`${date.toLocaleDateString()} ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}${r.source === 'replay' ? ' · replay' : ''}${r.partial ? ' · partial' : ''}`,
        `${r.heroName}${r.result ? ` · ${r.result}` : ''}`, `${r.minutes} min`, String(r.deaths), r.deadPct == null ? '—' : `${r.deadPct}%`,
        `${r.fightsJoined} / ${r.fightsMissed}`, r.unspentAtDeath, r.topKiller || '—'];
      for (const c of cells) tr.append(el('td', c));
      body.append(tr);
    }
    $('player-prompt').textContent = p.promptText(select.value || null) || 'Nothing learned yet.';
    this.replayReady();
  }
  private traits(id: string, traits: Trait[], empty: string) {
    const list = $(id); list.replaceChildren();
    if (!traits.length) { list.append(el('li', empty, 'empty-row')); return; }
    for (const t of traits) {
      const li = el('li', undefined, `player-trait ${t.kind}`);
      const head = el('div', undefined, 'player-trait-head');
      head.append(el('strong', t.label), el('span', TREND[t.trend], `tag player-trend ${t.trend}`), el('span', `${t.confidenceLabel} confidence`, 'tag'));
      if (t.kind === 'weakness') head.append(el('span', ['', 'minor', 'notable', 'major'][t.severity], `tag player-severity s${t.severity}`));
      li.append(head, el('p', t.evidence));
      if (t.kind === 'weakness') { const tip = el('p', undefined, 'player-tip'); tip.append(el('strong', 'Practice: '), document.createTextNode(t.tip)); li.append(tip); }
      list.append(li);
    }
  }
}
