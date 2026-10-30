// Unit tests for the optional AI coach (imported by tests/unit.test.ts).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { AiCoach, AiHistory, MAX_INPUT_CHARS, SUMMARY_MAX_CHARS, SYSTEM_PROMPT, buildPrompt, buildRequest, buildSummary, callGateway, cleanSpeech,
  endpoint, extractText, heroMatchup, parseAdvice, retrieve, sections, situationTerms, testConnection, upcomingObjectives, setProfileProvider, profileText,
  checkAdvice, coachingAngles, overlap, soulsView, topicOf, TASKS, aiExchanges, clearAiExchanges, onAiExchange,
  configFrom, effectiveMaxTokens, isOpenRouter, liveTriggers, themesIn, usageOf, weaknessOf, reinforceGate, strangerHeroes, damageByHero, isPraise, HERO_NAMES,
  wordBudget, structuresLine, scrubProfile, ruleThemes, ruleCovered, structureProblem, deathCovered,
  deathAfter, DEATH_PRAISE_SECS, DEATH_PANEL_MAX_WAIT_MS, PROFILE_MAX_CHARS, clipAtBoundary, profileForPrompt, profileLabelSkills, setSkillStateProvider, skillQuiet,
  rejuvenatorState, rejuvenatorLine, REJUVENATOR_SECS, beforeDeathLine,
  type Advice, type AbilityRow, type AiConfig, type AiSnapshot, type ItemInfo } from '../src/ai-coach.ts';
import { defaults, normalize } from '../src/settings.ts';
import type { Knowledge, Settings } from '../src/types.ts';

const root = fileURLToPath(new URL('../../', import.meta.url));
function docs(): Knowledge[] {
  const out: Knowledge[] = [];
  for (const folder of ['timings', 'jungle', 'strategy', 'heroes']) for (const name of readdirSync(path.join(root, 'docs', folder)).filter(n => n.endsWith('.md')).sort()) {
    const body = readFileSync(path.join(root, 'docs', folder, name), 'utf8').replace(/^\uFEFF/, '');
    out.push({ id: `${folder}/${name}`, category: folder, title: body.match(/^#\s+(.+)$/m)?.[1] || name, body });
  }
  return out;
}
const lane = (push: number) => ({ push, ally_troopers: 3, enemy_troopers: 2 });
function snap(clock: number, extra: Partial<AiSnapshot> = {}): AiSnapshot {
  return { game_clock_secs: clock, screen: 'Gameplay', hero: 'hero_haze', hp: 600, max_hp: 1000, souls: 3400, items: ['upgrade_headshot_booster'],
    enemy_heroes: ['hero_graves', 'hero_shiv'], ally_networth: 20000, enemy_networth: 18000,
    hero_status: [
      { ally: true, slot: 0, hero: 'hero_haze', health_ratio: .6, networth_observations: 3, observations: 3, position_observations: 0 },
      { ally: false, slot: 0, hero: 'hero_graves', dead: true, respawn_secs: 22, networth_observations: 3, observations: 3, position_observations: 0 },
      { ally: false, slot: 1, hero: 'hero_shiv', bar_visible: false, networth_observations: 3, observations: 3, position_observations: 0 },
    ],
    minimap: { observations: 3, green: lane(.6), blue: lane(.4), yellow: lane(.5), midboss_up: true, urn: null, enemies: [], allies: [], enemy_count: 1,
      you: { x: .5, y: .5 }, your_lane: 'Blue', your_progress: .45, zone: 'Blue jungle',
      heroes: [{ id: 1, ally: false, pos: { x: .52, y: .52 }, unseen_ms: 0, hits: 3 }, { id: 2, ally: false, pos: { x: .2, y: .2 }, unseen_ms: 12000, hits: 3 }] },
    ...extra };
}
const settings = (patch: Partial<Settings> = {}): Settings => normalize({ ...defaults, aiEnabled: true, aiModel: 'test-model', aiKey: 'sk-test-SECRET-123456', ...patch });

test('ai settings are top-level, off by default, normalized and never inside engine options', () => {
  assert.equal(defaults.aiEnabled, false); assert.equal(defaults.aiInterval, 90); assert.equal(defaults.aiReasoning, 'low');
  assert.equal(normalize({ aiInterval: 40 }).aiInterval, 90, 'the old chatty default moves to 90 s'); assert.equal(normalize({ aiInterval: 60 }).aiInterval, 60);
  assert.equal(normalize({ aiReasoning: 'off' }).aiReasoning, 'off'); assert.equal(normalize({ aiReasoning: 'max' }).aiReasoning, 'low');
  const s = normalize({ aiEnabled: true, aiProvider: 'evil', aiInterval: 2, aiMaxTokens: 1e9, aiKey: '  k  ', options: { aiKey: 'x' } });
  assert.equal(s.aiProvider, 'openai'); assert.equal(s.aiInterval, 15); assert.equal(s.aiMaxTokens, 8192); assert.equal(s.aiKey, 'k');
  assert.ok(!('aiKey' in s.options));
});

test('state summary is deterministic, cites history and stays within budget', () => {
  const h = new AiHistory();
  h.push(snap(600, { hp: 1000 }), [], 1000);
  h.push(snap(630, { hp: 900 }), [{ key: 'lane:back_off', text: 'Back off to cover.', priority: 'High', cooldown_secs: 20 }], 31000);
  h.push(snap(660, { hp: 700 }), [], 61000);
  h.push(snap(690), [], 91000);
  const input = { snapshot: snap(690), history: h, now: 91000, trigger: 'periodic' as const, name: (id: string) => id === 'hero_graves' ? 'Graves' : id.replace(/^(hero_|upgrade_)/, '') };
  const a = buildSummary(input), b = buildSummary(input);
  assert.equal(a, b);
  for (const part of ['clock 11:30 (midgame, 8:00 to 20:00)', 'You: haze', 'HP 600/1,000 (60%)', '100→90→70→60', 'Unspent souls 3,400', 'held ≥2,550 for 90s', 'can afford the usual 3,200 buy for 11:30 · Souls advice: allowed',
    'Area: Blue lane, 45% up lane · zone Blue jungle', 'Enemies: 1 alive, dead: Graves (22s), not seen recently: 1', 'Team souls: yours 20,000 vs enemy 18,000 (+2,000)',
    '1 enemy hero visible (1 near you', '1 enemy hero missing (last seen 12s ago)', 'Midboss alive', 'rule (healing): Back off to cover.',
    'Word budget: at most 20 words (midgame) · heard about 6 s after this state', 'Structures: not read: do not name a specific Guardian, Walker or Shrine as the target']) assert.ok(a.includes(part), `${part}\n---\n${a}`);
  assert.ok(a.includes('Upcoming: Guardians reach minimum resist at 12:00 (in 0:30)'), a);
  // A huge state still respects the summary budget and keeps the trigger line.
  const big = snap(690, { items: Array.from({ length: 400 }, (_, i) => `upgrade_item_${i}_${'x'.repeat(30)}`), status_effects: Array.from({ length: 50 }, (_, i) => `effect ${i}`) });
  for (let i = 0; i < 30; i++) h.push(big, [{ key: `k${i}`, text: 'y'.repeat(190), priority: 'Normal', cooldown_secs: 1 }], 91000 + i);
  const s = buildSummary({ ...input, snapshot: big });
  assert.ok(s.length <= SUMMARY_MAX_CHARS, String(s.length)); assert.ok(s.startsWith('Trigger: periodic'));
});

test('death history records the death panel and the souls held before dying', () => {
  const h = new AiHistory();
  h.push(snap(700, { souls: 2400, unsecured_souls: 600, hp: 150 }), [], 0);
  h.push(snap(702, { dead: true, screen: 'Dead', respawn_secs: 18, death_cause: 'Graves', death_damage: [{ source: 'Graves', damage: 620 }, { source: 'Shiv', damage: 300 }], death_killing_blow: 'Graves' }), [], 2000);
  h.push(snap(704, { dead: true, screen: 'Dead', respawn_secs: 16 }), [], 4000);
  assert.equal(h.deathCount, 1);
  const s = buildSummary({ snapshot: snap(704, { dead: true, screen: 'Dead' }), history: h, now: 4000, trigger: 'death' });
  assert.match(s, /YOU ARE DEAD/);
  assert.match(s, /Death at 11:42: cause Graves · damage taken: Graves 620, Shiv 300 · killing blow Graves · last alive HP 15% · allies near you 0, enemies near 1 · unspent before death 2,400 \(unsecured 600\) · death-spend threshold 1,600: reached · respawn 16s/);
});

test('objective timer helper lists only near events in order', () => {
  assert.deepEqual(upcomingObjectives(250, 60), ['medium camps spawn at 5:00 (in 0:50)', 'Powerups spawn at 5:00 (in 0:50)']);
  assert.deepEqual(upcomingObjectives(1300, 30), []);
});

test('knowledge retrieval picks dying/unsecured-souls mistakes on death and jungle tactics in the jungle', () => {
  const all = sections(docs());
  assert.ok(all.length > 50); assert.ok(all.every(s => !/machine-friendly/i.test(s.heading)));
  const h = new AiHistory();
  const dead = retrieve(all, situationTerms(snap(800, { dead: true, screen: 'Dead', minimap: null }), h, 'death'));
  assert.ok(dead.length >= 1 && dead.length <= 3);
  assert.ok(dead.some(s => s.id === 'strategy/common-mistakes.md' && /Dying with haunted/.test(s.heading)), dead.map(d => `${d.id} ${d.heading}`).join('\n'));
  const jungle = retrieve(all, situationTerms(snap(800, { souls: 100 }), h, 'periodic'));
  assert.ok(jungle.some(s => s.id.startsWith('jungle/jungle-tactics')), jungle.map(d => `${d.id} ${d.heading}`).join('\n'));
  const names: Record<string, string> = { hero_haze: 'Haze', hero_graves: 'Graves', hero_shiv: 'Shiv' };
  assert.equal(heroMatchup(docs(), 'hero_haze', ['hero_graves', 'hero_shiv'], id => names[id]), 'Matchup stats for Haze (correlational win rates): weak vs Graves 47.2%; strong vs Shiv 60.2%.');
});

test('prompt stays within ~2,500 tokens and includes the profile, brief and excerpts', () => {
  setProfileProvider(() => 'Dies often with 2k+ unspent souls. Over-extends in Blue lane.');
  assert.equal(profileText(), 'Dies often with 2k+ unspent souls. Over-extends in Blue lane.');
  setProfileProvider(() => { throw new Error('broken'); }); assert.equal(profileText(), null);
  setProfileProvider(undefined);
  const p = buildPrompt({ trigger: 'manual', summary: 'S'.repeat(5000), profile: 'P'.repeat(5000), matchup: 'M', excerpts: Array.from({ length: 6 }, (_, i) => ({ title: `T${i}`, heading: 'H', text: 'E'.repeat(5000) })) });
  assert.ok(p.chars <= MAX_INPUT_CHARS, String(p.chars)); assert.equal(p.tokens, Math.ceil(p.chars / 4)); assert.ok(p.tokens <= 2500);
  assert.equal(p.system, SYSTEM_PROMPT);
  assert.ok(p.user.includes('PLAYER PROFILE') && p.user.includes('REFERENCE EXCERPTS') && !p.user.includes('DEADLOCK FACTS'), 'no generic wiki block any more');
  assert.ok(p.user.startsWith('TASK: The player pressed'));
  const small = buildPrompt({ trigger: 'death', summary: 'Trigger: death' });
  assert.ok(!small.user.includes('PLAYER PROFILE (from')); assert.ok(small.user.includes('just died'));
});

test('request shapes for OpenAI-compatible and Anthropic gateways', () => {
  const base: AiConfig = { provider: 'openai', baseUrl: 'http://localhost:11434/v1/', model: 'llama3.1', key: '', maxTokens: 150, headers: '' };
  const o = buildRequest(base, 'SYS', 'USER');
  assert.equal(o.url, 'http://localhost:11434/v1/chat/completions');
  assert.deepEqual(o.init.headers, { 'content-type': 'application/json' });
  assert.deepEqual(JSON.parse(o.init.body), { model: 'llama3.1', messages: [{ role: 'system', content: 'SYS' }, { role: 'user', content: 'USER' }], max_tokens: 150, temperature: 0.3 });
  const keyed = buildRequest({ ...base, baseUrl: '', key: 'sk-abc', headers: '{"HTTP-Referer":"https://x.test"}' }, 'S', 'U', 99);
  assert.equal(keyed.url, 'https://api.openai.com/v1/chat/completions');
  assert.equal(keyed.init.headers.authorization, 'Bearer sk-abc'); assert.equal(keyed.init.headers['HTTP-Referer'], 'https://x.test');
  assert.equal(JSON.parse(keyed.init.body).max_tokens, 99);
  const a = buildRequest({ ...base, provider: 'anthropic', baseUrl: 'https://api.anthropic.com/v1', model: 'claude', key: 'ak' }, 'SYS', 'USER');
  assert.equal(a.url, 'https://api.anthropic.com/v1/messages');
  assert.equal(endpoint({ ...base, provider: 'anthropic', baseUrl: '' }), 'https://api.anthropic.com/v1/messages');
  assert.deepEqual(a.init.headers, { 'content-type': 'application/json', 'anthropic-version': '2023-06-01', 'anthropic-dangerous-direct-browser-access': 'true', 'x-api-key': 'ak' });
  assert.deepEqual(JSON.parse(a.init.body), { model: 'claude', max_tokens: 150, temperature: 0.3, system: 'SYS', messages: [{ role: 'user', content: 'USER' }] });
  assert.throws(() => buildRequest({ ...base, headers: '[1]' }, 's', 'u'), /JSON object/);
  assert.equal(extractText('openai', { choices: [{ message: { content: 'hi' } }] }), 'hi');
  assert.equal(extractText('anthropic', { content: [{ type: 'text', text: 'a' }, { type: 'tool_use' }, { type: 'text', text: 'b' }] }), 'ab');
  assert.throws(() => extractText('openai', { error: { message: 'bad' } }), /returned an error.*bad/);
});

test('reply parsing handles JSON, fenced JSON, prose around JSON, PASS and garbage', () => {
  const j = parseAdvice('{"speak":"Spend your souls now.","priority":"High","why":"3,400 unspent","focus":"Souls!"}');
  assert.deepEqual(j, { ok: true, advice: { speak: 'Spend your souls now.', priority: 'High', why: '3,400 unspent', focus: 'souls' } });
  const fenced = parseAdvice('Sure!\n```json\n{"speak": "Back off, two enemies are missing.", "priority": "normal", "why": "2 missing", "focus": "map"}\n```');
  assert.ok(fenced.ok && fenced.advice.speak === 'Back off, two enemies are missing.' && fenced.advice.priority === 'Normal');
  const think = parseAdvice('<think>{"speak":"no"}</think> Here: {"speak": "Push the wave {now}.", "priority": "Low", "why": "", "focus": ""}');
  assert.ok(think.ok && think.advice.speak === 'Push the wave {now}.' && think.advice.focus === 'general');
  for (const pass of ['PASS', ' pass. ', '{"speak": null, "priority": "Low", "why": "quiet", "focus": "none"}', '{"speak":"PASS","priority":"Low","why":"","focus":""}']) {
    const r = parseAdvice(pass); assert.ok(r.ok && r.advice.speak === null, pass);
  }
  for (const garbage of ['I think you should farm more.', '', '{not json', '{"advice":"x"}', '{"speak": 5}']) assert.equal(parseAdvice(garbage).ok, false, garbage);
  const long = parseAdvice(JSON.stringify({ speak: `${'word '.repeat(20).trim()}. ${'more '.repeat(20).trim()}. Third sentence.`, priority: 'Normal', why: 'x', focus: 'x' }));
  assert.ok(long.ok && long.advice.speak!.split(' ').length <= 30);
  assert.equal(cleanSpeech('**Buy** Healing Rite. Then return to Blue. Then farm.', 30, 2), 'Buy Healing Rite. Then return to Blue.');
  assert.equal(cleanSpeech('Go '.repeat(40), 5), 'Go Go Go Go Go.');
});

test('gateway failures are classified and never reveal the key', async () => {
  const cfg: AiConfig = { provider: 'openai', baseUrl: 'https://gw.test/v1', model: 'm', key: 'sk-live-SECRET-998877', maxTokens: 50, headers: '' };
  const fail = async () => { throw new TypeError('Failed to fetch'); };
  await assert.rejects(callGateway(cfg, 's', 'u', { fetchImpl: fail as typeof fetch }), (e: Error & { kind?: string }) => e.kind === 'network' && /CORS/.test(e.message));
  const echo = (async () => new Response('{"error":"Incorrect API key provided: sk-live-SECRET-998877"}', { status: 401, statusText: 'Unauthorized' })) as typeof fetch;
  await assert.rejects(callGateway(cfg, 's', 'u', { fetchImpl: echo }), (e: Error & { kind?: string; status?: number }) => e.kind === 'http' && e.status === 401 && /HTTP 401 Unauthorized/.test(e.message) && !e.message.includes('SECRET'));
  await assert.rejects(callGateway({ ...cfg, baseUrl: 'http://gw.test/v1' }, 's', 'u', { pageProtocol: 'https:' }), (e: Error & { kind?: string }) => e.kind === 'config' && /localhost/.test(e.message));
  const t = await testConnection(cfg, { fetchImpl: echo }); assert.equal(t.ok, false); assert.match(t.message, /^Gateway error: HTTP 401/); assert.ok(!t.message.includes('SECRET'));
  const ok = (async () => new Response(JSON.stringify({ model: 'gw-model', choices: [{ message: { content: '{"speak":null,"priority":"Low","why":"connection test","focus":"test"}' } }] }), { status: 200 })) as typeof fetch;
  const t2 = await testConnection(cfg, { fetchImpl: ok }); assert.equal(t2.ok, true); assert.match(t2.message, /^Connected · gw-model · \d+ ms · reply is valid coach JSON$/);
});

test('controller: periodic cadence, one request in flight, stale drop, backoff and one death review per death', async () => {
  let now = 100000, generation = 1, live = true, calls = 0; const bodies: string[] = [];
  let reply: (() => Response) = () => new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":"Buy Extra Health now.","priority":"High","why":"3,400 souls","focus":"souls"}' } }] }));
  let gate: Promise<void> = Promise.resolve();
  const fetchImpl = (async (_url: string, init: RequestInit) => { calls++; bodies.push(String(init.body)); await gate; return reply(); }) as unknown as typeof fetch;
  const delivered: Array<{ key: string; priority: string; live: boolean }> = [];
  let st = settings({ aiInterval: 20 });
  const coach = new AiCoach({ settings: () => st, isLive: () => live, generation: () => generation, knowledge: async () => [], now: () => now, fetchImpl,
    onAdvice: (cue, _a, meta) => delivered.push({ key: cue.key, priority: cue.priority, live: meta.live }) });
  const flush = () => new Promise(r => setTimeout(r, 0));
  coach.observe(snap(600), [], true); assert.equal(calls, 0, 'first observation only starts the clock');
  now += 19000; coach.observe(snap(619), [], true); await flush(); assert.equal(calls, 0);
  now += 2000; coach.observe(snap(621), [{ key: 'lane:back_off', text: 'Back off', priority: 'High', cooldown_secs: 5 }], true); await flush();
  assert.equal(calls, 0, 'a High rule moment suppresses periodic requests');
  now += 11000; coach.observe(snap(632), [], true); await flush(); await flush();
  assert.equal(calls, 1); assert.deepEqual(delivered, [{ key: 'ai:periodic:economy', priority: 'Normal', live: true }]);
  assert.ok(!bodies[0].includes('SECRET'), 'key travels only in headers');
  // Stale answers (older than 15 s) are dropped for live triggers.
  let release!: () => void; gate = new Promise(r => { release = r; });
  now += 21000; coach.observe(snap(653), [], true); await flush(); assert.equal(calls, 2);
  now += 21000; coach.observe(snap(674), [], true); await flush(); assert.equal(calls, 2, 'never more than one request in flight');
  now += 16000 - 21000; release(); await flush(); await flush();
  assert.equal(delivered.length, 1); assert.equal(coach.status.stale, 1);
  // Errors back off exponentially.
  gate = Promise.resolve(); reply = () => new Response('upstream down', { status: 503 });
  now += 30000; coach.observe(snap(700), [], true); await flush(); await flush();
  assert.equal(coach.status.state, 'error'); assert.match(coach.status.error!, /HTTP 503/); assert.match(coach.diagnostic(), /backing off 5 s/);
  now += 21000 - 1; const before = calls; now += 1; coach.observe(snap(721), [], true); await flush(); await flush(); assert.equal(calls, before + 1);
  assert.match(coach.diagnostic(), /backing off 10 s/);
  // Death: one deeper review per death, after the death panel is read.
  reply = () => new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":"Graves caught you in melee range. Next life hold max range behind your front line.","priority":"Normal","why":"death panel","focus":"death"}' } }] }));
  now += 60000; coach.observe(snap(780, { dead: true, screen: 'Dead' }), [], true); await flush(); const atDeath = calls;
  assert.equal(atDeath, before + 1, 'waits for the death panel');
  now += 500; coach.observe(snap(781, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [], true); await flush();
  assert.equal(calls, atDeath, 'waits for the rule death review or 4 s after the panel');
  now += 500; coach.observe(snap(782, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [{ key: 'death:review:780', text: 'Why you died: Graves did 620 damage at close range.', priority: 'Normal', cooldown_secs: 0 }], true); await flush(); await flush();
  assert.equal(calls, atDeath + 1); assert.equal(delivered.at(-1)?.key, 'ai:death:positioning');
  assert.ok(bodies.at(-1)!.includes('Rule death review of this death (already spoken): Why you died: Graves did 620'));
  assert.ok(bodies.at(-1)!.includes('just died') && bodies.at(-1)!.includes('cause Graves'));
  now += 3000; coach.observe(snap(784, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [], true); await flush(); assert.equal(calls, atDeath + 1);
  // Not live (replay/paused): no automatic requests; generation change drops a manual answer.
  live = false; now += 60000; coach.observe(snap(900), [], false); await flush(); assert.equal(calls, atDeath + 1);
  gate = new Promise(r => { release = r; }); void coach.ask('manual'); await flush(); generation++; release(); await flush(); await flush();
  assert.equal(delivered.at(-1)?.key, 'ai:death:positioning');
  // Disabled: nothing is sent.
  st = settings({ aiEnabled: false }); gate = Promise.resolve(); await coach.ask('manual'); assert.equal(coach.status.state, 'off');
});

// ---------------------------------------------------------------------------------------
// Souls tiers, repeat gate, death review and coaching angles (rec8 regressions)
// ---------------------------------------------------------------------------------------
const unspent = (text = 'You can afford a 6,400-soul item: you have 7,443 souls.') => ({ key: 'unspent', text, priority: 'Normal' as const, cooldown_secs: 150 });
function summaryAt(clock: number, souls: number, cues: ReturnType<typeof unspent>[] = []) {
  const h = new AiHistory();
  h.push(snap(clock - 60, { souls }), cues, 0); h.push(snap(clock, { souls }), [], 60000);
  return buildSummary({ snapshot: snap(clock, { souls }), history: h, now: 60000, trigger: 'periodic' });
}

test('souls are worded against the usual buy for the game time (economy.rs spend_target), never as "can afford a 1,600 item" late', () => {
  const late = summaryAt(2049, 2834);
  assert.ok(late.includes('Souls 2,834 (saving: the usual buy now is a 6,400 item; 3,566 short) · Souls advice: not allowed (saving for the usual 6,400 buy)'), late);
  assert.ok(!/can afford|1,600 item/.test(late), late);
  assert.ok(summaryAt(2426, 7443).includes('Unspent souls 7,443 · held ≥5,582 for 60s · can afford the usual 6,400 buy for 40:26 · Souls advice: allowed'));
  assert.ok(summaryAt(2426, 7443, [unspent()]).includes('can afford the usual 6,400 buy for 40:26 · Souls advice: not allowed (the rule coach already said it at 39:26)'));
  assert.ok(summaryAt(540, 1700).includes('can afford the usual 1,600 buy for 9:00 · Souls advice: allowed'));
  assert.ok(summaryAt(900, 2000).includes('Souls 2,000 (saving: the usual buy now is a 3,200 item; 1,200 short)'));
  // The rule reminder closes the gate for 3 minutes only.
  const h = new AiHistory(); h.push(snap(2200, { souls: 7443 }), [unspent()], 0); h.push(snap(2390, { souls: 7443 }), [], 190000);
  assert.equal(soulsView(snap(2390, { souls: 7443 }), h, 190000).allowed, true);
});

test('repeat gate: same focus within 3 min, near-duplicate text and rule repeats are not spoken; manual and urgent calls pass', () => {
  const a = (speak: string, focus: string, priority: Advice['priority'] = 'Normal'): Advice => ({ speak, focus, priority, why: '' });
  const h = new AiHistory();
  h.push(snap(2000, { souls: 7000 }), [], 0);
  h.noteAdvice({ t: 0, clock: 2000, trigger: 'periodic', topic: 'economy', text: 'Spend your 7,000 souls now at the nearest shop on a 6,400 item.' });
  h.push(snap(2100, { souls: 7200 }), [{ key: 'lane:back_off', text: 'Health looks low or falling. Back behind cover before trading.', priority: 'Normal', cooldown_secs: 20 }], 100000);
  const v = soulsView(snap(2100, { souls: 7200 }), h, 100000); assert.equal(v.allowed, true);
  const repeat = a('Spend your 7,200 souls at the Blue shop and get a 6,400 item.', 'items');
  const r1 = checkAdvice(repeat, 'periodic', v, h, 100000);
  assert.ok(!r1.ok && /near-duplicate of the AI call at 33:20|same theme \(economy\) as the AI call at 33:20/.test(r1.reason), JSON.stringify(r1));
  assert.ok(checkAdvice(repeat, 'manual', v, h, 100000).ok, 'the player asked: no gating');
  const r2 = checkAdvice(a('Yamato is alive: hold max range behind Abrams.', 'positioning'), 'periodic', v, h, 100000);
  assert.ok(r2.ok && r2.topic === 'positioning' && r2.speak === 'Yamato is alive: hold max range behind Abrams.');
  const r3 = checkAdvice(a('Your health is low: back behind cover before trading.', 'health'), 'periodic', v, h, 100000);
  assert.ok(!r3.ok && /repeats the rule call/.test(r3.reason), JSON.stringify(r3));
  // After the 3-minute focus window a near-identical text is still a repeat (5-minute window).
  h.push(snap(2200, { souls: 7200 }), [], 200000);
  const r4 = checkAdvice(a('Spend your 7,200 souls now at the nearest shop on a 6,400 item.', 'general'), 'periodic', soulsView(snap(2200, { souls: 7200 }), h, 200000), h, 200000);
  assert.ok(!r4.ok && /near-duplicate of the AI call at 33:20/.test(r4.reason), JSON.stringify(r4));
  // Same theme again within 5 min only with a NEW trigger (High priority alone is not new).
  h.noteAdvice({ t: 200000, clock: 2200, trigger: 'periodic', topic: 'positioning', text: 'Hold max range behind Abrams while Yamato is up.' });
  h.push(snap(2230), [], 230000);
  const v2 = soulsView(snap(2230), h, 230000);
  assert.equal(checkAdvice(a('Shiv is diving you now: dash back to Bebop.', 'range'), 'periodic', v2, h, 230000).ok, false);
  assert.equal(checkAdvice(a('Shiv is diving you now: dash back to Bebop.', 'range', 'High'), 'periodic', v2, h, 230000).ok, false, 'High is not a new trigger');
  // The enemy leaves and comes back near you: a new episode of the diver trigger.
  const far = snap(2240); far.minimap = { ...far.minimap!, heroes: [{ id: 1, ally: false, pos: { x: .9, y: .9 }, unseen_ms: 0, hits: 3 }] };
  h.push(far, [], 240000); h.push(snap(2250), [], 250000);
  assert.equal(checkAdvice(a('Shiv is diving you now: dash back to Bebop.', 'range', 'High'), 'periodic', soulsView(snap(2250), h, 250000), h, 250000).ok, true);
  // Topics and overlap (the rec8 calls tagged "souls" and "items" are one topic).
  assert.equal(topicOf('items', "Spend your 2,834 souls now at the nearest shop on a 1,600 item - you're holding too long."), 'economy');
  assert.equal(topicOf('objective', 'Escort the Soul Urn to the delivery point.'), 'objective');
  assert.ok(overlap("Spend your 2,834 souls now at the nearest shop on a 1,600 item - you're holding too long and risk losing them. Buy before pushing the next wave.",
    'Spend your 2,026 souls at the Blue shop after this wave - get a 1,600 item to stay ahead.') >= .6);
  assert.ok(overlap('Push the Green wave into the enemy guardian, then grab the easy camp.', 'Walk out with Bebop and hold max range behind him.') < .2);
});

test('death review: no souls talk below the death-spend threshold; the prompt carries damage, killing blow, CC and the rule review', () => {
  const h = new AiHistory();
  h.push(snap(1670, { souls: 2394, hp: 1800, max_hp: 1890, status_effects: ['stunned'] }), [], 0);
  h.push(snap(1674, { souls: 2394, hp: 599, max_hp: 1890 }), [], 4000);
  const dead = snap(1677, { dead: true, screen: 'Dead', hero: null, hp: null, max_hp: null, souls: 2409, respawn_secs: 56, death_cause: 'Melee',
    death_damage: [{ source: 'Melee', damage: 525 }, { source: 'Flying Slash', damage: 501 }], death_killing_blow: 'Melee' });
  h.push(dead, [{ key: 'death:review:1677', text: 'Why you died: Yamato did 35% of the damage (622) with Flying Slash and Power Slash.', priority: 'Normal', cooldown_secs: 0 }], 7000);
  const summary = buildSummary({ snapshot: dead, history: h, now: 7000, trigger: 'death' });
  for (const part of ['YOU ARE DEAD', 'You: haze', 'cause Melee · damage taken: Melee 525, Flying Slash 501 · killing blow Melee · status effects before death: stunned · last alive HP 32%',
    'death-spend threshold 3,200: not reached', 'Souls advice: not allowed (2,409 is below the 3,200 death-spend threshold: not a souls death)',
    'Rule death review of this death (already spoken): Why you died: Yamato did 35%']) assert.ok(summary.includes(part), `${part}\n---\n${summary}`);
  const p = buildPrompt({ trigger: 'death', summary });
  assert.ok(p.user.startsWith(TASKS.death)); assert.match(p.user, /Explain WHY/); assert.match(p.system, /Souls advice: allowed/);
  assert.ok(!situationTerms(dead, h, 'death').includes('haunted'), 'no unsecured-souls excerpts for a death below the threshold');
  const v = soulsView(dead, h, 7000); assert.equal(v.allowed, false);
  const mixed = checkAdvice({ speak: 'Yamato dove you at close range again. Buy a 1,600 item now. Next life hold max range behind your front line.', priority: 'Normal', why: '', focus: 'items' }, 'death', v, h, 7000);
  assert.ok(mixed.ok && mixed.speak === 'Yamato dove you at close range again. Next life hold max range behind your front line.' && mixed.topic === 'positioning', JSON.stringify(mixed));
  assert.equal(checkAdvice({ speak: 'Spend your 2,409 souls on the death screen.', priority: 'Normal', why: '', focus: 'souls' }, 'death', v, h, 7000).ok, false);
  // At or above the threshold souls may be named, but not a tier below half the usual buy.
  const rich = new AiHistory(); rich.push(snap(2980, { souls: 4474 }), [], 0); const late = snap(2982, { dead: true, screen: 'Dead', souls: 4474 }); rich.push(late, [], 2000);
  const rv = soulsView(late, rich, 2000); assert.equal(rv.allowed, true);
  assert.ok(buildSummary({ snapshot: late, history: rich, now: 2000, trigger: 'death' }).includes('Souls 4,474 on the death screen (≥ the 3,200 death-spend threshold: buy before respawning) · Souls advice: allowed'));
  const tier = checkAdvice({ speak: 'Bullet Dance shredded you. Spend 4,474 souls on a 1,600 item now. Buy a 3,200 item for bullet resist.', priority: 'Normal', why: '', focus: 'death' }, 'death', rv, rich, 2000);
  assert.ok(tier.ok && tier.speak === 'Bullet Dance shredded you. Buy a 3,200 item for bullet resist.', JSON.stringify(tier));
});

test('coaching angles tie the live situation to profile weaknesses; the death-screen shop is not a second death', () => {
  const profile = 'Player profile (3 matches, mostly Haze). Weak: Killed at close range: 6 of 7 deaths came from melee range (melee hits x3); Dies soon after respawning: 8 of 12 deaths within 90 s of respawning. Strong: Keeps lanes covered. Focus: keep distance from divers.';
  const h = new AiHistory();
  h.push(snap(990), [], 0);
  h.push(snap(1000, { dead: true, screen: 'Dead', death_cause: 'Melee' }), [], 10000);
  h.push(snap(1020, { screen: 'Shop', hp: null, max_hp: null }), [], 30000);
  assert.equal(h.samples.at(-1)!.dead, true, 'the shop opened from the death screen is still dead');
  h.push(snap(1040, { dead: true, screen: 'Dead' }), [], 50000); assert.equal(h.deathCount, 1);
  h.push(snap(1060), [], 70000);
  const status = [
    { ally: true, slot: 0, hero: 'hero_haze', health_ratio: 1, networth_observations: 3, observations: 3, position_observations: 0 },
    { ally: false, slot: 0, hero: 'hero_graves', dead: true, respawn_secs: 22, networth_observations: 3, observations: 3, position_observations: 0 },
    { ally: false, slot: 1, hero: 'hero_shiv', dead: true, respawn_secs: 30, networth_observations: 3, observations: 3, position_observations: 0 },
    { ally: false, slot: 2, hero: 'hero_yamato', networth_observations: 3, observations: 3, position_observations: 0 },
  ];
  const s = snap(1070, { hp: 1000, hero_status: status, enemy_heroes: ['hero_graves', 'hero_shiv', 'hero_yamato'] });
  h.push(s, [], 80000);
  const angles = coachingAngles({ snapshot: s, history: h, now: 80000, profile });
  assert.equal(angles.length, 3, angles.join('\n'));
  assert.match(angles[0], /^- Respawned 10s ago, 0 allies near you → walk out with a teammate .*\[profile: Dies soon after respawning\]$/);
  assert.match(angles[1], /^- Close-range divers alive: yamato \(1 enemy near you\); your last death was at close range → hold max range behind your front line .*\[profile: Killed at close range\]$/);
  // No structure state read: the angle never names a Guardian or Walker as the target.
  assert.match(angles[2], /^- 2 enemies dead \(graves, shiv, first back in 22s\) → group up and push Green lane \(front 60% up\) to its next standing structure/);
  const summary = buildSummary({ snapshot: s, history: h, now: 80000, trigger: 'periodic', profile });
  assert.ok(summary.includes('Respawned 10s ago (last death at 16:40, cause Melee)') && summary.includes('COACHING ANGLES'), summary);
  assert.deepEqual(coachingAngles({ snapshot: snap(1080, { dead: true, screen: 'Dead' }), history: h, now: 90000, profile }), []);
});

test('controller: a model that always says "spend your souls" is gated, gets one cheap retry, and is never spoken twice in a row', async () => {
  let now = 100000; const bodies: string[] = []; const delivered: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(String(init.body));
    return new Response(JSON.stringify({ choices: [{ message: { content: JSON.stringify({ speak: 'Spend your souls now, you are holding too long.', priority: 'Normal', why: 'souls', focus: bodies.length % 2 ? 'souls' : 'items' }) } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings({ aiInterval: 20 }), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, onAdvice: c => delivered.push(c.text) });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  coach.observe(snap(2040, { souls: 2834 }), [], true);
  now += 21000; coach.observe(snap(2061, { souls: 2834 }), [], true); await flush();
  assert.equal(bodies.length, 2, 'one cheap retry');
  assert.match(JSON.parse(bodies[1]).messages[1].content, /YOUR PREVIOUS ANSWER WAS NOT USED \(souls advice blocked: saving for the usual 6,400 buy\)/);
  assert.equal(delivered.length, 0); assert.equal(coach.status.suppressed, 2); assert.match(coach.status.message, /^Skipped a repeat: souls advice blocked/);
  now += 21000; coach.observe(snap(2400, { souls: 7443 }), [], true); await flush();
  assert.deepEqual(delivered, ['Spend your souls now, you are holding too long.']);
  now += 21000; coach.observe(snap(2421, { souls: 7443 }), [], true); await flush();
  assert.equal(delivered.length, 1, 'same focus within 3 minutes is not spoken');
  assert.match(JSON.parse(bodies.at(-1)!).messages[1].content, /RECENT AI ADVICE \(yours\):\n- \[40:00\] economy: Spend your souls now[\s\S]*THEMES COACHED[^\n]*\n- theme economy: 40:00 \(blocked\)/);
  assert.match(coach.diagnostic(), /4 repeats skipped/);
});
test('AI replies in the shapes real gateways send are understood, and failures say why', () => {
  const j = '{"speak":"Back off.","priority":"High","why":"w","focus":"health"}';
  assert.equal(extractText('openai', { choices: [{ message: { content: [{ type: 'text', text: j }] } }] }), j);
  assert.equal(extractText('openai', { choices: [{ message: { content: [{ type: 'output_text', text: j }] } }] }), j);
  assert.equal(extractText('openai', { choices: [{ text: j }] }), j, 'legacy completions');
  assert.equal(extractText('openai', { output_text: j }), j, 'Responses API');
  assert.equal(extractText('openai', { output: [{ type: 'message', content: [{ type: 'output_text', text: j }] }] }), j);
  assert.equal(extractText('openai', { message: { role: 'assistant', content: j } }), j, 'Ollama /api/chat');
  assert.equal(extractText('openai', { content: [{ type: 'text', text: j }] }), j, 'Anthropic shape behind an OpenAI URL');
  assert.equal(extractText('openai', { choices: [{ message: { content: null, reasoning_content: `thinking... ${j}` }, finish_reason: 'stop' }] }).includes('"speak"'), true);
  assert.throws(() => extractText('openai', { choices: [{ message: { content: null, reasoning_content: 'long thoughts' }, finish_reason: 'length' }] }), /max tokens on reasoning.*Raise "Max tokens"/);
  assert.throws(() => extractText('openai', { choices: [{ message: { content: '' }, finish_reason: 'length' }] }), /finish_reason "length"/);
  assert.throws(() => extractText('openai', { choices: [{ message: { content: null, refusal: 'no' } }] }), /refused/);
  assert.throws(() => extractText('openai', { id: 'x', object: 'list', data: [] }), /no "choices" array.*Reply starts: \{"id":"x"/);
  assert.throws(() => extractText('anthropic', { content: [], stop_reason: 'max_tokens' }), /max_tokens/);
});

test('every gateway call is logged with the whole reply, keys redacted', async () => {
  clearAiExchanges();
  const cfg: AiConfig = { provider: 'openai', baseUrl: 'https://gw.example/v1', model: 'm', key: 'sk-secret-123456', headers: '', maxTokens: 100 };
  const seen: unknown[] = []; const off = onAiExchange(x => seen.push(x));
  const reply = { id: 'r1', choices: [{ message: { content: null }, finish_reason: 'length' }], echo: 'sk-secret-123456' };
  const fake = (async () => new Response(JSON.stringify(reply), { status: 200 })) as typeof fetch;
  await assert.rejects(callGateway(cfg, 'system', 'STATE: souls 100', { fetchImpl: fake, trigger: 'periodic', clock: 600 }), /finish_reason "length"/);
  const [x] = aiExchanges();
  assert.equal(x.trigger, 'periodic'); assert.equal(x.clock, 600); assert.equal(x.status, 200); assert.equal(x.prompt, 'STATE: souls 100');
  assert.match(x.reply, /"id":"r1"/); assert.ok(!x.reply.includes('sk-secret-123456'), 'key redacted'); assert.match(x.error!, /length/);
  const ok = (async () => new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":null}' } }] }), { status: 200 })) as typeof fetch;
  assert.equal((await callGateway(cfg, 's', 'u', { fetchImpl: ok })).text, '{"speak":null}');
  assert.equal(aiExchanges().length, 2); assert.equal(aiExchanges()[1].error, null); assert.equal(seen.length, 2);
  off(); clearAiExchanges();
});
test('max reply tokens default to 1,024, an old 200 is raised, and a cut-off reply is retried with double the limit', async () => {
  assert.equal(defaults.aiMaxTokens, 1024);
  assert.equal(normalize({ ...defaults, aiMaxTokens: 200 } as Settings).aiMaxTokens, 1024);
  assert.equal(normalize({ ...defaults, aiMaxTokens: 2048 } as Settings).aiMaxTokens, 2048);
  const cfg: AiConfig = { provider: 'openai', baseUrl: 'https://openrouter.ai/api/v1', model: 'anthropic/claude-haiku-4.5', key: 'k', headers: '', maxTokens: 1024 };
  const cut = (async () => new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":"Hold max range behind your' }, finish_reason: 'length' }] }), { status: 200 })) as typeof fetch;
  await assert.rejects(callGateway(cfg, 's', 'u', { fetchImpl: cut }), (e: Error & { kind?: string }) => e.kind === 'truncated' && /3072-token limit/.test(e.message), 'reply budget 1,024 + 2,048 reasoning headroom for an OpenRouter model at its default');
  const sent: number[] = [];
  const later = (async (_u: string, init: RequestInit) => { const body = JSON.parse(String(init.body)); sent.push(body.max_tokens);
    return new Response(JSON.stringify(sent.length === 1 ? { choices: [{ message: { content: '{"speak":"Hold' }, finish_reason: 'length' }] }
      : { choices: [{ message: { content: '{"speak":"Hold max range behind your front line.","priority":"Normal","why":"melee diver near","focus":"positioning"}' }, finish_reason: 'stop' }] }), { status: 200 }); }) as unknown as typeof fetch;
  const settings = normalize({ ...defaults, aiEnabled: true, aiBaseUrl: cfg.baseUrl, aiModel: cfg.model, aiKey: 'k' } as Settings);
  const spoken: string[] = [];
  const coach = new AiCoach({ settings: () => settings, isLive: () => true, generation: () => 1, knowledge: async () => [], name: (x: string) => x,
    fetchImpl: later, pageProtocol: () => 'https:', onAdvice: (c: { text: string }) => spoken.push(c.text) } as never);
  await coach.ask('manual');
  assert.deepEqual(sent, [], 'no match state read yet: the manual request is not sent');
  assert.match(coach.status.message, /No match state read yet/);
  coach.observe(snap(600), [], false);
  await coach.ask('manual');
  assert.deepEqual(sent, [2048, 3072], 'reasoning Low on OpenRouter adds 1,024 tokens of headroom'); assert.equal(spoken[0], 'Hold max range behind your front line.');
});

// ---------------------------------------------------------------------------------------
// rec11 regressions: reasoning control, theme memory, adaptive cadence, summary wording
// ---------------------------------------------------------------------------------------
test('model reasoning: OpenRouter reasoning object, OpenAI reasoning_effort, nothing for Anthropic or local gateways', () => {
  const body = (cfg: Partial<AiConfig>) => JSON.parse(buildRequest({ provider: 'openai', baseUrl: 'https://openrouter.ai/api/v1', model: 'anthropic/claude-haiku-5.5', key: 'k', headers: '', maxTokens: 1024, ...cfg }, 'S', 'U').init.body);
  const low = body({ reasoning: 'low' });
  assert.deepEqual(low.reasoning, { effort: 'low' }); assert.equal(low.max_tokens, 2048, 'Anthropic via OpenRouter thinks 1,024 tokens at low; max_tokens must stay above it'); assert.equal(low.temperature, 0.3);
  assert.deepEqual(body({ reasoning: 'off' }).reasoning, { enabled: false }); assert.equal(body({ reasoning: 'off' }).max_tokens, 1024);
  assert.ok(!('reasoning' in body({ reasoning: 'default' }))); assert.equal(body({ reasoning: 'default' }).max_tokens, 3072);
  assert.deepEqual(body({ model: 'openrouter/free', reasoning: 'low' }).reasoning, { effort: 'low' }, 'the free router passes it on');
  // OpenAI reasoning models on the OpenAI API.
  const gpt5 = body({ baseUrl: 'https://api.openai.com/v1', model: 'gpt-5-mini', reasoning: 'off' });
  assert.equal(gpt5.reasoning_effort, 'minimal'); assert.equal(gpt5.max_completion_tokens, 1280); assert.ok(!('max_tokens' in gpt5) && !('temperature' in gpt5) && !('reasoning' in gpt5));
  assert.equal(body({ baseUrl: 'https://api.openai.com/v1', model: 'o4-mini', reasoning: 'low' }).reasoning_effort, 'low');
  assert.equal(body({ baseUrl: 'https://api.openai.com/v1', model: 'o3', reasoning: 'off' }).reasoning_effort, 'low', 'o-series has no minimal');
  const plain = body({ baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini', reasoning: 'low' });
  assert.ok(!('reasoning' in plain) && !('reasoning_effort' in plain)); assert.equal(plain.max_tokens, 1024);
  assert.deepEqual(body({ baseUrl: 'http://localhost:11434/v1', model: 'llama3.1', reasoning: 'low' }), { model: 'llama3.1', messages: [{ role: 'system', content: 'S' }, { role: 'user', content: 'U' }], max_tokens: 1024, temperature: 0.3 });
  const anth = body({ provider: 'anthropic', baseUrl: 'https://api.anthropic.com', model: 'claude-haiku-5-5', reasoning: 'default' });
  assert.ok(!('thinking' in anth) && !('reasoning' in anth)); assert.equal(anth.max_tokens, 1024);
  assert.equal(configFrom(settings({ aiReasoning: 'off' })).reasoning, 'off');
  assert.ok(isOpenRouter({ baseUrl: 'https://openrouter.ai/api/v1' }) && !isOpenRouter({ baseUrl: 'https://my-openrouter.ai.example.com' }));
  assert.equal(effectiveMaxTokens({ provider: 'openai', baseUrl: 'https://openrouter.ai/api/v1', model: 'x', key: '', headers: '', maxTokens: 1024, reasoning: 'low' }), 2048);
  assert.deepEqual(usageOf({ usage: { prompt_tokens: 3400, completion_tokens: 900, completion_tokens_details: { reasoning_tokens: 840 } } }), { prompt: 3400, completion: 900, reasoning: 840 });
  assert.deepEqual(usageOf({ usage: { input_tokens: 10, output_tokens: 5 } }), { prompt: 10, completion: 5, reasoning: null });
});

test('reasoning tokens reach the log and diagnostics; reasoning that eats the budget is retried; a rejected reasoning field is dropped once', async () => {
  clearAiExchanges();
  const replies: unknown[] = [
    { model: 'nvidia/nemotron', choices: [{ message: { content: '', reasoning: 'thinking '.repeat(50) }, finish_reason: 'length' }], usage: { prompt_tokens: 3500, completion_tokens: 2048, completion_tokens_details: { reasoning_tokens: 2040 } } },
    { model: 'nvidia/nemotron', choices: [{ message: { content: '{"speak":null,"priority":"Low","why":"quiet","focus":"map"}' }, finish_reason: 'stop' }], usage: { prompt_tokens: 3500, completion_tokens: 300, completion_tokens_details: { reasoning_tokens: 260 } } },
  ];
  const sent: Array<Record<string, unknown>> = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { sent.push(JSON.parse(String(init.body))); return new Response(JSON.stringify(replies.shift()), { status: 200 }); }) as unknown as typeof fetch;
  const st = settings({ aiBaseUrl: 'https://openrouter.ai/api/v1', aiModel: 'openrouter/free' });
  const coach = new AiCoach({ settings: () => st, isLive: () => true, generation: () => 1, knowledge: async () => [], fetchImpl, onAdvice: () => {} });
  coach.observe(snap(600), [], false);
  await coach.ask('manual');
  assert.deepEqual(sent.map(b => b.max_tokens), [2048, 3072], 'cut off by reasoning: one retry with double the reply budget');
  assert.equal(coach.status.reasoningLast, 260);
  const log = aiExchanges(); assert.match(log[0].error!, /spent 2040 of 2048 tokens on reasoning/); assert.equal(log[1].usage?.reasoning, 260); assert.equal(log[1].reasoning, 'OpenRouter reasoning effort low');
  assert.match(coach.diagnostic(), /reasoning low \(OpenRouter reasoning effort low, max_tokens 2048\)/); assert.match(coach.diagnostic(), /reasoning tokens last 260, avg 260/);
  // A model that rejects the reasoning field: sent again without it.
  const bodies: Array<Record<string, unknown>> = [];
  const strict = (async (_u: string, init: RequestInit) => { const b = JSON.parse(String(init.body)); bodies.push(b);
    return 'reasoning' in b ? new Response('{"error":{"message":"Reasoning is mandatory for this endpoint and cannot be disabled."}}', { status: 400 })
      : new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":null}' } }] }), { status: 200 }); }) as unknown as typeof fetch;
  const r = await callGateway(configFrom(settings({ aiBaseUrl: 'https://openrouter.ai/api/v1', aiReasoning: 'off' })), 's', 'u', { fetchImpl: strict });
  assert.equal(r.text, '{"speak":null}'); assert.equal(bodies.length, 2); assert.ok(!('reasoning' in bodies[1]));
  clearAiExchanges();
});

test('theme memory: a weakness restated within 5 min is dropped or gated unless a new trigger happened, and the prompt lists it', () => {
  const profile = 'Player profile (6 matches, mostly Grey Talon, 70% dead). Weak: Killed at close range: 5 of 6 deaths came from melee range; Dies soon after respawning: 8 of 12. Strong: Uses enemy-dead windows.';
  const a = (speak: string, focus: string, priority: Advice['priority'] = 'Normal'): Advice => ({ speak, focus, priority, why: '' });
  const h = new AiHistory();
  const noDiver = (clock: number) => { const s = snap(clock, { souls: 100 }); s.minimap = { ...s.minimap!, heroes: [{ id: 1, ally: false, pos: { x: .9, y: .9 }, unseen_ms: 0, hits: 3 }] }; return s; };
  h.push(noDiver(1000), [], 0);
  h.noteAdvice({ t: 0, clock: 1000, trigger: 'periodic', topic: 'positioning', text: 'Calico is alive, so hold max range behind your front line and save your escape for their dive.', weakness: 'Killed at close range' });
  assert.deepEqual(h.advice[0].concepts, ['close']); assert.ok(h.advice[0].themes!.includes('positioning'));
  h.push(noDiver(1060), [], 60000);
  const v = soulsView(noDiver(1060), h, 60000);
  // The rec11 pattern: a team call with the stock range line appended → the range sentence is dropped.
  const mixed = checkAdvice(a('Silver and Apollo are down, so regroup with your four allies near the Yellow lane Guardian. Stay at max range behind your front line.', 'team'), 'periodic', v, h, 60000);
  assert.ok(mixed.ok && mixed.speak === 'Silver and Apollo are down, so regroup with your four allies near the Yellow lane Guardian.' && /range against divers/.test(mixed.note!), JSON.stringify(mixed));
  // Only the restated weakness → not spoken.
  const only = checkAdvice(a('Graves is alive too, so keep max range and do not step into melee.', 'positioning', 'High'), 'periodic', v, h, 60000);
  assert.ok(!only.ok && /"range against divers" was coached at 16:40 and nothing new triggered it/.test(only.reason), JSON.stringify(only));
  // A sentence left without its context is not spoken alone.
  assert.equal(checkAdvice(a('Calico dives melee. Take it together once your group is back.', 'objective'), 'periodic', v, h, 60000).ok, false);
  // The prompt shows the memory.
  const summary = buildSummary({ snapshot: noDiver(1060), history: h, now: 60000, trigger: 'periodic', profile, reason: 'enemy-dead window: 2 enemies dead' });
  assert.match(summary, /WHY NOW: enemy-dead window: 2 enemies dead/);
  assert.match(summary, /THEMES COACHED[^\n]*\n- theme positioning: 16:40 \(blocked\)\n- weakness "range against divers" \[Killed at close range\]: 16:40 \(blocked\)/);
  // An enemy comes within reach (new diver-near episode): the range call is allowed again.
  h.push(snap(1080, { souls: 100 }), [], 80000);
  assert.ok(h.active.includes('diver-near'));
  assert.ok(checkAdvice(a('Shiv is on you now: dash back to your team and fight at max range.', 'positioning'), 'periodic', soulsView(snap(1080, { souls: 100 }), h, 80000), h, 80000).ok);
  assert.match(buildSummary({ snapshot: snap(1080, { souls: 100 }), history: h, now: 80000, trigger: 'periodic', profile }), /theme positioning: 16:40 \(NEW trigger now: (diver|enemy)-near\)/);
  // After 5 minutes the theme is free again even without a trigger.
  h.push(noDiver(1310), [], 310000);
  assert.ok(checkAdvice(a('Calico is alive: hold max range behind your front line.', 'positioning'), 'periodic', soulsView(noDiver(1310), h, 310000), h, 310000).ok);
  // The rule coach's "Profile focus" line counts as coached; death reviews and manual asks are not theme-gated.
  const r = new AiHistory(); r.push(noDiver(500), [{ key: 'player:focus:dying', text: 'Profile focus: you spend 65% of the game dead. Stay at max range behind your front line.', priority: 'Normal', cooldown_secs: 300 }], 0);
  r.push(noDiver(560), [], 60000);
  const rv = soulsView(noDiver(560), r, 60000);
  assert.ok(!checkAdvice(a('Graves is alive, keep max range from melee and do not step forward.', 'positioning'), 'periodic', rv, r, 60000).ok);
  assert.ok(checkAdvice(a('Graves is alive, keep max range from melee and do not step forward.', 'positioning'), 'death', rv, r, 60000).ok);
  assert.ok(checkAdvice(a('Graves is alive, keep max range from melee and do not step forward.', 'positioning'), 'manual', rv, r, 60000).ok);
  // Themes from text when the tag is not a theme.
  assert.equal(topicOf('death', 'Fall back to your Guardian and heal to full.'), 'healing');
  assert.deepEqual(themesIn('Spend your 3,400 souls, then push Green lane with Silver.').slice(0, 2), ['economy', 'wave']);
  assert.equal(weaknessOf('Hold max range behind your front line.', profile), 'Killed at close range');
});

test('adaptive cadence: events ask sooner (respawn, enemy-dead window, objective, phase, souls, team souls swing), a quiet game waits the 90 s interval', async () => {
  let now = 100000; const bodies: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(JSON.parse(String(init.body)).messages[1].content);
    return new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":null,"priority":"Low","why":"quiet","focus":"map"}' } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings(), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, onAdvice: () => {} });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  const quiet = (clock: number, extra: Partial<AiSnapshot> = {}) => { const s = snap(clock, { souls: 500, hero_status: [], ...extra }); s.minimap = { ...s.minimap!, heroes: [] }; return s; };
  const step = async (secs: number, s: AiSnapshot) => { now += secs * 1000; coach.observe(s, [], true); await flush(); };
  await step(0, quiet(1300)); // starts the clock
  for (let c = 1310; c <= 1380; c += 10) await step(10, quiet(c));
  assert.equal(bodies.length, 0, 'quiet game: nothing before 90 s');
  await step(10, quiet(1390)); assert.equal(bodies.length, 1, 'the quiet interval');
  assert.match(bodies[0], /WHY NOW: routine check, nothing new since 90 s ago: PASS unless/);
  // Two enemies die: an event, but at least 45 s after the last request.
  const window = (clock: number) => quiet(clock, { hero_status: [
    { ally: false, slot: 0, hero: 'hero_nano', dead: true, respawn_secs: 20, networth_observations: 3, observations: 3, position_observations: 0 },
    { ally: false, slot: 1, hero: 'hero_necro', dead: true, respawn_secs: 25, networth_observations: 3, observations: 3, position_observations: 0 }] });
  await step(20, window(1410)); assert.equal(bodies.length, 1, 'events wait EVENT_GAP_SECS after the last request');
  await step(26, window(1436)); assert.equal(bodies.length, 2);
  assert.match(bodies[1], /WHY NOW: enemy-dead window: 2 enemies dead \(nano, necro\), first back in 20s/);
  // A death and respawn: the respawn is an event (and the respawn shows in WHY NOW).
  await step(30, quiet(1466, { dead: true, screen: 'Dead', hp: null, death_cause: 'Melee' })); await flush();
  const deaths = bodies.length; assert.equal(deaths, 2, 'no death review yet (waits for the panel)');
  // 24:26 without a respawn timer read: a long respawn, so it waits up to 15 s for the rule death review.
  now += 9000; coach.observe(quiet(1475, { dead: true, screen: 'Dead', hp: null, death_cause: 'Melee' }), [], true); await flush();
  assert.equal(bodies.length, 2, 'waits for the rule death review');
  now += 7000; coach.observe(quiet(1482, { dead: true, screen: 'Dead', hp: null, death_cause: 'Melee' }), [], true); await flush();
  assert.equal(bodies.length, 3); assert.match(bodies[2], /^TASK: The player just died/);
  // The death review passed (nothing spoken), so the respawn asks; the Soul Urn at 25:00 is 5 s away.
  await step(20, quiet(1495)); assert.equal(bodies.length, 4);
  assert.match(bodies[3], /WHY NOW: respawn: you respawned 0s ago; objective within 60 s: Soul Urn spawns at 25:00 \(in 5s\)/);
  // Nothing new 50 s later: no request before the quiet interval.
  await step(50, quiet(1545)); const n = bodies.length; assert.equal(n, 4);
  await step(50, quiet(1595, { ally_networth: 20000, enemy_networth: 26000 }));
  assert.equal(bodies.length, n + 1); assert.match(bodies.at(-1)!, /big team souls swing: team souls difference went from \+2,000 to -6,000/);
  assert.equal(coach.status.quietRequests, 1); assert.equal(coach.status.eventRequests, 3);
  assert.match(coach.diagnostic(), /on changes, or every 90s when quiet/);
  // Phase change (laning → midgame at 8:00) is an event; the first phase seen is not.
  const h = new AiHistory(); h.push(quiet(470), [], 0); h.push(quiet(485), [], 15000);
  const phases = h.events.filter(e => e.kind === 'phase'); assert.equal(phases.length, 1); assert.match(phases[0].detail, /midgame phase started at 8:05/);
  // Souls reaching the usual buy is an event.
  const g = new AiHistory(); g.push(quiet(700, { souls: 2000 }), [], 0); g.push(quiet(710, { souls: 3300 }), [], 10000);
  assert.ok(g.events.some(e => e.kind === 'souls-tier' && /3,300 souls reach the usual 3,200 buy/.test(e.detail)));
  assert.deepEqual(liveTriggers(quiet(540), g, 20000).get('objective-soon:600'), 'Soul Urn spawns at 10:00 (in 60s)');
});

test('summary wording (rec11): lanes are places, roles and items are named, team souls trend, last death, Midboss only when it matters', () => {
  const items: Record<string, ItemInfo> = { upgrade_headshot_booster: { name: 'Headshot Booster', cost: 800, slot: 'weapon' }, upgrade_extra_health: { name: 'Extra Health', cost: 800, slot: 'vitality' } };
  const names: Record<string, string> = { hero_nano: 'Calico', hero_necro: 'Graves', hero_artist: 'Violet', hero_baba: 'Baba' };
  const name = (id: string) => names[id] || items[id]?.name || id;
  const h = new AiHistory(); h.name = name;
  h.push(snap(900, { ally_networth: 50000, enemy_networth: 50000 }), [], 0);
  h.push(snap(1000, { dead: true, screen: 'Dead', death_cause: 'Jar of Dead', death_damage: [{ source: 'Jar of Dead', damage: 879 }, { source: 'Melee', damage: 301 }] }),
    [{ key: 'death:review:1000', text: 'Why you died: Graves did at least 1,607 damage with Jar of Dead. Stay away.', priority: 'Normal', cooldown_secs: 0 }], 100000);
  h.push(snap(1040), [], 140000);
  const s = snap(1100, { hero: 'hero_artist', enemy_heroes: ['hero_nano', 'hero_necro', 'hero_baba'], ally_networth: 60000, enemy_networth: 66000,
    items: ['upgrade_headshot_booster', 'upgrade_extra_health'], hero_status: [] });
  s.minimap = { ...s.minimap!, heroes: [{ id: 2, ally: false, pos: { x: .2, y: .2 }, unseen_ms: 3000, hits: 3, lane: 'Yellow' }] };
  h.push(s, [], 200000);
  const out = buildSummary({ snapshot: s, history: h, now: 200000, trigger: 'periodic', name, items: id => items[id] ?? null, profile: 'Player profile (6 matches, mostly Grey Talon). Weak: Killed at close range: x.' });
  for (const part of ['You: Violet (role not in the coach table: do not guess the kit)', 'profile is mostly from Grey Talon games: check its weaknesses fit Violet',
    '1 enemy hero missing (last seen 3s ago in Yellow lane)', 'Green lane front 60% up (troopers 3 yours v 2 theirs)',
    'Enemy roles: Calico close-range diver; Graves ranged spirit caster with summons; others not in the coach table',
    'Your items (2): Headshot Booster (800), Extra Health (800) · invested by slot: weapon 800, vitality 800, spirit 0',
    'Team souls: yours 60,000 vs enemy 66,000 (-6,000) · at 15:00 it was +0: your team fell 6,000 further behind since',
    'Last death at 16:40 (1:40 ago): Graves did at least 1,607 damage with Jar of Dead; cause Jar of Dead · most damage Jar of Dead 879',
    'Objectives: Midboss alive (team target only after a won fight)']) assert.ok(out.includes(part), `${part}\n---\n${out}`);
  assert.ok(!/ago Yellow\b|enemy yellow/i.test(out), out);
  // No Midboss line in the laning phase; "a team take is possible" with 3+ enemies dead.
  assert.ok(!/Midboss/.test(buildSummary({ snapshot: snap(300), history: new AiHistory(), now: 0, trigger: 'periodic' })));
  const three = snap(1300, { hero_status: [0, 1, 2].map(slot => ({ ally: false, slot, hero: null, dead: true, networth_observations: 3, observations: 3, position_observations: 0 })) });
  assert.match(buildSummary({ snapshot: three, history: new AiHistory(), now: 0, trigger: 'periodic' }), /Midboss alive and 3 enemies dead: a team take is possible/);
  // The prompt forbids lane colours as heroes, asks for situation → action → why, and allows PASS.
  assert.match(SYSTEM_PROMPT, /Green, Blue and Yellow are LANES, never heroes/); assert.match(SYSTEM_PROMPT, /situation → specific action → why/); assert.match(SYSTEM_PROMPT, /"weakness"/);
  assert.ok(!/Do not die alone while it is up/.test(SYSTEM_PROMPT));
  // A low enemy is an opportunity angle, never a threat; the diver angle needs a live reason.
  const low = snap(1100, { hero_status: [{ ally: false, slot: 0, hero: 'hero_nano', health_ratio: .18, networth_observations: 3, observations: 3, position_observations: 0 }], enemy_heroes: ['hero_nano'] });
  low.minimap = { ...low.minimap!, heroes: [] };
  const angles = coachingAngles({ snapshot: low, history: new AiHistory(), now: 0, name, profile: 'Weak: Killed at close range: 5 of 6.' });
  assert.deepEqual(angles, ['- Calico at 18% HP and visible → a kill chance if they are in your range with an ally; a low enemy is a target, not a threat']);
  // Over the word limit the action sentence is kept, not a bare situation.
  assert.equal(cleanSpeech("Calico is at 18% and melee range is how you've died most. Stay at max range behind your front line until Paige and Apollo are back, then walk out.", 20, 2),
    'Stay at max range behind your front line until Paige and Apollo are back, then walk out.');
  assert.deepEqual(parseAdvice('{"speak":"Push Green lane.","priority":"Normal","why":"w","focus":"wave","weakness":"Dies alone"}'), { ok: true, advice: { speak: 'Push Green lane.', priority: 'Normal', why: 'w', focus: 'wave', weakness: 'Dies alone' } });
});
// ---------------------------------------------------------------------------------------
// rec12: match boundaries, evidence of the current death only, this match's heroes, reinforcement
// ---------------------------------------------------------------------------------------
const team12 = (enemies: string[], allies: string[], deadEnemies = 0) => [
  ...allies.map((hero, slot) => ({ ally: true, slot, hero, health_ratio: 1, networth_observations: 3, observations: 3, position_observations: 0 })),
  ...enemies.map((hero, slot) => ({ ally: false, slot, hero, dead: slot < deadEnemies, respawn_secs: slot < deadEnemies ? 20 : null, networth_observations: 3, observations: 3, position_observations: 0 })),
];
// rec11 (Violet, the previous match of the session) and rec12 (Grey Talon) rosters.
const V_ENEMIES = ['hero_ghost', 'hero_baba', 'hero_nurse', 'hero_dynamo', 'hero_nano', 'hero_necro'], V_ALLIES = ['hero_werewolf', 'hero_ratking', 'hero_artist', 'hero_bookworm', 'hero_fencer', 'hero_unicorn'];
const G_ENEMIES = ['hero_astro', 'hero_inferno', 'hero_atlas', 'hero_drifter', 'hero_gigawatt', 'hero_werewolf'], G_ALLIES = ['hero_mirage', 'hero_orion', 'hero_warden', 'hero_doorman', 'hero_nano', 'hero_haze'];
const m12 = (clock: number, hero: string, enemies: string[], allies: string[], extra: Partial<AiSnapshot> = {}, deadEnemies = 0): AiSnapshot =>
  snap(clock, { hero, enemy_heroes: enemies, ally_heroes: allies, hero_status: team12(enemies, allies, deadEnemies), ...extra });
const heroName = (id: string) => HERO_NAMES[id] || id;
const abilities = (): AbilityRow[] => JSON.parse(readFileSync(path.join(root, 'data', 'abilities.json'), 'utf8'));

test('rec12 match boundary: the clock restart is found across clock-less menus (same HUD match id); pauses and portrait misreads are not new matches', () => {
  const h = new AiHistory(); h.name = heroName;
  h.push(m12(2230, 'hero_artist', V_ENEMIES, V_ALLIES, { match_id: 127139499 }), [], 0);
  h.push(m12(2236, 'hero_artist', V_ENEMIES, V_ALLIES, { match_id: 127139499, dead: true, screen: 'Dead', death_cause: 'Feed The Birds', death_damage: [{ source: 'Feed The Birds', damage: 1438 }] }),
    [{ key: 'death:review:2236', text: 'Why you died: Baba did 71% of the damage (2,311) with Feed The Birds. At least 2 enemy heroes hit you: Baba and Graves.', priority: 'Normal', cooldown_secs: 0 }], 6000);
  h.noteAdvice({ t: 6000, clock: 2236, trigger: 'death', topic: 'positioning', text: "Stay out of Baba's reach until you are above 60% health." });
  assert.equal(h.deathCount, 1);
  // The old check compared with the previous SAMPLE only: these clock-less frames hid the restart.
  for (const [i, screen] of ['Shop', 'Countdown', 'Unknown'].entries()) h.push({ screen, hero: i ? null : 'hero_artist', game_clock_secs: null, match_id: 127139499 }, [], 60000 + i * 1000);
  h.push(m12(67, 'hero_orion', G_ENEMIES, G_ALLIES, { match_id: 127139499 }), [], 600000);
  assert.equal(h.match, 1); assert.match(h.boundaries[0].reason, /^the clock restarted \(37:16 → 1:07\)$/);
  assert.equal(h.deaths.length, 0); assert.equal(h.deathCount, 0); assert.equal(h.spoken.length, 0); assert.equal(h.advice.length, 0); assert.equal(h.hero, 'hero_orion');
  assert.deepEqual(h.rosterOf().enemies.sort(), [...G_ENEMIES].sort());
  const s = buildSummary({ snapshot: m12(70, 'hero_orion', G_ENEMIES, G_ALLIES), history: h, now: 603000, trigger: 'periodic', name: heroName });
  assert.ok(!/Baba|Graves|Feed The Birds|37:16/.test(s), s);
  // A game pause (clock barely moves over a long gap) and an early portrait misread are the same match.
  h.push(m12(500, 'hero_orion', G_ENEMIES, G_ALLIES), [], 1033000);
  h.push(m12(590, 'hero_orion', G_ENEMIES, G_ALLIES), [], 1500000);
  for (let i = 0; i < 4; i++) h.push(m12(591 + i, 'hero_mirage', G_ENEMIES, G_ALLIES), [], 1501000 + i * 1000);
  for (let i = 0; i < 4; i++) h.push(m12(596 + i, 'hero_orion', G_ENEMIES, G_ALLIES), [], 1506000 + i * 1000);
  assert.equal(h.match, 1); assert.equal(h.hero, 'hero_orion'); assert.ok(h.samples.length >= 10);
  // A post-game screen ends the match; the next gameplay frame is a new one.
  h.push({ screen: 'PostGame', game_clock_secs: null }, [], 1520000);
  h.push(m12(35, 'hero_haze', G_ENEMIES, G_ALLIES), [], 1700000);
  assert.equal(h.match, 2); assert.match(h.boundaries.at(-1)!.reason, /post-game/);
  // A hero change after a long gap is a new match even when the clock looks plausible.
  h.push(m12(300, 'hero_haze', G_ENEMIES, G_ALLIES), [], 1965000);
  h.push(m12(400, 'hero_wraith', G_ENEMIES, G_ALLIES), [], 2400000);
  assert.equal(h.match, 3); assert.match(h.boundaries.at(-1)!.reason, /hero changed \(haze → wraith\) after a 435 s gap/);
});

test('rec12 death evidence: only this death\'s rule review, damage attributed to this match\'s enemies (Drifter Rend, Silver Boot Kick), a "Trooper" cause that was hero damage, and replies naming other matches\' heroes are cut', () => {
  const h = new AiHistory(); h.name = heroName; h.setAbilities(abilities());
  let t = 0; const at = (c: number, extra: Partial<AiSnapshot> = {}, cues: Parameters<AiHistory['push']>[1] = []) => { h.push(m12(c, 'hero_orion', G_ENEMIES, G_ALLIES, extra), cues, t); t += 3000; };
  for (let c = 870; c <= 886; c += 4) at(c, { souls: 6073 });
  at(890, { dead: true, screen: 'Dead', souls: 6073, respawn_secs: 19, death_cause: 'Gun Damage', status_effects: ['out_of_stamina'],
    death_damage: [{ source: 'Melee', damage: 237 }, { source: 'Rend', damage: 222 }, { source: 'Gun Damage', damage: 190 }, { source: 'Boot Kick', damage: 168 }] });
  const first = buildSummary({ snapshot: m12(890, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Dead', souls: 6073 }), history: h, now: t, trigger: 'death', name: heroName });
  assert.match(first, /Damage by hero \(ability names matched to this match's enemies\): Drifter 222 \(Rend\), Silver 168 \(Boot Kick\); not attributable to one hero: Melee 237, Gun Damage 190/);
  // Its rule review (spoken 12 s later) belongs to this death only.
  at(902, { dead: true, screen: 'Dead', souls: 132 }, [{ key: 'death:review:890', text: 'Why you died: Drifter did at least 222 damage with Rend. At least 2 enemy heroes hit you: Drifter and Silver.', priority: 'Normal', cooldown_secs: 0 }]);
  at(906, { dead: true, screen: 'Dead', souls: 132 });
  for (let c = 912; c <= 1325; c += 6) at(c, { souls: 2060 });
  at(1330, { dead: true, screen: 'Dead', souls: 2060, respawn_secs: 62, death_cause: 'Trooper', death_killing_blow: 'Melee',
    death_damage: [{ source: 'Gun Damage', damage: 481 }, { source: 'Rend', damage: 407 }, { source: 'Melee', damage: 387 }, { source: 'Melee', damage: 363 }] });
  const dead = m12(1334, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Dead', souls: 2060 });
  h.push(dead, [], t);
  const s = buildSummary({ snapshot: dead, history: h, now: t, trigger: 'death', name: heroName });
  assert.ok(!/Rule death review/.test(s), `the 14:50 review is not evidence for the 22:10 death\n${s}`);
  assert.match(s, /Earlier Death at 14:50/); assert.match(s, /Death at 22:10 \(THIS death\): cause Trooper/);
  assert.match(s, /Damage by hero[^\n]*: Drifter 407 \(Rend\)[^\n]*the panel cause "Trooper" is misleading: hero abilities did 25% of the listed damage, so enemy heroes killed you, not troopers/);
  assert.ok(!/Baba|Graves/.test(s));
  assert.match(TASKS.death, /Name only heroes of this match/); assert.match(TASKS.death, /cause says Trooper but hero abilities/);
  assert.equal(damageByHero([{ source: 'Stalkers MarkX', damage: 300 }], h).heroes[0]?.hero, 'hero_drifter', 'OCR suffixes still match');
  // Replies naming heroes of the previous match (the live rec12 answers) are cut or dropped.
  assert.deepEqual(strangerHeroes("Died at melee range to Baba and Graves while out of stamina.", h), ['Graves', 'Baba']);
  assert.deepEqual(strangerHeroes('Seven seconds until Drifter respawns: Seven is still alive.', h), []);
  const v = soulsView(dead, h, t);
  const cut = checkAdvice({ speak: "Baba's Feed The Birds finished you at 41%. Next life, hold max range behind your front line and heal above 60%.", priority: 'High', why: '', focus: 'death' }, 'death', v, h, t);
  assert.ok(cut.ok && cut.speak === 'Next life, hold max range behind your front line and heal above 60%.' && /Baba, not in this match/.test(cut.note || ''), JSON.stringify(cut));
  const gone = checkAdvice({ speak: 'Died at melee range to Baba and Graves while out of stamina and silenced.', priority: 'High', why: '', focus: 'death' }, 'manual', v, h, t);
  assert.ok(!gone.ok && /names Graves, Baba, not in this match/.test(gone.reason), JSON.stringify(gone));
  assert.ok(checkAdvice({ speak: 'Drifter and Silver caught you alone in the jungle. Next life, walk with Warden.', priority: 'High', why: '', focus: 'death' }, 'death', v, h, t).ok);
});

test('death-screen shopping: souls advice uses what is left to spend, and the spend is a good play', () => {
  const h = new AiHistory();
  h.push(m12(880, 'hero_orion', G_ENEMIES, G_ALLIES, { souls: 6073 }), [], 0);
  h.push(m12(890, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Dead', souls: 2873 }), [], 2000);
  h.push(m12(894, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Shop', souls: 132, hp: null, max_hp: null }), [], 6000);
  h.push(m12(900, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Dead', souls: 132 }), [], 12000);
  const v = soulsView(m12(900, 'hero_orion', G_ENEMIES, G_ALLIES, { dead: true, screen: 'Dead', souls: 132 }), h, 12000);
  assert.equal(v.souls, 132); assert.equal(v.allowed, false); assert.match(v.reason, /already spent about 5,900 on this death screen/);
  assert.equal(h.good.filter(g => g.kind === 'death-shop').length, 1); assert.match(h.good[0].text, /^spent about 5,900 souls on the death screen after the 14:50 death/);
});

test('reinforce: a fresh good play may be praised once, briefly and specifically, at most 1 in 4 AI tips; no praise without evidence', () => {
  const h = new AiHistory(); let t = 0;
  const push = (s: AiSnapshot) => { h.push(s, [], t); t += 4000; };
  const v = () => soulsView(h.samples.length ? m12(h.samples.at(-1)!.clock!, 'hero_orion', G_ENEMIES, G_ALLIES) : snap(0), h, t);
  const a = (speak: string, focus = 'reinforce'): Advice => ({ speak, focus, priority: 'Normal', why: '' });
  push(m12(800, 'hero_orion', G_ENEMIES, G_ALLIES));
  // No good play yet: praise is dropped (or the answer is), whatever the model tags it.
  const none = checkAdvice(a('Good push, keep doing that.'), 'periodic', v(), h, t);
  assert.ok(!none.ok && /praise not allowed: no fresh good play/.test(none.reason), JSON.stringify(none));
  const kept = checkAdvice(a('Nice work in lane. Group with Warden and hit the Yellow lane Guardian now.', 'objective'), 'periodic', v(), h, t);
  assert.ok(kept.ok && kept.speak === 'Group with Warden and hit the Yellow lane Guardian now.' && /praise dropped/.test(kept.note || ''), JSON.stringify(kept));
  assert.equal(reinforceGate(h, t).allowed, false);
  // 13:54: five enemies dead and you are in the enemy base: a used enemy-dead window.
  const base = (c: number, dead: number) => m12(c, 'hero_orion', G_ENEMIES, G_ALLIES, { minimap: { ...snap(c).minimap!, zone_area: 'EnemyBase', your_lane: null } }, dead);
  push(m12(825, 'hero_orion', G_ENEMIES, G_ALLIES, {}, 5)); push(base(834, 5)); push(base(838, 4));
  assert.deepEqual(h.good.map(g => g.kind), ['window-push']);
  assert.match(h.good[0].text, /^pushed into the enemy base at 13:54 while 5 enemies were dead/);
  assert.ok(h.events.some(e => e.kind === 'good'), 'a good play is a cadence event');
  const summary = buildSummary({ snapshot: base(842, 4), history: h, now: t, trigger: 'periodic' });
  assert.match(summary, /GOOD PLAYS \(this match, observed; evidence for praise\):\n- \[13:54\] pushed into the enemy base at 13:54 while 5 enemies were dead[^\n]*\nPraise: allowed \(fresh good play: pushed into the enemy base/);
  assert.match(SYSTEM_PROMPT, /Reinforce what works: when GOOD PLAYS says "Praise: allowed"/); assert.match(SYSTEM_PROMPT, /wave, mechanics, reinforce"/);
  const ok = checkAdvice(a('Good push while five were dead: that is how a lead snowballs. Back off before they respawn.'), 'periodic', v(), h, t);
  assert.ok(ok.ok && ok.topic === 'reinforce' && ok.praised === 'window-push:834', JSON.stringify(ok));
  h.noteAdvice({ t, clock: 842, trigger: 'periodic', topic: 'reinforce', text: ok.ok ? ok.speak : '' }); h.praised.push('window-push:834');
  assert.equal(reinforceGate(h, t).allowed, false, 'praised once');
  // The urn delivered right after: fresh, but one of the last 3 tips was praise (at most 1 in 4).
  push(m12(850, 'hero_orion', G_ENEMIES, G_ALLIES, { souls: 479, status_effects: ['carrying_urn'] }));
  push(m12(856, 'hero_orion', G_ENEMIES, G_ALLIES, { souls: 482 })); push(m12(862, 'hero_orion', G_ENEMIES, G_ALLIES, { souls: 2607 }));
  assert.ok(h.good.some(g => g.kind === 'urn' && /^delivered the Soul Urn at 14:22 \(\+2,128 souls in 12 s\)/.test(g.text)), JSON.stringify(h.good));
  const gate = reinforceGate(h, t); assert.equal(gate.allowed, false); assert.match(gate.reason, /at most 1 in 4/);
  const blocked = checkAdvice(a('Good urn delivery, that was clean. Push Yellow lane with Warden now.', 'objective'), 'periodic', v(), h, t);
  assert.ok(blocked.ok && blocked.speak === 'Push Yellow lane with Warden now.' && blocked.topic !== 'reinforce', JSON.stringify(blocked));
  // Three other tips and 150 s later, a new good play may be praised again.
  for (const [i, text] of ['Push Yellow lane with Warden now.', 'Check the minimap before the next camp.', 'Hold the Blue wave under your Guardian.'].entries())
    h.noteAdvice({ t: t + i, clock: 870 + i * 40, trigger: 'periodic', topic: ['wave', 'map', 'objective'][i], text });
  push(m12(1000, 'hero_orion', G_ENEMIES, G_ALLIES, { hp: 300, max_hp: 1000 })); push(m12(1040, 'hero_orion', G_ENEMIES, G_ALLIES, { hp: 950, max_hp: 1000 }));
  const later = reinforceGate(h, t);
  assert.ok(later.allowed && later.play?.kind === 'reset', JSON.stringify(later));
  // A praise of death-screen shopping is not souls advice (the souls gate does not cut it).
  assert.ok(isPraise('Good shopping on the death screen: you spent 5,900 souls.') && !isPraise('Stay at max range behind Warden.'));
});

test('controller: a good play asks sooner (WHY NOW) and a reinforcement is spoken with focus reinforce', async () => {
  let now = 100000; const bodies: string[] = []; const delivered: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(String(init.body));
    return new Response(JSON.stringify({ choices: [{ message: { content: JSON.stringify({ speak: 'Good push while five were dead: that is how a lead snowballs. Back off before they respawn.', priority: 'Normal', why: 'GOOD PLAYS', focus: 'reinforce' }) } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings({ aiInterval: 300 }), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, onAdvice: c => delivered.push(`${c.key}|${c.text}`) });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  const base = (c: number, dead: number) => m12(c, 'hero_orion', G_ENEMIES, G_ALLIES, { minimap: { ...snap(c).minimap!, zone_area: 'EnemyBase', your_lane: null } }, dead);
  coach.observe(m12(780, 'hero_orion', G_ENEMIES, G_ALLIES), [], true);
  now += 50000; coach.observe(m12(830, 'hero_orion', G_ENEMIES, G_ALLIES, {}, 1), [], true); await flush();
  assert.equal(bodies.length, 0, 'nothing changed');
  now += 4000; coach.observe(base(834, 1), [], true); await flush(); assert.equal(bodies.length, 0, 'one enemy dead is no window');
  now += 4000; coach.observe(base(838, 5), [], true); await flush();
  // Four enemies fell at once: a fight is on (2+ deaths in 10 s), so the rule coach has it.
  assert.equal(bodies.length, 0, 'no periodic request during a fight'); assert.equal(coach.status.fightSkips, 1);
  now += 12000; coach.observe(base(850, 5), [], true); await flush();
  assert.equal(bodies.length, 1);
  const user = JSON.parse(bodies[0]).messages[1].content as string;
  assert.match(user, /WHY NOW: [^\n]*good play \(see GOOD PLAYS\): pushed into the enemy base at 13:58 while 5 enemies were dead/);
  assert.deepEqual(delivered, ['ai:periodic:reinforce|Good push while five were dead: that is how a lead snowballs. Back off before they respawn.']);
  assert.deepEqual(coach.history.praised, ['window-push:838']);
});

// ---------------------------------------------------------------------------------------
// rec13 (docs/coach-review-13.md items 12-14): game state, one coach voice, the death review
// ---------------------------------------------------------------------------------------
const E13 = ['hero_inferno', 'hero_atlas', 'hero_vampirebat', 'hero_ghost', 'hero_chrono', 'hero_necro'], A13 = ['hero_orion', 'hero_bebop', 'hero_familiar', 'hero_mirage', 'hero_fencer', 'hero_punkgoat'];
const NW13 = [49000, 38000, 35000, 41000, 35000, 35000];
const team13 = (timers: number[] = [], alliesDead = 0) => [
  ...A13.map((hero, slot) => ({ ally: true, slot, hero, health_ratio: 1, dead: slot > 0 && slot <= alliesDead, respawn_secs: slot > 0 && slot <= alliesDead ? 20 : null, networth: 30000, networth_observations: 3, observations: 3, position_observations: 0 })),
  ...E13.map((hero, slot) => ({ ally: false, slot, hero, health_ratio: 1, networth: NW13[slot], dead: slot < timers.length, respawn_secs: timers[slot] ?? null, networth_observations: 3, observations: 3, position_observations: 0 })),
];
const s13 = (clock: number, extra: Partial<AiSnapshot> = {}, timers: number[] = [], alliesDead = 0): AiSnapshot =>
  snap(clock, { hero: 'hero_orion', enemy_heroes: E13, ally_heroes: A13, hero_status: team13(timers, alliesDead), hp: 2000, max_hp: 2100, souls: 1000, ...extra });

test('rec13 state: structures, Midboss holder, fed enemy, enemy items, K/D/A, fight state, throw, ultimate, focus goal, latency-aware window', () => {
  const items: Record<string, ItemInfo> = { upgrade_titan_round: { name: 'Titanic Magazine', cost: 3200, slot: 'weapon' }, upgrade_magic_shield: { name: 'Spirit Shielding', cost: 1600, slot: 'vitality' } };
  const name = (id: string) => HERO_NAMES[id] || items[id]?.name || id;
  const h = new AiHistory(); h.name = name; let t = 0;
  const push = (s: AiSnapshot) => { h.push(s, [], t); t += 5000; };
  push(s13(2000, { ally_networth: 150000, enemy_networth: 120000 }));
  push(s13(2190, { ally_networth: 176000, enemy_networth: 130000, players: [{ ally: false, hero: 'hero_inferno', items: ['upgrade_magic_shield', 'upgrade_titan_round'] }, { ally: true, hero: 'hero_orion', items: [], kills: 23, deaths: 5, assists: 11 }] }));
  push(s13(2360, { ally_networth: 180000, enemy_networth: 150000, kills: null, deaths: null, assists: null }));
  // Three enemies fall within a few seconds while you are at 30%: a won fight, on now.
  const now = s13(2370, { ally_networth: 180000, enemy_networth: 150000, hp: 630, structures: [{ lane: 'Yellow', kind: 'walker', own: false, alive: true }, { lane: 'Blue', kind: 'guardian', own: false, alive: false }, { lane: 'Green', kind: 'guardian', own: true, alive: true }],
    midboss_taken_by: 'enemy', abilities: [{ slot: 1, ready: true }, { slot: 4, ready: true }] }, [30, 25, 20]);
  push(now);
  const f = h.fightState(t); assert.equal(f.kind, 'won'); assert.equal(f.on, true); assert.equal(f.enemies.length, 3);
  const out = buildSummary({ snapshot: now, history: h, now: t, trigger: 'periodic', name, items: id => items[id] ?? null, goal: 'No deaths below 35% health after a won fight (so far: on track, no deaths)' });
  for (const part of [
    'Word budget: at most 10 words (fight on) · heard about 6 s after this state',
    'MATCH FOCUS GOAL: No deaths below 35% health after a won fight (so far: on track, no deaths)',
    'Fight: ON NOW (3 heroes died in the last 10 s) · just WON a fight: 3 enemies died in the last 20 s (Infernus, Abrams, Mina) vs 0 of ours · you are at 30%: heal first, objective second',
    'Enemy-dead window: at least 2 stay dead about 19 s after you are heard',
    'You: Grey Talon (long-range marksman) · K/D/A 23/5/11', 'Your ultimate: ready',
    'Enemy items (scoreboard, 3:00 ago): Infernus: Titanic Magazine, Spirit Shielding',
    'Fed enemy: Infernus 49,000 net worth vs next Lady Geist 41,000 (+8,000): their win condition',
    'THROW RISK: we were +46,000 and it is shrinking: protect the lead, no base dives without the Rejuvenator',
    'Structures (read; a structure not listed is unknown, never a target): theirs: Blue Guardian destroyed; Yellow Walker standing · ours: Green Guardian standing',
    'Rejuvenator: THEY hold it until about 42:36 (3:06 left) (Midboss taken by the enemy at about 39:30): their deaths may revive in place, so no fights in their base until it runs out']) assert.ok(out.includes(part), `${part}\n---\n${out}`);
  assert.ok(out.length <= SUMMARY_MAX_CHARS);
  // Budgets per moment; a window that closes within 10 s of being heard.
  assert.deepEqual(wordBudget('death', now, h, t), { words: 30, sentences: 2, why: 'death screen' });
  assert.equal(wordBudget('periodic', snap(300), new AiHistory(), 0).words, 15);
  assert.equal(wordBudget('periodic', snap(900), new AiHistory(), 0).words, 20);
  const short = s13(2380, {}, [14, 12]); h.push(short, [], t + 20000);
  assert.equal(h.windowLeft(short), 6);
  assert.match(buildSummary({ snapshot: short, history: h, now: t + 20000, trigger: 'periodic', name }), /Enemy-dead window: at least 2 stay dead about 6 s after you are heard: too short for a structure, PASS or call the reset/);
  // No structure state at all: the summary forbids naming one.
  assert.equal(structuresLine(new AiHistory(), 600), 'Structures: not read: do not name a specific Guardian, Walker or Shrine as the target');
  // The prompt: no wiki block, the essentials in the system prompt, the profile without other matches' heroes.
  const profile = "Player profile (7 matches, mostly Violet). Weak: Killed at close range: 2 of 2 deaths (melee hits x1, Victor's Aura of Suffering x1); Same killer again: Graves killed you 3 times. Focus: keep distance.";
  const scrubbed = scrubProfile(profile);
  assert.ok(!/Victor|Graves/.test(scrubbed) && /mostly Violet/.test(scrubbed), scrubbed);
  assert.equal(scrubProfile('Seven of 9 deaths came from melee.'), 'Seven of 9 deaths came from melee.');
  const p = buildPrompt({ trigger: 'periodic', summary: out, profile });
  assert.ok(!p.user.includes('DEADLOCK FACTS') && !p.user.includes('Graves killed') && p.user.includes('PLAYER PROFILE (earlier matches'));
  for (const rule of [/"we" for the team/, /Name one teammate/, /the why in six words or fewer/, /Decide the next 30 seconds/, /only to a structure STATE lists as standing/, /closes within 10 seconds of being heard, PASS or call the reset/, /a Walker is 3,675 team souls/, /MATCH FOCUS GOAL/])
    assert.match(SYSTEM_PROMPT, rule);
});

test('one coach voice: rule-call themes (plan:, threat:, lead:, reset:) within 30 s, structures not standing, windows too short, one AI death call per death', async () => {
  const a = (speak: string, focus: string): Advice => ({ speak, focus, priority: 'Normal', why: '' });
  assert.deepEqual(ruleThemes('plan:walker:Yellow'), ['objective', 'team']); assert.deepEqual(ruleThemes('threat:hero_atlas'), ['positioning', 'map']);
  assert.deepEqual(ruleThemes('lead:throw'), ['team', 'objective']); assert.deepEqual(ruleThemes('reset:heal'), ['healing']); assert.deepEqual(ruleThemes('pos:enemies_down'), ['objective', 'team']);
  const h = new AiHistory(); h.name = heroName;
  h.push(s13(1800, { structures: [{ lane: 'Yellow', kind: 'walker', own: false, alive: true }, { lane: 'Blue', kind: 'guardian', own: false, alive: false }] }), [], 0);
  h.push(s13(1810), [{ key: 'plan:walker:Yellow', text: 'Plan: their Yellow Walker is weak, we go together at 30:30.', priority: 'Normal', cooldown_secs: 60 }], 10000);
  h.push(s13(1820), [], 20000);
  const v = soulsView(s13(1820), h, 20000);
  const take = a('We take their Yellow Walker with Bebop now, structures beat kills.', 'objective');
  const r1 = checkAdvice(take, 'periodic', v, h, 20000);
  assert.ok(!r1.ok && /same theme \(objective\) as the rule call plan:walker:Yellow at 30:10/.test(r1.reason), JSON.stringify(r1));
  assert.ok(ruleCovered('objective', h, 20000) && !ruleCovered('healing', h, 20000));
  h.push(s13(1845), [], 45000);
  assert.ok(checkAdvice(take, 'periodic', soulsView(s13(1845), h, 45000), h, 45000).ok, '30 s later the theme is free');
  // A structure read as destroyed, or never read, is never the target; our own Guardian is a place to heal.
  const gone = checkAdvice(a('We hit their Blue Guardian with Rem now, it is our best push.', 'objective'), 'manual', v, h, 45000);
  assert.ok(!gone.ok && /names their Blue Guardian, read as destroyed/.test(gone.reason), JSON.stringify(gone));
  assert.equal(structureProblem('Take the Green Shrine with Mirage.', h), 'names their Green Shrine, not read as standing');
  assert.equal(structureProblem('Heal at our Green Guardian with Bebop first.', h), null);
  assert.equal(structureProblem('We take their Blue Guardian.', new AiHistory()), null, 'no structure state: the summary rule applies, not the gate');
  const cut = checkAdvice(a('Heal up to full with Bebop first. Then we hit their Blue Guardian.', 'healing'), 'periodic', v, h, 45000);
  assert.ok(cut.ok && cut.speak === 'Heal up to full with Bebop first.' && /read as destroyed/.test(cut.note || ''), JSON.stringify(cut));
  // An enemy-dead window that closes 6 s after it is heard (timers 14 and 12, latency 6 s).
  h.push(s13(1850, {}, [14, 12]), [], 50000);
  const late = checkAdvice(a('Push with Rem now: we take their Yellow Walker.', 'objective'), 'periodic', soulsView(s13(1850), h, 50000), h, 50000);
  assert.ok(!late.ok && /window closes about 6 s after it is heard/.test(late.reason), JSON.stringify(late));
  // The death review: one death despite the Shop / health-less Gameplay frames of a 64 s death
  // (rec13 41:05), one AI request, and an answer with the rule review's lesson is not spoken.
  let now = 1e6; const bodies: string[] = []; const delivered: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(JSON.parse(String(init.body)).messages[1].content);
    return new Response(JSON.stringify({ choices: [{ message: { content: JSON.stringify({ speak: 'Melee caught you alone. Next life, walk out with a teammate after you respawn.', priority: 'Normal', why: 'w', focus: 'positioning' }) } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings(), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, name: heroName, onAdvice: c => delivered.push(c.text) });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  const review = { key: 'death:review:2480', text: 'Why you died: Melee was the biggest source on the damage panel (2,322). You walked out alone 5 seconds after respawning. Next time: After a respawn, group with a teammate or catch a wave near your guardian first.', priority: 'Normal' as const, cooldown_secs: 0 };
  const seq: Array<[number, Partial<AiSnapshot>, typeof review[]]> = [[2442, {}, []], [2461, { hp: 713 }, []], [2465, { dead: true, screen: 'Shop', hp: null, max_hp: null }, []], [2469, { dead: true, screen: 'Dead', hp: null, death_cause: 'Melee' }, []],
    [2477, { screen: 'Gameplay', hp: null, max_hp: null }, []], [2480, { dead: true, screen: 'Dead', hp: null }, []], [2490, { dead: true, screen: 'Dead', hp: null, respawn_secs: 64 }, [review]], [2494, { dead: true, screen: 'Dead', hp: null, respawn_secs: 60 }, []],
    [2504, { screen: 'Gameplay', hp: null, max_hp: null }, []], [2509, { screen: 'Gameplay', hp: null, max_hp: null }, []], [2513, { dead: true, screen: 'Dead', hp: null }, []], [2528, { dead: true, screen: 'Dead', hp: null }, []], [2555, { hp: 3000, max_hp: 3201 }, []]];
  let last = 2442;
  for (const [clock, extra, cues] of seq) { now += (clock - last) * 1000; last = clock; coach.observe(s13(clock, extra), cues, true); await flush(); }
  assert.equal(coach.history.deathCount, 1, 'one death, not three');
  assert.equal(bodies.filter(b => /^TASK: The player just died/.test(b)).length, 1, 'one AI death request');
  assert.match(bodies[0], /Rule death review of this death \(already spoken\): Why you died: Melee/);
  assert.deepEqual(delivered, [], 'the same lesson as the rule review is not spoken twice');
  assert.match(coach.status.message, /same next-life lesson as the rule death review|repeats the rule call "Why you died/);
  assert.match(deathCovered('Abrams dove you in their jungle. Next life, walk out with a teammate first.', coach.history)!, /same next-life lesson as the rule death review \("After a respawn, group with a teammate/);
  assert.equal(deathCovered('Abrams dove you while Bebop and Rem were down. Next life, back off to Bebop when two allies fall.', coach.history), null, 'a different lesson is spoken');
});

test('fight state: no periodic AI request during a fight; an answer that arrives in a fight is dropped', async () => {
  let now = 1e6; const bodies: string[] = []; const delivered: string[] = []; let gate: Promise<void> = Promise.resolve();
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(String(init.body)); await gate;
    return new Response(JSON.stringify({ choices: [{ message: { content: JSON.stringify({ speak: 'Abrams is their engage: we hold behind Bebop and kite.', priority: 'Normal', why: 'w', focus: 'positioning' }) } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings({ aiInterval: 20 }), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, name: heroName, onAdvice: c => delivered.push(c.text) });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  const step = async (secs: number, s: AiSnapshot) => { now += secs * 1000; coach.observe(s, [], true); await flush(); };
  await step(0, s13(1500)); await step(10, s13(1510));
  await step(5, s13(1515, {}, [30, 28])); // two enemies die at once: a fight is on
  await step(6, s13(1521, {}, [24, 22])); assert.equal(bodies.length, 0, 'the 20 s quiet interval passed, but the fight blocks it');
  assert.equal(coach.status.fightSkips, 1);
  await step(10, s13(1531, {}, [14, 12])); assert.equal(bodies.length, 1, 'the fight is over 10 s after the deaths');
  // A request in flight when a fight starts: the answer is dropped.
  let release!: () => void; gate = new Promise(r => { release = r; });
  await step(50, s13(1581)); assert.equal(bodies.length, 2);
  coach.history.push(s13(1582), [], now); coach.history.push(s13(1583, {}, [], 2), [], now + 1000);
  release(); await flush(); await flush();
  assert.equal(coach.status.fightDrops, 1); assert.match(coach.status.message, /a fight started/);
  assert.match(coach.diagnostic(), /fights left to the rule coach/);
});

// The live rec11 → rec12 session (local recordings, not in the repository): with the previous match
// in the history first, as it was live, no rec12 death prompt names a previous-match hero.
test('rec12 recording: death prompts carry only this match\'s heroes and the death evidence of each death', async () => {
  const dir = 'C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
  const files = ['rec11', 'rec12'].map(r => `${dir}/${r}/recording.json`);
  if (!files.every(f => { try { readFileSync(f, { flag: 'r' }); return true; } catch { return false; } })) return;
  type Rec = { frames: Array<{ timestamp: number; snapshot: AiSnapshot; cues?: Array<{ key: string; text: string; priority: 'High' | 'Normal' | 'Low'; cooldown_secs: number }> }> };
  const [r11, r12] = files.map(f => JSON.parse(readFileSync(f, 'utf8')) as Rec);
  const frames = [...r11.frames.filter(f => f.timestamp < r12.frames[0].timestamp), ...r12.frames];
  let now = 0; const prompts: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { const user = JSON.parse(String(init.body)).messages[1].content as string; if (/^Trigger: death/m.test(user)) prompts.push(user);
    return new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":null,"priority":"Low","why":"pass","focus":"pass"}' } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings(), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, name: heroName, onAdvice: () => {} });
  coach.history.setAbilities(abilities());
  // Only wait while a request is in flight (a timer per frame made this test take a minute on Windows).
  const flush = async () => { for (let i = 0; i < 50 && coach.busy; i++) await new Promise(r => setImmediate(r)); };
  let start = -1;
  for (const f of frames) { now = f.timestamp; if (start < 0 && f.timestamp >= r12.frames[0].timestamp) start = prompts.length; coach.observe(f.snapshot, f.cues || [], true); await flush(); }
  const mine = prompts.slice(start);
  assert.equal(mine.length, 3, mine.map(p => p.match(/clock (\S+)/)?.[1]).join(', '));
  assert.ok(coach.history.match >= 1, 'the Violet → Grey Talon boundary was found');
  for (const p of mine) {
    const state = p.split('\nPLAYER PROFILE')[0];
    for (const stale of ['Baba', 'Graves', 'Violet', 'Feed The Birds', 'Jar of Dead', 'Deaths observed this session']) assert.ok(!state.includes(stale), `${stale} in\n${state}`);
  }
  assert.match(mine[1], /Damage by hero[^\n]*Drifter 222 \(Rend\), Silver 168 \(Boot Kick\)/);
  assert.match(mine[2], /the panel cause "Trooper" is misleading/);
});

// ---------------------------------------------------------------------------------------
// rec8 (docs/coach-review-8.md D11): praise, the death panel, the profile cut, the Rejuvenator, mastery
// ---------------------------------------------------------------------------------------
test('death praise (rec8 21:01): a play that led to the death is never praised; a push you came back from still is', () => {
  const h = new AiHistory(); let t = 0;
  const push = (s: AiSnapshot) => { h.push(s, [], t); t += 4000; };
  // 20:14 a deep Yellow push with 3 enemies dead, still in their half at 20:35, dead at 20:40.
  const deep = (c: number, dead: number, y = 0.28) => s13(c, { minimap: { ...snap(c).minimap!, you: { x: 0.3, y }, your_lane: 'Yellow', your_progress: 0.8, zone_area: 'Lane' } }, Array(dead).fill(25));
  push(deep(1200, 0)); push(deep(1210, 3)); push(deep(1214, 3)); push(deep(1225, 1)); push(deep(1235, 1, 0.34));
  assert.equal(h.good.filter(g => g.kind === 'window-push').length, 1);
  const g = h.good.find(x => x.kind === 'window-push')!;
  assert.equal(deathAfter(h, g), null, 'no death yet');
  push(s13(1240, { dead: true, screen: 'Dead', hp: null, max_hp: null, minimap: null }));
  assert.equal(deathAfter(h, g)?.clock, 1240, 'never came back to our half: the push led to the death');
  const gate = reinforceGate(h, t); assert.equal(gate.allowed, false); assert.match(gate.reason, /the 20:10 play led to the 20:40 death/);
  const summary = buildSummary({ snapshot: s13(1250, { dead: true, screen: 'Dead', hp: null, max_hp: null }), history: h, now: t, trigger: 'death' });
  assert.match(summary, /- \[20:10\] pushed Yellow lane deep at 20:10 while 3 enemies were dead[^\n]*\(led to the 20:40 death: never praise it\)\nPraise: not allowed \(the 20:10 play led to the 20:40 death\)/);
  assert.doesNotMatch(summary, /Praise: allowed/);
  // Within 20 s before a death it is tainted even after walking back; walking back and dying later is not.
  const h2 = new AiHistory(); t = 0; const p2 = (s: AiSnapshot) => { h2.push(s, [], t); t += 4000; };
  p2(deep(1200, 0)); p2(deep(1210, 3)); p2(deep(1214, 3)); p2(deep(1222, 1, 0.6)); p2(deep(1240, 0, 0.7)); p2(deep(1290, 0, 0.7));
  p2(s13(1300, { dead: true, screen: 'Dead', hp: null, max_hp: null, minimap: null }));
  assert.equal(deathAfter(h2, h2.good.find(x => x.kind === 'window-push')!), null, 'came back to our half; the death 86 s later is another story');
  const h3 = new AiHistory(); t = 0; const p3 = (s: AiSnapshot) => { h3.push(s, [], t); t += 4000; };
  p3(deep(1200, 0)); p3(deep(1210, 3)); p3(deep(1214, 3)); p3(deep(1222, 1, 0.6));
  p3(s13(1230, { dead: true, screen: 'Dead', hp: null, max_hp: null, minimap: null }));
  assert.equal(deathAfter(h3, h3.good.find(x => x.kind === 'window-push')!)?.clock, 1230, '16 s before the death');
  assert.equal(DEATH_PRAISE_SECS, 20);
});

test('death call waits for the damage panel (rec8 20:40: shop open, panel read at 21:10), within the respawn timer', async () => {
  let now = 100000; const bodies: string[] = [];
  const fetchImpl = (async (_u: string, init: RequestInit) => { bodies.push(String(init.body)); return new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":"Pocket and Kelvin collapsed on you deep in Yellow. Next life stop at their Walker.","priority":"Normal","why":"panel","focus":"positioning"}' } }] })); }) as unknown as typeof fetch;
  const coach = new AiCoach({ settings: () => settings(), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl, onAdvice: () => {} });
  const flush = async () => { for (let i = 0; i < 6; i++) await new Promise(r => setTimeout(r, 0)); };
  const dead = (c: number, extra: Partial<AiSnapshot> = {}) => s13(c, { dead: true, screen: 'Dead', hp: null, max_hp: null, ...extra });
  coach.observe(s13(1235), [], true);
  now += 5000; coach.observe(dead(1240), [], true); await flush();
  // The shop hides the panel: a rule review alone (no cause) gives it 4 s more, not "cause unknown" at once.
  now += 4000; coach.observe(dead(1244, { screen: 'Shop', respawn_secs: 55 }), [], true); await flush();
  now += 17000; coach.observe(dead(1261, { screen: 'Shop', respawn_secs: 37 }), [], true); await flush();
  assert.equal(bodies.length, 0, 'still waiting for the panel at 21:01 (the old call went out here)');
  now += 9000; coach.observe(dead(1270, { respawn_secs: 29, death_cause: 'Tankbuster', death_damage: [{ source: 'Tankbuster', damage: 479 }, { source: 'Barrage', damage: 459 }] }), [{ key: 'death:review:1240', text: 'You died because they burst you from 92% in 8 seconds.', priority: 'High', cooldown_secs: 0 }], true); await flush();
  assert.equal(bodies.length, 1);
  const user = JSON.parse(bodies[0]).messages[1].content as string;
  assert.match(user, /Death at 20:40[^\n]*: cause Tankbuster · damage taken: Tankbuster 479, Barrage 459/); assert.doesNotMatch(user, /cause unknown/);
  // Never into the last 10 s of the respawn timer: then it asks with what it has.
  now = 900000; const b2: string[] = [];
  const c2 = new AiCoach({ settings: () => settings(), isLive: () => true, generation: () => 1, knowledge: async () => [], now: () => now, fetchImpl: (async (_u: string, init: RequestInit) => { b2.push(String(init.body)); return new Response('{"choices":[{"message":{"content":"{\\"speak\\":null}"}}]}'); }) as unknown as typeof fetch, onAdvice: () => {} });
  c2.observe(s13(1235), [], true); now += 5000; c2.observe(dead(1240, { respawn_secs: 20 }), [], true); await flush();
  now += 6000; c2.observe(dead(1246, { screen: 'Shop', respawn_secs: 14 }), [], true); await flush(); assert.equal(b2.length, 0);
  now += 5000; c2.observe(dead(1251, { screen: 'Shop', respawn_secs: 9 }), [], true); await flush(); assert.equal(b2.length, 1, 'asked before the respawn');
  assert.ok(DEATH_PANEL_MAX_WAIT_MS >= 30000);
});

test('profile text is cut at sentence / item boundaries, never mid-word (rec8 "between the 10:12 deat")', () => {
  const rec8 = 'Player profile (4 matches, mostly Grey Talon, 15% dead, 1.9/10min deaths). Weak: Killed at close range: 6 of 7 deaths with a known cause came from melee range (melee hits x3, a hero Aura of Suffer...; Dies soon after respawning: 8 of 12 deaths within 90 s of respawning [improving]; Gets burst down: 15 of 15 deaths fell from healthy to dead in 15 s or less. Strong: latest match: Used 5 of 6 windows with 2+ enemies dead (20:41, 24:58, 31:06); latest match: Spent 2.3k souls between the 10:12 death and the respawn (3 of 7 deaths). Focus: Grey Talon is long-range: keep distance, leave on the first big hit, and stay behind your front line. Praise what went well as specifically as the fixes.';
  const p = profileForPrompt(rec8, () => false);
  assert.ok(p.length <= PROFILE_MAX_CHARS, String(p.length));
  assert.ok(/[.…]$/.test(p), p); assert.doesNotMatch(p, /deat$|Suffer\.\.\./);
  assert.match(p, /Killed at close range: 6 of 7 deaths with a known cause came from melee range; Dies soon/);
  assert.equal(clipAtBoundary('One two three. Four five six; seven eight nine', 30), 'One two three. Four five six.');
  assert.equal(clipAtBoundary('alpha beta gamma delta epsilon', 18), 'alpha beta gamma…');
  assert.equal(clipAtBoundary('short', 30), 'short');
  setProfileProvider(() => rec8); const live = profileText()!; setProfileProvider(undefined);
  assert.ok(live.length <= PROFILE_MAX_CHARS && !/deat$/.test(live), live);
  // The prompt budget cut is at a boundary too.
  const prompt = buildPrompt({ trigger: 'periodic', summary: 'S'.repeat(2990), profile: rec8, excerpts: [] }, SYSTEM_PROMPT.length + 3400);
  const prof = prompt.user.split('PLAYER PROFILE (earlier matches, no heroes of this match: coach a weakness only when the live situation matches)\n')[1]?.split('\n')[0] ?? '';
  assert.ok(!prof || /[.…]$/.test(prof), prof);
});

test('Rejuvenator state (rec13 30:13-33:00): we hold it until ~mm:ss, they hold it, it ran out; a revive spends a credit', () => {
  const h = new AiHistory(); let t = 0;
  const push = (s: AiSnapshot) => { h.push(s, [], t); t += 4000; };
  const at = (c: number, extra: Partial<AiSnapshot> = {}, up: boolean | null = false) => s13(c, { minimap: { ...snap(c).minimap!, midboss_up: up as boolean }, ...extra });
  push(at(1780, {}, true)); push(at(1792, {}, false)); push(at(1813, { midboss_taken_by: 'ally' }));
  const r = rejuvenatorState(h, 1864)!;
  assert.deepEqual([r.by, r.since, r.until, r.left], ['ally', 1792, 1978, 114], 'from the kill (icon gone at 29:52) + 6 s landing + 180 s');
  const line = (c: number) => buildSummary({ snapshot: at(c), history: h, now: t, trigger: 'periodic' });
  assert.match(line(1864), /Rejuvenator: WE hold it until about 32:58 \(1:54 left\) \(Midboss taken by our team at about 29:52; up to 3 revives in place\): use it now with all five on a standing structure; never spend a credit alone/);
  assert.match(line(1970), /Rejuvenator: ours runs out in about 8 s \(at 32:58/);
  assert.match(line(2054), /Rejuvenator: ours ran out at about 32:58/);
  assert.equal(rejuvenatorLine(h, 2200), null, 'old news after two minutes');
  // A revive read by the vision is a credit used.
  push(at(1900, { revived: true })); push(at(1904, { revived: true })); push(at(1908));
  assert.equal(rejuvenatorState(h, 1910)!.revives, 1);
  assert.match(rejuvenatorLine(h, 1910)!, /1 of up to 3 credits used/);
  // The enemy takes the next one (the Midboss came back up first): they hold it; the throw text follows.
  push(at(2300, {}, true)); push(at(2380, {}, false)); push(at(2398, { midboss_taken_by: 'enemy', ally_networth: 200000, enemy_networth: 180000 }));
  const s = buildSummary({ snapshot: at(2420, { ally_networth: 200000, enemy_networth: 180000 }), history: h, now: t, trigger: 'periodic' });
  assert.match(s, /Rejuvenator: THEY hold it until about 42:46 \(2:26 left\) \(Midboss taken by the enemy at about 39:40\)/);
  assert.match(s, /THROW RISK: big lead but they hold the Rejuvenator: no fights in their base until it runs out/);
  assert.equal(REJUVENATOR_SECS, 180);
});

test('mastery: prompts leave out mastered skills (profile weaknesses, angles, souls reminders) and pitch at the level', () => {
  const mastered = new Set(['positioning_vs_divers', 'shop_on_death', 'shop_in_base', 'dont_die_holding_souls', 'walk_out_with_team']);
  setSkillStateProvider(s => mastered.has(s) ? 'mastered' : 'working');
  try {
    const profile = 'Player profile (4 matches, mostly Grey Talon). Weak: Killed at close range: 6 of 7 deaths from melee range; Unspent souls: 4 of 7 deaths holding 3,200+; Pushes in with enemies missing: 3 of 7 deaths. Strong: latest match: Used 5 of 6 windows. Focus: Grey Talon is long-range: keep distance. Praise what went well.';
    const p = profileForPrompt(profile);
    assert.match(p, /Weak: Pushes in with enemies missing: 3 of 7 deaths\. Strong:/); assert.doesNotMatch(p, /close range|Unspent|Focus: Grey Talon/);
    assert.equal(profileLabelSkills('Dies soon after respawning').join(), 'walk_out_with_team');
    assert.equal(skillQuiet('shop_on_death'), true); assert.equal(skillQuiet('respect_missing_enemies'), false);
    // Coaching angles: no "walk out with a teammate" or "max range" reminder for mastered skills.
    const h = new AiHistory();
    h.push(snap(700, { hero: 'hero_orion' }), [], 0);
    h.push(snap(702, { hero: 'hero_orion', dead: true, screen: 'Dead', death_cause: 'Melee' }), [], 2000);
    h.push(snap(730, { hero: 'hero_orion', enemy_heroes: ['hero_shiv'] }), [], 30000);
    const angles = coachingAngles({ snapshot: snap(732, { hero: 'hero_orion', enemy_heroes: ['hero_shiv'] }), history: h, now: 32000, profile: null });
    assert.ok(!angles.some(a => /walk out with a teammate|hold max range/.test(a)), angles.join('\n'));
    // Souls: a mastered death-screen buy is not reminded below 1.5x the usual buy; a whole tier above is.
    const d = new AiHistory(); d.push(snap(1700, { souls: 4000 }), [], 0); d.push(snap(1702, { dead: true, screen: 'Dead', souls: 4000 }), [], 2000);
    const v = soulsView(snap(1702, { dead: true, screen: 'Dead', souls: 4000 }), d, 2000);
    assert.equal(v.allowed, false); assert.match(v.reason, /mastered skill/);
    assert.equal(soulsView(snap(1702, { dead: true, screen: 'Dead', souls: 9800 }), d, 2000).allowed, true);
    // The level text reaches the prompt whole (cut at a boundary).
    const lv = `Level: Advanced (6 basic and 4 intermediate skills mastered). Mastered, do not coach unless failed now: ${'Skill name; '.repeat(60)}end.`;
    const prompt = buildPrompt({ trigger: 'periodic', summary: 'S', level: lv });
    const block = prompt.user.split('pitch the advice at this level)\n')[1].split('\n')[0];
    assert.ok(block.length <= 600 && /[.…]$/.test(block), block);
  } finally { setSkillStateProvider(undefined); }
  assert.equal(skillQuiet('shop_on_death'), false, 'no tracker: nothing is quiet');
});

test('death prompt: teammates dead at the death and the walk-out from base (rec8 42:05), position unknown when not read', () => {
  const h = new AiHistory(); let t = 0;
  const push = (s: AiSnapshot) => { h.push(s, [], t); t += 4000; };
  const at = (c: number, y: number | null, alliesDead: number) => s13(c, { minimap: { ...snap(c).minimap!, you: y == null ? null : { x: 0.5, y } } }, [], alliesDead);
  push(at(2490, 0.92, 3)); push(at(2500, 0.9, 3)); push(at(2504, 0.78, 3)); push(at(2513, 0.72, 3));
  push(s13(2525, { dead: true, screen: 'Dead', hp: null, max_hp: null, minimap: null }, [], 3));
  const d = h.deaths.at(-1)!;
  assert.equal(beforeDeathLine(h, d), 'Before this death: teammates dead at the death (top bar): 3 of 5 · you left our base at 41:44 with 3 teammates dead, 21 s before the death · you were in our half');
  const s = buildSummary({ snapshot: s13(2532, { dead: true, screen: 'Dead', hp: null, max_hp: null }, [], 3), history: h, now: t, trigger: 'death' });
  assert.match(s, /Before this death: teammates dead at the death \(top bar\): 3 of 5 · you left our base at 41:44/);
  assert.match(TASKS.death, /teammates dead and a walk-out from base/);
  const blind = new AiHistory(); t = 0; const p = (x: AiSnapshot) => { blind.push(x, [], t); t += 4000; };
  p(at(2490, null, 0)); p(at(2510, null, 0)); p(s13(2525, { dead: true, screen: 'Dead', hp: null, max_hp: null, minimap: null }));
  assert.match(beforeDeathLine(blind, blind.deaths.at(-1)!)!, /your position was not read \(unknown\)$/);
});
