// Offline evaluation of the start-of-game pep talk (docs/ai-coach.md "Start-of-game pep talk").
// Feeds the live recordings rec11 -> rec12 -> rec13 (frames merged in wall-clock order) through
// the Player profile and the PregameWatch as main.ts liveMatchFrame does: the profile learns rec11
// (and rec12), rec12's match start at 0:31 gets a pep talk once the roster latched, rec13 (first
// seen at 5:52) gets none. Prints each event, the rule-based briefing and the AI prompt. Read-only;
// no network unless PREGAME_AI_URL and PREGAME_AI_MODEL are set (PREGAME_AI_KEY, _PROVIDER, _REASONING).
//   node --max-old-space-size=8000 --experimental-strip-types scripts/pregame-eval.ts [--quiet]
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { heroMatchup, type AiConfig } from '../src/ai-coach.ts';
import { PlayerProfile, clockText } from '../src/player-profile.ts';
import { PregameWatch, buildPregameDigest, buildPregamePrompt, requestPregame, templateBriefing, threatData } from '../src/pregame.ts';
import type { Knowledge } from '../src/types.ts';
import { heroNames, liveFramesOf } from './player-profile-replay.ts';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const framesRoot = [path.join(repo, 'frames'), path.resolve(repo, '..', '..', 'DeadlockCoach', 'frames')].find(p => existsSync(p)) || path.join(repo, 'frames');
const quiet = process.argv.includes('--quiet');
const files = ['rec11', 'rec12', 'rec13'].map(r => path.join(framesRoot, r, 'recording.json'));
if (!files.every(existsSync)) { console.log(`Recordings not found under ${framesRoot}`); process.exit(0); }

const json = (p: string) => JSON.parse(readFileSync(path.join(repo, p), 'utf8').replace(/^\uFEFF/, ''));
const threats = threatData(json('data/coaching/plans.json'), json('data/coaching/review.json'));
const names = heroNames(); const name = (id: string) => names[id] || id.replace(/^hero_/, '').replaceAll('_', ' ');
const docs: Knowledge[] = readdirSync(path.join(repo, 'docs', 'heroes')).filter(n => n.endsWith('.md')).sort().map(file => {
  const body = readFileSync(path.join(repo, 'docs', 'heroes', file), 'utf8').replace(/^\uFEFF/, '');
  return { id: `heroes/${file}`, category: 'heroes', title: body.match(/^#\s+(.+)$/m)?.[1] || file, body };
});

const frames = liveFramesOf(files);
let now = frames[0]?.wall ?? 0;
const profile = new PlayerProfile({ now: () => now }); profile.setHeroNames(names);
const watch = new PregameWatch();
const short = (f: string) => f.replace(/\\/g, '/').split('/').slice(-2, -1)[0] || f;
const first = new Map<string, number>();
const prompts: Array<{ label: string; digest: ReturnType<typeof buildPregameDigest> }> = [];
for (const f of frames) {
  now = f.wall; if (!first.has(f.file)) first.set(f.file, f.wall);
  const ev = profile.observe(f.snapshot, f.wall);
  const at = `${short(f.file)} +${Math.round((f.wall - first.get(f.file)!) / 1000)}s clock ${typeof f.snapshot.game_clock_secs === 'number' ? clockText(f.snapshot.game_clock_secs) : '--'}`;
  if (ev.ended) console.log(`${at}: profile closed a match (${profile.model.heroName(ev.ended.hero)}, ${ev.ended.deaths.length} deaths, ${ev.ended.endedBy ?? ev.endedBy ?? ''})`);
  if (ev.matchStarted) console.log(`${at}: profile match start; focus goals ${profile.goals.map(g => g.id).join(', ') || 'none'}${(f.snapshot.game_clock_secs ?? 999) <= 180 ? ' (the start focus cue is left to the pep talk)' : ''}`);
  const p = watch.frame(f.snapshot, f.wall);
  if (!p) continue;
  if (p.type === 'skipped') { console.log(`${at}: PEP TALK SKIPPED (${p.reason}, match first seen at ${clockText(p.start.clock)})`); continue; }
  console.log(`${at}: PEP TALK DUE (match first seen at ${clockText(p.start.clock)}; hero ${p.roster.hero ?? 'not sure'}; enemies ${p.roster.enemies.map(name).join(', ') || 'not read'}; allies ${p.roster.allies.map(name).join(', ')})`);
  const digest = buildPregameDigest({ key: p.start.key, clock: p.clock, roster: p.roster, previous: profile.matches, goals: profile.goals, threats, name,
    matchup: p.roster.hero ? heroMatchup(docs, p.roster.hero, p.roster.enemies, name) : null });
  const b = templateBriefing(digest);
  console.log(`\n===== ${short(f.file)} RULE-BASED BRIEFING (${b.spoken.split(/\s+/).length} words) =====\n${b.spoken}\nFocus: ${b.focus}\nMatchup: ${b.matchup_tips.join(' | ') || '-'}\nPlan: ${b.plan}\n`);
  prompts.push({ label: short(f.file), digest });
}
profile.endMatch();
for (const { label, digest } of prompts) {
  const prompt = buildPregamePrompt(digest);
  console.log(`${label} AI prompt: ${prompt.chars} chars (~${Math.ceil(prompt.chars / 4)} tokens), max reply tokens ${prompt.maxTokens}`);
  if (!quiet) console.log(`\n===== SYSTEM =====\n${prompt.system}\n\n===== USER =====\n${prompt.user}\n`);
}
const url = process.env.PREGAME_AI_URL, model = process.env.PREGAME_AI_MODEL;
if (url && model) {
  const cfg: AiConfig = { provider: process.env.PREGAME_AI_PROVIDER === 'anthropic' ? 'anthropic' : 'openai', baseUrl: url, model, key: process.env.PREGAME_AI_KEY || '', maxTokens: 1024, headers: '',
    reasoning: (['off', 'low', 'default'] as const).find(r => r === process.env.PREGAME_AI_REASONING) ?? 'low' };
  for (const { label, digest } of prompts) {
    try {
      const r = await requestPregame(cfg, buildPregamePrompt(digest), [...digest.allies, ...digest.enemies, ...(digest.hero ? [digest.hero] : [])], { pageProtocol: 'https:' });
      console.log(`\n===== ${label} AI BRIEFING (${r.model}, ${r.ms} ms, ${r.briefing.spoken.split(/\s+/).length} words) =====\n${JSON.stringify(r.briefing, null, 1)}`);
    } catch (e) { console.log(`${label}: AI failed (${e instanceof Error ? e.message : String(e)}): the app would speak the template.`); }
  }
} else if (!quiet) console.log('(Set PREGAME_AI_URL and PREGAME_AI_MODEL to send the prompt to a gateway.)');
