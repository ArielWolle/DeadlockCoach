// DOM wiring for the end-of-game AI review (logic: ai-review.ts; docs/ai-coach.md "End-of-game review"):
// the "Match reviews" panel on the Player page, a short notice on the Coach page, the Settings
// checkbox and a sentence-by-sentence reader with Stop. main.ts feeds frames through the handle.
import { configFrom, validateConfig } from './ai-coach';
import { MatchTimeline, ReviewStore, ReviewTracker, autoReviewable, buildDigest, buildReviewPrompt, requestReview, reviewKey, sentencesOf, storedReview, type EndedMatch, type StoredReview } from './ai-review';
import { spanOf, type MatchRecord } from './player-profile';
import type { Cue, Knowledge, Settings, Snapshot } from './types';

export interface ReviewUiDeps {
  settings: () => Settings; save: (s: Settings) => void;
  /** Earlier matches for the comparison (the Player profile, newest first). */
  previous: () => MatchRecord[];
  knowledge: () => Promise<Knowledge[]>; name: (id: string) => string;
  notice: (message: string, error?: boolean) => void;
  download: (name: string, data: Blob) => void;
  /** Stop the coach's own speech before a review is read aloud. */
  stopSpeech: () => void;
  /** Whether live capture is running (a match in progress is then not closed by "Review my last match"). */
  isLive?: () => boolean;
  /** A live match was closed here (Stop, "Review my last match"): close it in the Player profile too. */
  finalized?: () => void;
}
export interface ReviewUi {
  /** A live full-scan frame (with its rule cues). */
  live: (s: Snapshot, wall: number, cues: Cue[]) => void;
  /** Live capture stopped (Stop button or the share ended): a match observed >= 10 min is over and reviewed. */
  stopped: () => void;
  replayStart: () => void; replayFrame: (s: Snapshot, cues: Cue[]) => void; replayDone: (complete: boolean) => void;
  reviewLast: () => Promise<void>;
  populate: (s: Settings) => void; read: (next: Settings) => void; render: () => void;
  store: ReviewStore; stop: () => void;
}
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string) => {
  const e = document.createElement(tag); if (text != null) e.textContent = text; if (className) e.className = className; return e;
};

/** Reads a long text sentence by sentence (the coach Speaker keeps one pending cue for 15 s and
 *  stops an utterance after 25 s, too short for a 1-2 minute review). Each sentence has its own
 *  watchdog; another speech cancel (a High coach call, mute) stops the reading. */
export class ReviewReader {
  private gen = 0; private timer?: ReturnType<typeof setTimeout>;
  speaking: string | null = null;
  onChange: () => void = () => {};
  private settings: () => Settings;
  constructor(settings: () => Settings) { this.settings = settings; }
  speak(text: string, id: string): boolean {
    this.stop();
    const synth = window.speechSynthesis; const parts = sentencesOf(text);
    if (!synth || !parts.length) return false;
    const gen = ++this.gen; let i = 0; this.speaking = id; this.onChange();
    const s = this.settings();
    const voices = synth.getVoices?.() || [];
    const voice = voices.find(v => v.voiceURI === s.voice) || voices.find(v => v.localService && v.lang.startsWith('en'));
    const next = () => {
      if (gen !== this.gen) return;
      if (i >= parts.length) { this.finish(gen); return; }
      const sentence = parts[i++]; const u = new SpeechSynthesisUtterance(sentence);
      if (voice) u.voice = voice; u.rate = s.rate; u.volume = s.volume;
      let done = false;
      const budget = 5000 + sentence.split(' ').length * 700 / Math.max(0.5, s.rate);
      this.timer = setTimeout(() => { if (done || gen !== this.gen) return; done = true; synth.cancel(); next(); }, budget);
      u.onend = () => { if (done) return; done = true; clearTimeout(this.timer); next(); };
      u.onerror = e => {
        if (done) return; done = true; clearTimeout(this.timer);
        if (e.error === 'interrupted' || e.error === 'canceled') this.finish(gen); else next();
      };
      synth.speak(u);
    };
    next();
    return true;
  }
  private finish(gen: number) { if (gen !== this.gen) return; this.gen++; clearTimeout(this.timer); this.speaking = null; this.onChange(); }
  stop() {
    const was = this.speaking != null; this.gen++; clearTimeout(this.timer); this.speaking = null;
    if (was) { window.speechSynthesis?.cancel(); this.onChange(); }
  }
}

export function mountMatchReviews(deps: ReviewUiDeps): ReviewUi {
  const store = new ReviewStore((() => { try { return localStorage; } catch { return null; } })());
  const liveTracker = new ReviewTracker('live');
  let replayTracker: ReviewTracker | null = null, replayEnded: EndedMatch | null = null;
  let last: EndedMatch | null = null; let busy = false; let open: string | null = null;
  /** When `last` was set and when the live tracker last saw a frame (the newer one is "the last match"). */
  let lastAt = 0, liveAt = 0;
  /** Matches already handled automatically (one automatic review per match). */
  const autoDone = new Set<string>();
  const reader = new ReviewReader(deps.settings);
  // Speak / Stop states update in place (a re-render would drop keyboard focus).
  reader.onChange = () => {
    for (const b of document.querySelectorAll<HTMLButtonElement>('#review-list button[data-speak]')) { const on = reader.speaking === b.dataset.speak; b.disabled = on; b.textContent = on ? 'Speaking…' : 'Speak'; }
    for (const b of document.querySelectorAll<HTMLButtonElement>('#review-list button[data-stop]')) b.disabled = reader.speaking !== b.dataset.stop;
  };
  const status = (text: string, error = false) => { const p = $('review-status'); p.textContent = text; p.classList.toggle('error', error); };

  function aiProblem(s: Settings): string | null {
    if (!s.aiEnabled) return 'Turn on the AI coach in Settings (with a gateway and model) to get match reviews.';
    try { validateConfig(configFrom(s), location.protocol); } catch (e) { return `AI coach settings: ${e instanceof Error ? e.message : String(e)}`; }
    return null;
  }
  async function run(match: EndedMatch, auto: boolean) {
    const s = deps.settings(); const key = reviewKey(match.record);
    if (store.has(key)) { open = key; render(); if (!auto) status('This match is already reviewed: it is open below.'); return; }
    const problem = aiProblem(s); if (problem) { if (!auto) status(problem, true); return; }
    if (busy) { if (!auto) status('A review is already being written. One moment.'); return; }
    busy = true; $<HTMLButtonElement>('review-last').disabled = true;
    status(`Writing your match review (${deps.name(match.record.hero || '') || 'your hero'})… this can take up to a minute.`);
    if (auto) coachText('Match over: writing your end-of-game review… it will appear on the Player page.');
    try {
      const docs = await deps.knowledge().catch(() => [] as Knowledge[]);
      const digest = buildDigest({ match, previous: deps.previous(), name: deps.name, docs });
      const prompt = buildReviewPrompt(digest, s.aiMaxTokens);
      try {
        const r = await requestReview(configFrom(s), prompt, { pageProtocol: location.protocol, clock: match.record.endClock });
        const entry = storedReview(digest, match, r); store.put(entry); open = entry.id;
        status(r.status === 'ok' ? `Match review ready (${digest.heroName}, ${digest.result}).` : 'The model did not answer in the expected format: its raw reply is shown below.', r.status !== 'ok');
        coachNotice(entry);
        const speak = r.review?.spoken || '';
        if (speak && s.aiSpeak && !s.muted) { deps.stopSpeech(); reader.speak(speak, entry.id); }
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        store.put(storedReview(digest, match, { error: message })); open = digest.key;
        status(`The review request failed: ${message}`, true);
        coachText(`The end-of-game review failed: ${message} `, 'Try again on the Player page');
      }
    } finally { busy = false; $<HTMLButtonElement>('review-last').disabled = false; render(); }
  }
  /** The short notice on the Coach page (AI coach panel), with a link to the Player page. */
  function coachText(text: string, link?: string) {
    const p = $('review-notice'); p.hidden = false; p.replaceChildren(text);
    if (link) { const a = el('a', link); a.href = '#player'; p.append(a); }
  }
  function coachNotice(r: StoredReview) {
    coachText(`Your end-of-game review is ready (${r.heroName}${r.result !== 'unknown' ? `, ${r.result}` : ''}). `, 'Read it on the Player page');
  }
  const minutes = (m: EndedMatch) => Math.round(spanOf(m.record) / 60);
  /** A finished live match: review it automatically, or say why there is no review. */
  function ended(m: EndedMatch) {
    last = m; lastAt = Date.now();
    const key = reviewKey(m.record); if (autoDone.has(key)) return; autoDone.add(key);
    const set = deps.settings();
    if (!autoReviewable(m)) {
      if (m.endedBy === 'stop' || m.byPostGame) coachText(`Match over after ${minutes(m)} min observed: too short for an automatic review. `, 'Review it on the Player page');
      return;
    }
    const problem = aiProblem(set);
    const reason = !set.aiEnabled ? 'the AI coach is off (turn it on in Settings with a gateway and model)'
      : !set.aiReview ? 'the end-of-game review is turned off in Settings' : problem?.replace(/\.$/, '') ?? null;
    if (reason) {
      coachText(`Match over (${minutes(m)} min observed). No AI review: ${reason}. `, 'Player page');
      status(`No automatic review of the last match: ${reason}.`);
      return;
    }
    void run(m, true);
  }
  function list(title: string, items: string[], ordered = false) {
    const frag = document.createDocumentFragment(); if (!items.length) return frag;
    frag.append(el('h4', title)); const ul = el(ordered ? 'ol' : 'ul'); for (const i of items) ul.append(el('li', i)); frag.append(ul); return frag;
  }
  function item(r: StoredReview) {
    const li = el('li', undefined, 'review-item');
    const d = el('details'); d.open = r.id === open; d.ontoggle = () => { if (d.open) open = r.id; else if (open === r.id) open = null; };
    const when = r.date ? new Date(r.date) : null;
    const sum = el('summary', `${when ? `${when.toLocaleDateString()} ${when.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : 'Undated'} · ${r.heroName} · ${r.result} · ${r.minutes} min · ${r.deaths} death${r.deaths === 1 ? '' : 's'}${r.source === 'replay' ? ' · replay' : ''}${r.status !== 'ok' ? (r.status === 'raw' ? ' · raw reply' : ' · failed') : ''}`);
    d.append(sum);
    const body = el('div', undefined, 'review-body');
    const v = r.review;
    if (v) {
      if (v.summary) body.append(el('p', v.summary, 'review-summary'));
      body.append(list('What went well', v.went_well));
      if (v.improvements.length) {
        body.append(el('h4', 'Top improvements'));
        const ol = el('ol', undefined, 'review-improvements');
        for (const i of v.improvements) {
          const x = el('li'); x.append(el('strong', i.title));
          if (i.evidence) x.append(el('p', `Evidence: ${i.evidence}`)); if (i.why_it_matters) x.append(el('p', `Why it matters: ${i.why_it_matters}`));
          if (i.how_to_practice) x.append(el('p', `How to practice: ${i.how_to_practice}`)); ol.append(x);
        }
        body.append(ol);
      }
      body.append(list('Keep doing', v.keep_doing || []));
      if (v.next_game_focus) { const p = el('p', undefined, 'review-focus'); p.append(el('strong', 'Next game: '), v.next_game_focus); body.append(p); }
      body.append(list('Hero tips', v.hero_tips));
      if (v.partial) body.append(el('p', 'The reply was cut off; this is the part that arrived.', 'muted'));
      if (v.spoken) {
        const actions = el('div', undefined, 'replay-actions');
        const speak = el('button', reader.speaking === r.id ? 'Speaking…' : 'Speak'); speak.type = 'button'; speak.disabled = reader.speaking === r.id; speak.dataset.speak = r.id;
        speak.setAttribute('aria-label', `Speak the review of the ${r.heroName} match`);
        speak.onclick = () => {
          const s = deps.settings();
          if (s.muted) { status('The voice is muted: unmute it to hear the review.'); return; }
          deps.stopSpeech(); if (!reader.speak(v.spoken, r.id)) status('Speech is not available in this browser.', true);
        };
        const stop = el('button', 'Stop', 'secondary'); stop.type = 'button'; stop.disabled = reader.speaking !== r.id; stop.dataset.stop = r.id;
        stop.setAttribute('aria-label', `Stop reading the ${r.heroName} review`); stop.onclick = () => reader.stop();
        actions.append(speak, stop); body.append(actions);
        const sp = el('details'); sp.append(el('summary', 'Spoken version'), el('p', v.spoken, 'muted')); body.append(sp);
      }
    } else if (r.status === 'raw' && r.raw) {
      body.append(el('p', 'The model did not reply in the expected format. Its raw text:', 'muted'));
      const pre = el('pre', r.raw, 'review-raw'); pre.tabIndex = 0; pre.setAttribute('aria-label', 'Raw review reply'); body.append(pre);
    } else body.append(el('p', `The review failed: ${r.error || 'no reply'}. Press "Review my last match" to try again.`, 'ai-error'));
    if (r.model) body.append(el('p', `${r.model} · digest ${r.digestChars.toLocaleString('en-US')} characters`, 'muted'));
    d.append(body); li.append(d); return li;
  }
  function render() {
    const ul = $('review-list'); ul.replaceChildren();
    if (!store.reviews.length) {
      const s = deps.settings();
      const why = !s.aiEnabled ? ' The AI coach is off: turn it on in Settings (with a gateway and model) to get them.' : !s.aiReview ? ' Automatic reviews are turned off in Settings.' : '';
      ul.append(el('li', `No match reviews yet. With the AI coach on, a review is written when a match ends (a Victory / Defeat or post-game screen, the loading screens after it, or Stop after 10+ minutes); or press "Review my last match".${why}`, 'empty-row'));
    }
    for (const r of store.reviews) ul.append(item(r));
    $<HTMLButtonElement>('review-export').disabled = !store.reviews.length;
  }
  $('review-last').onclick = () => { void reviewLast(); };
  $('review-export').onclick = () => deps.download('deadlock-match-reviews.json', new Blob([store.exportJSON()], { type: 'application/json' }));
  async function reviewLast() {
    // The live match the tracker still holds open (no end screen seen, capture stopped < 10 min
    // in, or never stopped) is the last match when it is newer than the last finished one: closed
    // first when capture is no longer running, else reviewed as it is so far.
    let match = last;
    if (liveTracker.current() && liveAt >= lastAt) {
      if (deps.isLive?.()) match = liveTracker.current();
      else { const m = liveTracker.finish(); if (m) { match = m; last = m; lastAt = Date.now(); autoDone.add(reviewKey(m.record)); deps.finalized?.(); } }
    }
    if (!match) {
      // After a reload the live timeline is gone; the Player profile still has the match.
      const rec = deps.previous().find(r => r.source === 'live');
      if (rec) match = { record: rec, timeline: MatchTimeline.empty(), byPostGame: false, source: 'live', endedBy: 'finish' };
    }
    if (!match) { status('No match to review yet: coach a match, or run a replay in Replay lab first.'); return; }
    await run(match, false);
  }
  render();
  return {
    store, stop: () => reader.stop(), render, reviewLast,
    live(s, wall, cues) {
      liveAt = Date.now();
      const m = liveTracker.add(s, wall, cues); if (m) ended(m);
    },
    stopped() {
      const m = liveTracker.stop();
      if (m) { deps.finalized?.(); ended(m); return; }
      const cur = liveTracker.current();
      if (cur) coachText(`Capture stopped after ${minutes(cur)} min of this match: it stays open (an automatic review needs 10 minutes). `, 'Review it on the Player page');
    },
    replayStart() { replayTracker = new ReviewTracker('replay'); replayEnded = null; },
    replayFrame(s, cues) { const e = replayTracker?.add(s, null, cues); if (e) replayEnded = e; },
    replayDone(complete) {
      const m = replayTracker?.finish() ?? replayEnded; replayTracker = null; replayEnded = null;
      if (complete && m) { last = m; lastAt = Date.now(); }
    },
    populate(s) { $<HTMLInputElement>('ai-review').checked = s.aiReview; },
    read(next) { next.aiReview = $<HTMLInputElement>('ai-review').checked; },
  };
}
