// DOM wiring for the optional AI coach: the Coach-page panel and the Settings fieldset.
// All decisions live in ai-coach.ts; main.ts only calls the handle returned by mountAiCoach().
import { AiCoach, PROVIDER_PRESETS, aiExchanges, onAiExchange, clockText, configFrom, parseExtraHeaders, testConnection, type Advice, type AdviceMeta, type AiSnapshot, type AiStatus, type AbilityRow, type ItemInfo } from './ai-coach';
import type { Cue, GameItem, Knowledge, Settings } from './types';

export interface AiUiDeps {
  settings: () => Settings; save: (s: Settings) => void;
  isLive: () => boolean; generation: () => number;
  knowledge: () => Promise<Knowledge[]>; name: (id: string) => string;
  /** Speak (if live and enabled) and list the advice; main.ts owns speech and the cue list. */
  deliver: (cue: Cue, live: boolean) => void;
  /** Called when the AI status changes (e.g. to refresh diagnostics). */
  changed?: () => void;
}
export interface AiUi {
  coach: AiCoach;
  observe: (s: AiSnapshot, cues: Cue[], live: boolean) => void;
  cancel: () => void; reset: () => void; diagnostic: () => string;
  populate: (s: Settings) => void;
  /** Copy form values into `next`; returns an error message instead of saving invalid input. */
  read: (next: Settings) => string | null;
}

export function mountAiCoach(deps: AiUiDeps): AiUi {
  const el = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
  const field = (id: string) => el<HTMLInputElement>(id);
  const setText = (id: string, value: string) => { el(id).textContent = value; };
  const states: Record<AiStatus['state'], string> = { off: 'OFF', idle: 'READY', thinking: 'THINKING…', error: 'ERROR' };
  const describe = (m: AdviceMeta) => `${m.trigger === 'manual' ? 'On demand' : m.trigger === 'death' ? 'Death review' : 'Periodic'} · ${clockText(m.clock)} · ${m.ms} ms · ${m.model} · ~${m.tokens} input tokens${m.reasoningTokens ? ` · ${m.reasoningTokens} reasoning tokens` : ''}${m.reason ? ` · why now: ${m.reason.slice(0, 120)}` : ''}`;
  function render(s: AiStatus) {
    const tag = el('ai-state'); tag.textContent = states[s.state]; tag.classList.toggle('teal', s.state === 'idle' || s.state === 'thinking');
    if (s.last) {
      const a: Advice = s.last.advice;
      setText('ai-advice', a.speak || 'PASS · nothing more important than the current calls.');
      setText('ai-why', a.why ? `Why: ${a.why}` : '');
      setText('ai-meta', `${describe(s.last.meta)}${s.state === 'thinking' ? ` · ${s.message}` : ''}`);
    } else {
      setText('ai-advice', s.message); setText('ai-why', ''); setText('ai-meta', s.state === 'off' ? '' : `${s.requests} requests so far`);
    }
    const error = el('ai-error'); error.hidden = s.state !== 'error' && !(s.state === 'idle' && /stale|canceled|in flight|skipped/i.test(s.message));
    error.textContent = s.state === 'error' ? `${s.message} ${s.error || ''}`.trim() : s.message;
    error.classList.toggle('error', s.state === 'error');
    deps.changed?.();
  }
  // The last gateway exchange, raw, so a confusing reply can be diagnosed in the page.
  onAiExchange(x => {
    const box = el('ai-raw'); if (!box) return; box.hidden = false;
    setText('ai-raw-meta', `${x.trigger} · ${clockText(x.clock)} · HTTP ${x.status ?? '—'} · ${x.ms} ms · ${x.model}${x.usage?.completion != null ? ` · ${x.usage.completion} output tokens${x.usage.reasoning ? ` (${x.usage.reasoning} reasoning)` : ''} of ${x.max_tokens ?? '?'}` : ''}${x.error ? ` · ${x.error.slice(0, 160)}` : ''}`);
    let pretty = x.reply; try { pretty = JSON.stringify(JSON.parse(x.reply), null, 2); } catch { /* not JSON: show as is */ }
    setText('ai-raw-reply', pretty || '(empty body)');
  });
  el('ai-log-download')?.addEventListener('click', () => {
    const blob = new Blob([JSON.stringify({ schema: 1, exported: new Date().toISOString(), exchanges: aiExchanges() }, null, 2)], { type: 'application/json' });
    const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = 'deadlock-ai-log.json'; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  });
  // Item names, costs and slots for the AI summary (the same asset main.ts loads for names).
  let itemInfo: Record<string, ItemInfo> = {};
  void fetch(`${import.meta.env.BASE_URL}assets/items.json`).then(r => r.ok ? r.json() as Promise<Array<GameItem & { slot?: string }>> : []).then(list => {
    itemInfo = Object.fromEntries(list.map(i => [i.class_name, { name: i.name, cost: i.cost, slot: i.slot ?? null }]));
  }).catch(() => { /* names fall back to deps.name */ });
  const coach = new AiCoach({
    settings: deps.settings, isLive: deps.isLive, generation: deps.generation, knowledge: deps.knowledge, name: deps.name,
    items: id => itemInfo[id] ?? null,
    pageProtocol: () => location.protocol, onStatus: render,
    onAdvice: (cue, _advice, meta) => deps.deliver(cue, meta.live),
  });
  // Ability names → heroes, so a death panel's "Rend" or "Boot Kick" is attributed to this match's
  // Drifter or Silver (the death review must not guess killers).
  void fetch(`${import.meta.env.BASE_URL}assets/abilities.json`).then(r => r.ok ? r.json() as Promise<AbilityRow[]> : []).then(rows => coach.history.setAbilities(rows))
    .catch(() => { /* attribution is optional */ });
  function presetHints(provider: Settings['aiProvider']) {
    const preset = PROVIDER_PRESETS[provider];
    field('ai-base-url').placeholder = `Default: ${preset.baseUrl}`; field('ai-model').placeholder = preset.modelHint;
  }
  function populate(s: Settings) {
    field('ai-enabled').checked = s.aiEnabled; el<HTMLSelectElement>('ai-provider').value = s.aiProvider;
    field('ai-base-url').value = s.aiBaseUrl; field('ai-model').value = s.aiModel;
    // The saved key is never written back into the page; an empty field keeps it.
    field('ai-key').value = ''; field('ai-key').placeholder = s.aiKey ? 'A key is saved · leave blank to keep it' : 'Not set · optional for local gateways';
    setText('ai-key-status', s.aiKey ? 'A key is saved only in this browser’s local storage. It is never logged or exported.' : 'Stored only in this browser’s local storage. Never logged or exported.');
    el<HTMLButtonElement>('ai-clear-key').disabled = !s.aiKey;
    field('ai-interval').value = String(s.aiInterval); field('ai-max-tokens').value = String(s.aiMaxTokens);
    el<HTMLSelectElement>('ai-reasoning').value = s.aiReasoning;
    field('ai-speak').checked = s.aiSpeak; el<HTMLTextAreaElement>('ai-headers').value = s.aiHeaders;
    presetHints(s.aiProvider); coach.refresh();
  }
  function read(next: Settings): string | null {
    const headers = el<HTMLTextAreaElement>('ai-headers').value.trim();
    try { parseExtraHeaders(headers); } catch (e) { return `AI coach: ${(e as Error).message}`; }
    next.aiEnabled = field('ai-enabled').checked; next.aiProvider = el<HTMLSelectElement>('ai-provider').value === 'anthropic' ? 'anthropic' : 'openai';
    next.aiBaseUrl = field('ai-base-url').value.trim(); next.aiModel = field('ai-model').value.trim();
    const key = field('ai-key').value.trim(); if (key) next.aiKey = key;
    next.aiInterval = Number(field('ai-interval').value); next.aiMaxTokens = Number(field('ai-max-tokens').value);
    const reasoning = el<HTMLSelectElement>('ai-reasoning').value; next.aiReasoning = reasoning === 'off' || reasoning === 'default' ? reasoning : 'low';
    next.aiSpeak = field('ai-speak').checked; next.aiHeaders = headers;
    if (next.aiEnabled && !next.aiModel) return 'AI coach: enter a model name, or turn the AI coach off.';
    return null;
  }
  el<HTMLSelectElement>('ai-provider').onchange = () => presetHints(el<HTMLSelectElement>('ai-provider').value === 'anthropic' ? 'anthropic' : 'openai');
  el<HTMLButtonElement>('ai-ask').onclick = () => { void coach.ask('manual'); };
  el<HTMLButtonElement>('ai-clear-key').onclick = () => {
    const s = deps.settings(); s.aiKey = ''; deps.save(s); populate(s); setText('ai-test-result', 'API key removed from this browser.');
  };
  el<HTMLButtonElement>('ai-test').onclick = async () => {
    const draft = structuredClone(deps.settings()); const problem = read(draft);
    if (problem && !/model name/.test(problem)) { setText('ai-test-result', problem); return; }
    if (coach.busy) { setText('ai-test-result', 'A coach request is in flight; try again in a moment.'); return; }
    const b = el<HTMLButtonElement>('ai-test'); b.disabled = true; setText('ai-test-result', 'Testing connection…');
    try { const r = await testConnection(configFrom(draft), { pageProtocol: location.protocol }); setText('ai-test-result', `${r.ok ? '✓' : '✗'} ${r.message}`); el('ai-test-result').classList.toggle('error', !r.ok); }
    finally { b.disabled = false; }
  };
  populate(deps.settings());
  return {
    coach, observe: (s, cues, live) => coach.observe(s, cues, live), cancel: () => coach.cancel(), reset: () => coach.reset(),
    diagnostic: () => coach.diagnostic(), populate, read,
  };
}
