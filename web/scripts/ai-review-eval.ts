// Offline evaluation of the end-of-game AI review (docs/ai-coach.md "End-of-game review").
// Builds the match digests for the rec11 (Violet, 7 deaths, a loss) and rec12 (Grey Talon, 3
// deaths) live recordings, prints their sizes and key facts, and the full rec12 prompt. Read-only
// on its inputs; no network unless AI_REVIEW_URL, AI_REVIEW_KEY and AI_REVIEW_MODEL are set (then
// it sends the rec12 prompt, or --all both, to that gateway and prints the parsed review).
// Optional: AI_REVIEW_PROVIDER=anthropic, AI_REVIEW_REASONING=off|low|default, AI_REVIEW_MAX_TOKENS.
// Keys are never printed or written.
//   node --experimental-strip-types scripts/ai-review-eval.ts [--quiet] [--all] [--digest rec11]
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ReviewTracker, buildDigest, buildReviewPrompt, requestReview, type EndedMatch } from '../src/ai-review.ts';
import type { AiConfig } from '../src/ai-coach.ts';
import type { Cue, Knowledge, Snapshot } from '../src/types.ts';
import { heroNames, profileOf } from './player-profile-replay.ts';
import { chooseGoals } from '../src/player-profile.ts';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const framesRoot = [path.join(repo, 'frames'), path.resolve(repo, '..', '..', 'DeadlockCoach', 'frames')].find(p => existsSync(p)) || path.join(repo, 'frames');
const args = process.argv.slice(2);
const quiet = args.includes('--quiet');

const docs: Knowledge[] = [];
for (const folder of ['timings', 'jungle', 'strategy', 'heroes']) for (const file of readdirSync(path.join(repo, 'docs', folder)).filter(n => n.endsWith('.md')).sort()) {
  const body = readFileSync(path.join(repo, 'docs', folder, file), 'utf8').replace(/^\uFEFF/, '');
  docs.push({ id: `${folder}/${file}`, category: folder, title: body.match(/^#\s+(.+)$/m)?.[1] || file, body });
}
const names = heroNames();
const name = (id: string) => names[id] || id.replace(/^hero_/, '').replaceAll('_', ' ');

/** The last match of a live recording export, with the rule coach's spoken texts of its time span. */
export function liveMatch(file: string, patch?: (s: Snapshot, i: number) => Snapshot): EndedMatch | null {
  const rec = JSON.parse(readFileSync(file, 'utf8')) as { frames: Array<{ timestamp: number; snapshot: Snapshot }>; spoken?: Array<{ t: number; key: string; text: string; priority: string; clock: number | null }> };
  if (patch) rec.frames.forEach((f, i) => { f.snapshot = patch(f.snapshot, i); });
  const spoken = (rec.spoken || []).filter(c => /^(death:review|review:pattern)/.test(c.key)).sort((a, b) => a.t - b.t);
  const tr = new ReviewTracker('live', () => rec.frames.at(-1)?.timestamp ?? 0);
  // Cues spoken before the first frame belong to earlier sessions in the persisted cue log.
  let last: EndedMatch | null = null, prevT = (rec.frames[0]?.timestamp ?? 0) - 5000, i = 0;
  for (const f of rec.frames) {
    const cues: Cue[] = [];
    while (i < spoken.length && spoken[i].t <= f.timestamp) { if (spoken[i].t > prevT) cues.push({ key: spoken[i].key, text: spoken[i].text, priority: 'Normal', cooldown_secs: 0 }); i++; }
    prevT = f.timestamp;
    const e = tr.add(f.snapshot, f.timestamp, cues); if (e) last = e;
  }
  return tr.finish() ?? last;
}

/** A replay's snapshots (vision replay jsonl) with the rule calls of `active_replay --all` (its
 *  "  m:ss [key] (Nw) text" lines, wrapped lines joined) fed at their game clock. */
export function replayMatch(file: string, cuesFile?: string | null): EndedMatch | null {
  const frames = readFileSync(file, 'utf8').split(/\r?\n/).filter(l => l.trim()).map(l => JSON.parse(l) as { t?: number; snapshot?: Snapshot }).filter(f => f.snapshot);
  const cues: Array<{ clock: number; key: string; text: string }> = [];
  if (cuesFile && existsSync(cuesFile)) {
    let open = false; // a wrapped call continues on the next physical lines
    for (const raw of readFileSync(cuesFile, 'utf8').split(/\r?\n/)) {
      const m = raw.match(/^[ *][ !]\s?(\d+):(\d\d) \[([^\]]+)\] \(\d+w\) (.*)$/);
      if (m) { cues.push({ clock: +m[1] * 60 + +m[2], key: m[3], text: m[4].trim() }); open = true; }
      else if (!raw.trim() || /^\s*(--|##|==|-->|\||\d+ \||cargo|At line|\+|CategoryInfo|FullyQualified|warning)/.test(raw)) open = false;
      else if (open) cues.at(-1)!.text += ` ${raw.trim()}`;
    }
  }
  const tr = new ReviewTracker('replay', () => 0);
  let last: EndedMatch | null = null, next = 0;
  for (const f of frames) {
    const c = f.snapshot!.game_clock_secs;
    const now: Cue[] = [];
    while (next < cues.length && typeof c === 'number' && cues[next].clock <= c) { const x = cues[next++]; now.push({ key: x.key, text: x.text, priority: 'Normal', cooldown_secs: 0 }); }
    const e = tr.add(f.snapshot!, typeof f.t === 'number' && f.t > 1e9 ? f.t : null, now); if (e) last = e;
  }
  return tr.finish() ?? last;
}
const replayArg = args.includes('--replay') ? args[args.indexOf('--replay') + 1] : null;
if (replayArg) {
  const cuesArg = args.includes('--cues') ? args[args.indexOf('--cues') + 1] : null;
  const match = replayMatch(replayArg, cuesArg);
  if (!match) { console.log('no match in the replay'); process.exit(0); }
  const digest = buildDigest({ match, previous: [], name, docs });
  const r = match.record;
  console.log(`${path.basename(replayArg)}: ${digest.heroName}, result ${digest.result}, ${digest.minutes} min, ${r.deaths.length} deaths at ${r.deaths.map(d => `${Math.floor(d.clock / 60)}:${String(d.clock % 60).padStart(2, '0')}`).join(' ')}, digest ${digest.chars} chars, prompt ${buildReviewPrompt(digest, 1024, null).chars} chars`);
  console.log(`  strengths ${digest.strengths.map(s => s.id).join(', ') || 'none'} · good game ${digest.goodGame}`);
  console.log(args.includes('--deaths') ? digest.text.split('\n\n').filter(p => /^DEATHS/.test(p.trim())).join('\n') : digest.text);
  process.exit(0);
}

const rec11 = path.join(framesRoot, 'rec11', 'recording.json'), rec12 = path.join(framesRoot, 'rec12', 'recording.json');
if (!existsSync(rec11) || !existsSync(rec12)) { console.log(`Recordings not found under ${framesRoot}`); process.exit(0); }
// Earlier matches in the profile: rec8 for rec11; rec8 + rec11 for rec12 (as a player would have them).
const rec8 = path.join(framesRoot, 'rec8', 'replay2.jsonl'), rec11r = path.join(framesRoot, 'rec11', 'replay1.jsonl');
const prev11 = existsSync(rec8) ? profileOf([rec8]).matches : [];
const prev12 = profileOf([rec8, rec11r].filter(f => existsSync(f))).matches;

const out: Record<string, { match: EndedMatch; digest: ReturnType<typeof buildDigest> }> = {};
for (const [label, file, previous] of [['rec11', rec11, prev11], ['rec12', rec12, prev12]] as const) {
  const match = liveMatch(file); if (!match) { console.log(`${label}: no match found`); continue; }
  const digest = buildDigest({ match, previous: [...previous], name, docs });
  const again = buildDigest({ match, previous: [...previous], name, docs });
  out[label] = { match, digest };
  const r = match.record;
  console.log(`${label}: ${digest.heroName}, result ${digest.result}, ${digest.minutes} min observed, ${r.deaths.length} deaths at ${r.deaths.map(d => `${Math.floor(d.clock / 60)}:${String(d.clock % 60).padStart(2, '0')}`).join(' ')}, ` +
    `digest ${digest.chars} chars (deterministic: ${again.text === digest.text}), ${match.timeline.points.length} frames, ${match.timeline.cues.length} rule texts, key ${digest.key}`);
  console.log(`  vs earlier: ${digest.comparisons.map(c => `${c.id} ${c.verdict}`).join(', ')}`);
  console.log(`  strengths ${digest.strengths.length} (${digest.strengths.map(s => s.id).join(', ') || 'none'}) · good game: ${digest.goodGame}`);
}
// rec13 (Grey Talon, a loss after a +46k lead): the earlier matches rec8, rec11 and rec12 give the
// focus goals (as the live profile would have at its start). The live export predates the
// match_result field: the Defeat banner the vision reads on frames 590-591 (over the death screen
// and the scoreboard) is simulated there (--no-result skips it).
const rec13 = path.join(framesRoot, 'rec13', 'recording.json'), rec12r = path.join(framesRoot, 'rec12', 'replay1.jsonl');
if (existsSync(rec13)) {
  const prev13 = profileOf([rec8, rec11r, rec12r].filter(f => existsSync(f))).matches;
  const withResult = !args.includes('--no-result');
  const match = liveMatch(rec13, (s, i) => withResult && i >= 590 && i <= 591 ? { ...s, match_result: 'loss' } : s);
  if (match) {
    const goals = chooseGoals(prev13);
    const digest = buildDigest({ match, previous: [...prev13], name, docs, goals });
    out.rec13 = { match, digest };
    console.log(`rec13: ${digest.heroName}, result ${digest.result}${withResult ? ' (match_result simulated on frames 590-591)' : ''}, ${digest.minutes} min, ${match.record.deaths.length} deaths, digest ${digest.chars} chars, focus goals ${goals.map(g => g.id).join(', ') || 'none'}`);
    const sec = (title: RegExp) => digest.text.split('\n\n').filter(p => title.test(p.trim().split('\n')[0])).join('\n\n');
    console.log(`\n===== rec13 digest: new sections =====\n${digest.text.split('\n').filter(l => /^(Hero|K\/D\/A)/.test(l)).join('\n')}\n\n${sec(/^FOCUS GOAL/)}\n\n${sec(/^LEAD AND THROW/)}\n\n${sec(/^OBJECTIVES SEEN/)}`);
  }
}
// The strengths the review must lead with (rec12: a win after a clean game).
if (out.rec12) {
  const s = out.rec12.digest.text.match(/\nSTRENGTHS[\s\S]*?(?=\n\n[A-Z])/)?.[0]?.trim();
  console.log(`\n===== rec12 STRENGTHS (as sent in the digest) =====\n${s ?? '(missing)'}`);
}
const which = args.includes('--digest') ? args[args.indexOf('--digest') + 1] : null;
if (which && out[which]) console.log(`\n===== ${which} DIGEST =====\n${out[which].digest.text}`);
if (out.rec12) {
  const prompt = buildReviewPrompt(out.rec12.digest, Number(process.env.AI_REVIEW_MAX_TOKENS) || 1024);
  console.log(`\nrec12 prompt: ${prompt.chars} chars (~${Math.ceil(prompt.chars / 4)} tokens), max reply tokens ${prompt.maxTokens}`);
  if (!quiet) console.log(`\n===== SYSTEM =====\n${prompt.system}\n\n===== USER =====\n${prompt.user}`);
}

const url = process.env.AI_REVIEW_URL, key = process.env.AI_REVIEW_KEY, model = process.env.AI_REVIEW_MODEL;
if (url && model) {
  const cfg: AiConfig = { provider: process.env.AI_REVIEW_PROVIDER === 'anthropic' ? 'anthropic' : 'openai', baseUrl: url, model, key: key || '', maxTokens: Number(process.env.AI_REVIEW_MAX_TOKENS) || 1024, headers: '',
    reasoning: (['off', 'low', 'default'] as const).find(r => r === process.env.AI_REVIEW_REASONING) ?? 'low' };
  for (const label of args.includes('--all') ? ['rec11', 'rec12'] : ['rec12']) {
    const o = out[label]; if (!o) continue;
    const prompt = buildReviewPrompt(o.digest, cfg.maxTokens);
    try {
      const r = await requestReview(cfg, prompt, { pageProtocol: 'https:' });
      console.log(`\n===== ${label} REVIEW (${r.model}, ${r.ms} ms, ${r.status}) =====`);
      if (r.review) { console.log(JSON.stringify(r.review, null, 1)); console.log(`spoken: ${r.review.spoken.split(/\s+/).length} words`); }
      else console.log(`raw: ${r.raw.slice(0, 3000)}\nerror: ${r.error}`);
    } catch (e) { console.log(`${label}: gateway error: ${e instanceof Error ? e.message : String(e)}`); }
  }
} else if (!quiet) console.log('\n(Set AI_REVIEW_URL, AI_REVIEW_KEY and AI_REVIEW_MODEL to send the prompt to a gateway.)');
