// Unit tests for the optional AI coach (imported by tests/unit.test.ts).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { AiCoach, AiHistory, MAX_INPUT_CHARS, SUMMARY_MAX_CHARS, SYSTEM_PROMPT, COACH_BRIEF, buildPrompt, buildRequest, buildSummary, callGateway, cleanSpeech,
  endpoint, extractText, heroMatchup, parseAdvice, retrieve, sections, situationTerms, testConnection, upcomingObjectives, setProfileProvider, profileText,
  type AiConfig, type AiSnapshot } from '../src/ai-coach.ts';
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
  assert.equal(defaults.aiEnabled, false); assert.equal(defaults.aiInterval, 40);
  const s = normalize({ aiEnabled: true, aiProvider: 'evil', aiInterval: 2, aiMaxTokens: 1e9, aiKey: '  k  ', options: { aiKey: 'x' } });
  assert.equal(s.aiProvider, 'openai'); assert.equal(s.aiInterval, 15); assert.equal(s.aiMaxTokens, 4096); assert.equal(s.aiKey, 'k');
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
  for (const part of ['clock 11:30 (midgame)', 'You: haze', 'HP 600/1,000 (60%)', '100→90→70→60', 'Unspent souls 3,400', 'held ≥2,550 for 90s', 'can afford a 3,200 item',
    'Area: Blue lane, 45% up lane · zone Blue jungle', 'Enemies: 1 alive, dead: Graves (22s), not seen recently: 1', 'Team souls: yours 20,000 vs enemy 18,000 (+2,000)',
    '1 enemies visible (1 near you', '1 missing (last seen 12s ago)', 'Midboss icon visible', 'rule: Back off to cover.']) assert.ok(a.includes(part), `${part}\n---\n${a}`);
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
  h.push(snap(702, { dead: true, screen: 'Dead', respawn_secs: 18, death_cause: 'Graves', death_damage: [{ source: 'Graves', damage: 620 }, { source: 'Shiv', damage: 300 }], death_killing_blow: { source: 'Graves', damage: 140 } }), [], 2000);
  h.push(snap(704, { dead: true, screen: 'Dead', respawn_secs: 16 }), [], 4000);
  assert.equal(h.deathCount, 1);
  const s = buildSummary({ snapshot: snap(704, { dead: true, screen: 'Dead' }), history: h, now: 4000, trigger: 'death' });
  assert.match(s, /YOU ARE DEAD/);
  assert.match(s, /Death at 11:42: cause Graves · damage taken: Graves 620, Shiv 300 · killing blow Graves \(140\) · last alive HP 15% · unspent before death 2,400 \(unsecured 600\) · respawn 16s/);
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
  assert.ok(p.user.includes('PLAYER PROFILE') && p.user.includes(COACH_BRIEF) && p.user.includes('REFERENCE EXCERPTS'));
  assert.ok(p.user.startsWith('TASK: The player pressed'));
  const small = buildPrompt({ trigger: 'death', summary: 'Trigger: death' });
  assert.ok(!small.user.includes('PLAYER PROFILE')); assert.ok(small.user.includes('just died'));
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
  assert.throws(() => extractText('openai', { error: { message: 'bad' } }), /choices/);
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
  assert.equal(calls, 1); assert.deepEqual(delivered, [{ key: 'ai:periodic:souls', priority: 'Normal', live: true }]);
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
  reply = () => new Response(JSON.stringify({ choices: [{ message: { content: '{"speak":"You died to Graves while holding 2,400 souls. Spend before you fight.","priority":"Normal","why":"death panel","focus":"death"}' } }] }));
  now += 60000; coach.observe(snap(780, { dead: true, screen: 'Dead' }), [], true); await flush(); const atDeath = calls;
  assert.equal(atDeath, before + 1, 'waits for the death panel');
  now += 500; coach.observe(snap(781, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [], true); await flush(); await flush();
  assert.equal(calls, atDeath + 1); assert.equal(delivered.at(-1)?.key, 'ai:death:death');
  assert.ok(bodies.at(-1)!.includes('just died') && bodies.at(-1)!.includes('cause Graves'));
  now += 3000; coach.observe(snap(784, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [], true); await flush(); assert.equal(calls, atDeath + 1);
  // Not live (replay/paused): no automatic requests; generation change drops a manual answer.
  live = false; now += 60000; coach.observe(snap(900), [], false); await flush(); assert.equal(calls, atDeath + 1);
  gate = new Promise(r => { release = r; }); void coach.ask('manual'); await flush(); generation++; release(); await flush(); await flush();
  assert.equal(delivered.at(-1)?.key, 'ai:death:death');
  // Disabled: nothing is sent.
  st = settings({ aiEnabled: false }); gate = Promise.resolve(); await coach.ask('manual'); assert.equal(coach.status.state, 'off');
});
