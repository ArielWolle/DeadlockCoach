import { test, expect, type Page, type Route } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

// Start-of-game pep talk (docs/ai-coach.md "Start-of-game pep talk"). A match start cannot be
// captured in a test: analyzed live frames are fed through window.__deadlockTestHooks.liveFrame
// (main.ts liveMatchFrame), coach calls through __deadlockTestHooks.coachSay (the speech gate).
test.use({ serviceWorkers: 'block' });
const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'authorization, content-type, x-api-key', 'access-control-allow-methods': 'POST, OPTIONS' };
const ENEMIES = ['hero_astro', 'hero_drifter', 'hero_inferno', 'hero_dynamo', 'hero_atlas', 'hero_necro'];
const ALLIES = ['hero_werewolf', 'hero_warden', 'hero_mirage', 'hero_doorman', 'hero_orion', 'hero_nano'];
const PEP = { spoken: 'Last game had real positives. Today, when two enemies are dead, we push together. Watch Drifter: stay next to a teammate. Let\'s win this one.',
  focus: 'Push every enemy-dead window.', matchup_tips: ['Drifter: stay next to a teammate'], plan: 'Farm Green from max range, then group.' };
const ai = { aiEnabled: true, aiProvider: 'openai', aiBaseUrl: 'https://gateway.test/v1', aiModel: 'mock-model', aiKey: 'sk-test-PREGAME', aiSpeak: true, aiInterval: 600, interval: 30 };
/** A rough earlier Violet match in the Player profile (7 deaths, two by Drifter, souls unspent). */
const PROFILE = { schema: 1, matches: [{ id: 'old', source: 'live', hero: 'hero_artist', startedAt: Date.now() - 7200_000, endedAt: Date.now() - 5200_000, startClock: 0, endClock: 1980, observedSecs: 1980, spanSecs: 1980, deadSecs: 340,
  time: { lane: 1100, jungle: 450, base: 90, dead: 340, unknown: 0 }, windows: { used: 0, unused: 3, unusedAt: [600, 900, 1200] },
  deaths: Array.from({ length: 7 }, (_, i) => ({ clock: 300 + i * 220, killer: i < 2 ? 'hero_drifter' : null, souls: 3200, deadSecs: 30, evidence: true })) }] };

async function setup(page: Page, settings: Record<string, unknown>, reply: (user: string) => { status: number; content: string } = () => ({ status: 200, content: JSON.stringify(PEP) }), profile: unknown = null) {
  await page.addInitScript(([s, p]) => {
    localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s));
    if (p) localStorage.setItem('deadlock-coach-player-profile-v1', JSON.stringify(p));
    (window as unknown as { __deadlockTestHooks: object }).__deadlockTestHooks = {};
    const w = window as unknown as { spoken: string[]; speechDelay?: number; pending?: SpeechSynthesisUtterance[] };
    w.spoken = [];
    Object.defineProperty(window, 'speechSynthesis', { configurable: true, value: {
      speak: (u: SpeechSynthesisUtterance) => { w.spoken.push(u.text); (w.pending ||= []).push(u); setTimeout(() => { w.pending = (w.pending || []).filter(x => x !== u); u.onend?.(new Event('end') as SpeechSynthesisEvent); }, w.speechDelay ?? 5); },
      cancel: () => { const ps = w.pending || []; w.pending = []; for (const u of ps) u.onerror?.({ error: 'canceled' } as SpeechSynthesisErrorEvent); }, getVoices: () => [], addEventListener: () => {}, removeEventListener: () => {}, onvoiceschanged: null,
    } });
  }, [settings, profile] as const);
  const calls: Array<{ system: string; user: string }> = [];
  await page.route('https://gateway.test/**', async (route: Route) => {
    const req = route.request();
    if (req.method() === 'OPTIONS') { await route.fulfill({ status: 204, headers: cors }); return; }
    const body = JSON.parse(req.postData() || '{}');
    const [system, user] = (body.messages || []).map((m: { content: string }) => String(m.content));
    if (!String(user).includes('PRE-GAME DIGEST')) { await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ choices: [{ message: { content: '{"speak": null, "priority": "Low", "why": "", "focus": "map"}' }, finish_reason: 'stop' }] }) }); return; }
    calls.push({ system, user });
    const r = reply(user);
    await route.fulfill({ status: r.status, headers: { ...cors, 'content-type': 'application/json' }, body: r.status === 200 ? JSON.stringify({ model: 'mock-pregame-model', choices: [{ message: { role: 'assistant', content: r.content }, finish_reason: 'stop' }] }) : 'gateway down' });
  });
  return calls;
}
/** A match from the countdown: clock 0-`to` s, a frame every 3 s, the roster read from 0:06. */
function start(wall0: number, to = 60, from = 0, enemies = ENEMIES) {
  const out: Array<{ s: Record<string, unknown>; wall: number }> = [{ wall: wall0 - 5000, s: { screen: 'Countdown' } }];
  for (let c = from; c <= to; c += 3) out.push({ wall: wall0 + (c - from) * 1000, s: { game_clock_secs: c, screen: 'Gameplay', hero: c >= 6 ? 'hero_orion' : null, souls: 0, hp: 1000, max_hp: 1000,
    ...(c >= 6 ? { enemy_heroes: enemies, ally_heroes: ALLIES } : {}) } });
  return out;
}
const feed = (page: Page, frames: Array<{ s: Record<string, unknown>; wall: number }>) => page.evaluate(fs => {
  const h = (window as unknown as { __deadlockTestHooks: { liveFrame: (s: unknown, w: number) => void } }).__deadlockTestHooks;
  for (const f of fs) h.liveFrame(f.s, f.wall);
}, frames);
const spoken = (page: Page) => page.evaluate(() => (window as unknown as { spoken: string[] }).spoken);

test('without the AI coach: the rule-based pep talk once per match, spoken, shown with Replay / Stop, kept across a reload', async ({ page }) => {
  const calls = await setup(page, { aiEnabled: false }, undefined, PROFILE);
  await page.goto('./');
  await expect(page.locator('#pregame-panel')).toBeVisible();
  const wall0 = Date.now();
  await feed(page, start(wall0, 30));
  await expect(page.locator('#pregame-state')).toHaveText('WAITING'); // the roster is read but it is only 0:30
  await feed(page, start(wall0, 60).slice(12));
  const text = page.locator('#pregame-text');
  await expect(text).toBeVisible();
  await expect(text).toContainText('Honest look at last game: 7 deaths');
  await expect(text).toContainText('Drifter caught you twice recently');
  await expect(page.locator('#pregame-meta')).toContainText('Rule coach template');
  await expect(page.locator('#pregame-details')).toContainText('Matchup');
  const words = (await text.textContent())!.split(/\s+/).length; expect(words).toBeLessThanOrEqual(120);
  await expect.poll(async () => (await spoken(page)).join(' ')).toContain("Let's");
  const said = await spoken(page);
  expect(said[0]).toMatch(/^Honest look at last game/); expect(said.join(' ')).not.toContain('Profile focus'); // the start focus cue is left to the pep talk
  await expect(page.locator('#cue-list')).toContainText('Honest look at last game');
  expect(calls.length).toBe(0);
  // Later frames of the same match: no second pep talk.
  const n = said.length;
  await feed(page, start(wall0 + 70_000, 120, 70).slice(1));
  await page.waitForTimeout(500); expect((await spoken(page)).length).toBe(n);
  // Replay, then Stop after the first sentence.
  await page.evaluate(() => { const w = window as unknown as { spoken: string[]; speechDelay: number }; w.spoken = []; w.speechDelay = 3000; });
  await page.getByRole('button', { name: 'Replay the pep talk' }).click();
  await expect(page.getByRole('button', { name: 'Stop the pep talk' })).toBeEnabled();
  await page.getByRole('button', { name: 'Stop the pep talk' }).click();
  await expect(page.getByRole('button', { name: 'Stop the pep talk' })).toBeDisabled();
  await page.waitForTimeout(3500); expect((await spoken(page)).length).toBe(1);
  const results = await new AxeBuilder({ page }).include('#pregame-panel').withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map(v => `${v.id}: ${v.nodes.map(x => x.target).join(', ')}`)).toEqual([]);
  // The last pep talk stays after a reload.
  await page.reload();
  await expect(page.locator('#pregame-text')).toContainText('Honest look at last game');
});

test('with the AI coach: a strict-JSON pep talk from the pre-game digest; a gateway failure falls back to the template', async ({ page }) => {
  let fail = false;
  const calls = await setup(page, ai, () => fail ? { status: 500, content: '' } : { status: 200, content: '```json\n' + JSON.stringify(PEP) + '\n```' });
  await page.goto('./');
  await feed(page, start(Date.now(), 60));
  await expect(page.locator('#pregame-text')).toHaveText(PEP.spoken, { timeout: 20000 });
  await expect(page.locator('#pregame-meta')).toContainText('AI coach (mock-pregame-model)');
  await expect(page.locator('#pregame-details')).toContainText('Push every enemy-dead window.');
  expect(calls.length).toBe(1);
  expect(calls[0].system).toContain('start-of-game pep talk'); expect(calls[0].system).toContain('"matchup_tips"');
  expect(calls[0].user).toContain('Your hero: Grey Talon'); expect(calls[0].user).toContain('Enemies: Holliday'); expect(calls[0].user).toContain('THREATS TO YOU');
  expect(calls[0].user).not.toContain('sk-test-PREGAME');
  await expect.poll(async () => (await spoken(page)).join(' ')).toContain("Let's win this one.");
  expect((await spoken(page))[0]).toBe('Last game had real positives.');
  // The next match (a new roster: the previous one counts as the vision's cache): the gateway fails, the template is spoken instead.
  fail = true;
  await feed(page, start(Date.now() + 3_000_000, 60, 0, ['hero_haze', 'hero_frank', 'hero_kelvin', 'hero_ghost', 'hero_bebop', 'hero_shiv']));
  await expect(page.locator('#pregame-meta')).toContainText('AI unavailable', { timeout: 20000 });
  await expect(page.locator('#pregame-text')).toContainText('First coached match');
  expect(calls.length).toBe(2);
});

test('a High safety call stops the pep talk at once; a Normal call waits until it ends', async ({ page }) => {
  await setup(page, { aiEnabled: false });
  await page.goto('./');
  await page.evaluate(() => { (window as unknown as { speechDelay: number }).speechDelay = 400; });
  await feed(page, start(Date.now(), 60));
  await expect.poll(async () => (await spoken(page)).length).toBeGreaterThanOrEqual(1);
  const say = (cue: Record<string, unknown>) => page.evaluate(c => (window as unknown as { __deadlockTestHooks: { coachSay: (c: unknown) => void } }).__deadlockTestHooks.coachSay(c), cue);
  await say({ key: 'lane:last_hit', text: 'Secure last hits and denies.', priority: 'Normal', cooldown_secs: 0 });
  await page.waitForTimeout(100);
  expect(await spoken(page)).not.toContain('Secure last hits and denies.'); // held
  await expect(page.getByRole('button', { name: 'Stop the pep talk' })).toBeEnabled();
  await say({ key: 'lane:critical', text: 'Health critical. Get out now.', priority: 'High', cooldown_secs: 0 });
  await expect(page.getByRole('button', { name: 'Stop the pep talk' })).toBeDisabled();
  const said = await spoken(page);
  expect(said.at(-1)).toBe('Health critical. Get out now.');
  await page.waitForTimeout(1500);
  const after = await spoken(page);
  expect(after.slice(said.length)).toEqual([]); // nothing of the pep talk after the High call, and the held call was dropped
  await expect(page.locator('#pregame-text')).toBeVisible(); // still shown
  // Replayed without a High call: the held Normal call is spoken right after the last sentence.
  await page.evaluate(() => { const w = window as unknown as { spoken: string[]; speechDelay: number }; w.spoken = []; w.speechDelay = 150; });
  await page.getByRole('button', { name: 'Replay the pep talk' }).click();
  await say({ key: 'lane:last_hit', text: 'Secure last hits and denies.', priority: 'Normal', cooldown_secs: 0 });
  expect(await spoken(page)).not.toContain('Secure last hits and denies.');
  await expect.poll(async () => (await spoken(page)).at(-1), { timeout: 10000 }).toBe('Secure last hits and denies.');
  const all = await spoken(page); expect(all.length).toBeGreaterThan(4); expect(all.at(-2)).toMatch(/[Ll]et's|game/);
});

test('a match joined mid-game (rec13 at 5:52) gets no pep talk; the setting turns it off', async ({ page }) => {
  const calls = await setup(page, ai);
  await page.goto('./');
  await feed(page, start(Date.now(), 380, 352).slice(1));
  await expect(page.locator('#pregame-status')).toContainText('Joined this match at 5:52: no pep talk');
  await expect(page.locator('#pregame-text')).toBeHidden();
  expect(calls.length).toBe(0);
  // Off in Settings (on by default): no pep talk for an early match either.
  await page.locator('[data-page="settings"]').click();
  await expect(page.locator('#pregame-enabled')).toBeChecked();
  await page.locator('#pregame-enabled').uncheck();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await page.locator('[data-page="coach"]').click();
  await feed(page, start(Date.now() + 3_000_000, 60));
  await page.waitForTimeout(1000);
  expect(calls.length).toBe(0); await expect(page.locator('#pregame-text')).toBeHidden();
  expect((await spoken(page)).length).toBe(0);
});
