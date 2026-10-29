// Unit tests for the optional AI coach (imported by tests/unit.test.ts).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { AiCoach, AiHistory, MAX_INPUT_CHARS, SUMMARY_MAX_CHARS, SYSTEM_PROMPT, COACH_BRIEF, buildPrompt, buildRequest, buildSummary, callGateway, cleanSpeech,
  endpoint, extractText, heroMatchup, parseAdvice, retrieve, sections, situationTerms, testConnection, upcomingObjectives, setProfileProvider, profileText,
  checkAdvice, coachingAngles, overlap, soulsView, topicOf, TASKS, aiExchanges, clearAiExchanges, onAiExchange,
  type Advice, type AiConfig, type AiSnapshot } from '../src/ai-coach.ts';
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
  for (const part of ['clock 11:30 (midgame)', 'You: haze', 'HP 600/1,000 (60%)', '100→90→70→60', 'Unspent souls 3,400', 'held ≥2,550 for 90s', 'can afford the usual 3,200 buy for 11:30 · Souls advice: allowed',
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
  assert.ok(p.user.includes('PLAYER PROFILE') && p.user.includes(COACH_BRIEF) && p.user.includes('REFERENCE EXCERPTS'));
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
  assert.equal(calls, atDeath + 1); assert.equal(delivered.at(-1)?.key, 'ai:death:death');
  assert.ok(bodies.at(-1)!.includes('Rule death review (already spoken): Why you died: Graves did 620'));
  assert.ok(bodies.at(-1)!.includes('just died') && bodies.at(-1)!.includes('cause Graves'));
  now += 3000; coach.observe(snap(784, { dead: true, screen: 'Dead', death_cause: 'Graves' }), [], true); await flush(); assert.equal(calls, atDeath + 1);
  // Not live (replay/paused): no automatic requests; generation change drops a manual answer.
  live = false; now += 60000; coach.observe(snap(900), [], false); await flush(); assert.equal(calls, atDeath + 1);
  gate = new Promise(r => { release = r; }); void coach.ask('manual'); await flush(); generation++; release(); await flush(); await flush();
  assert.equal(delivered.at(-1)?.key, 'ai:death:death');
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
  assert.ok(!r1.ok && /same focus \(economy\) as the AI call at 33:20/.test(r1.reason), JSON.stringify(r1));
  assert.ok(checkAdvice(repeat, 'manual', v, h, 100000).ok, 'the player asked: no gating');
  const r2 = checkAdvice(a('Yamato is alive: hold max range behind Abrams.', 'positioning'), 'periodic', v, h, 100000);
  assert.ok(r2.ok && r2.topic === 'positioning' && r2.speak === 'Yamato is alive: hold max range behind Abrams.');
  const r3 = checkAdvice(a('Your health is low: back behind cover before trading.', 'health'), 'periodic', v, h, 100000);
  assert.ok(!r3.ok && /repeats the rule call/.test(r3.reason), JSON.stringify(r3));
  // After the 3-minute focus window a near-identical text is still a repeat (5-minute window).
  h.push(snap(2200, { souls: 7200 }), [], 200000);
  const r4 = checkAdvice(a('Spend your 7,200 souls now at the nearest shop on a 6,400 item.', 'general'), 'periodic', soulsView(snap(2200, { souls: 7200 }), h, 200000), h, 200000);
  assert.ok(!r4.ok && /near-duplicate of the AI call at 33:20/.test(r4.reason), JSON.stringify(r4));
  // Same focus again is allowed only when urgent (High, not souls).
  h.noteAdvice({ t: 200000, clock: 2200, trigger: 'periodic', topic: 'positioning', text: 'Hold max range behind Abrams while Yamato is up.' });
  h.push(snap(2230), [], 230000);
  const v2 = soulsView(snap(2230), h, 230000);
  assert.equal(checkAdvice(a('Shiv is diving you now: dash back to Bebop.', 'range'), 'periodic', v2, h, 230000).ok, false);
  assert.equal(checkAdvice(a('Shiv is diving you now: dash back to Bebop.', 'range', 'High'), 'periodic', v2, h, 230000).ok, true);
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
    'Rule death review (already spoken): Why you died: Yamato did 35%']) assert.ok(summary.includes(part), `${part}\n---\n${summary}`);
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
  assert.match(angles[2], /^- 2 enemies dead \(first back in 22s\) → group up and hit a Guardian/);
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
  assert.match(JSON.parse(bodies.at(-1)!).messages[1].content, /RECENT AI ADVICE \(yours; do not reuse a focus within 3 min\):\n- \[40:00\] economy: Spend your souls now/);
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
  await assert.rejects(callGateway(cfg, 's', 'u', { fetchImpl: cut }), (e: Error & { kind?: string }) => e.kind === 'truncated' && /1024-token limit/.test(e.message));
  const sent: number[] = [];
  const later = (async (_u: string, init: RequestInit) => { const body = JSON.parse(String(init.body)); sent.push(body.max_tokens);
    return new Response(JSON.stringify(sent.length === 1 ? { choices: [{ message: { content: '{"speak":"Hold' }, finish_reason: 'length' }] }
      : { choices: [{ message: { content: '{"speak":"Hold max range behind your front line.","priority":"Normal","why":"melee diver near","focus":"positioning"}' }, finish_reason: 'stop' }] }), { status: 200 }); }) as unknown as typeof fetch;
  const settings = normalize({ ...defaults, aiEnabled: true, aiBaseUrl: cfg.baseUrl, aiModel: cfg.model, aiKey: 'k' } as Settings);
  const spoken: string[] = [];
  const coach = new AiCoach({ settings: () => settings, isLive: () => true, generation: () => 1, knowledge: async () => [], name: (x: string) => x,
    fetchImpl: later, pageProtocol: () => 'https:', onAdvice: (c: { text: string }) => spoken.push(c.text) } as never);
  await coach.ask('manual');
  assert.deepEqual(sent, [1024, 2048]); assert.equal(spoken[0], 'Hold max range behind your front line.');
});