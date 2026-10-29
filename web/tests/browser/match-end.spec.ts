import { test, expect, type Page, type Route } from '@playwright/test';

// The end of a live match without a Victory / Defeat frame (rec13) starts the end-of-game review:
// loading and post-game screens after the last live frame, or Stop after 10+ minutes. A whole match
// cannot be captured in a test, so analyzed live frames are fed through window.__deadlockTestHooks
// (main.ts liveMatchFrame: the same Player profile + review path as a captured frame).
test.use({ serviceWorkers: 'block' });
const REVIEW = { summary: 'Strong Grey Talon game from range.', went_well: ['Long deathless stretches'], improvements: [{ title: 'Spend before fights', evidence: '13:20 died holding 4,000 souls', why_it_matters: 'Unspent souls do nothing', how_to_practice: 'Shop at every tier' }],
  keep_doing: ['Stay on your lane'], next_game_focus: 'Spend before every fight.', hero_tips: ['Shoot from max range'], spoken: 'Strong game. Spend before every fight.' };
const ai = { aiEnabled: true, aiProvider: 'openai', aiBaseUrl: 'https://gateway.test/v1', aiModel: 'mock-model', aiKey: 'sk-test-END', aiSpeak: false, aiReview: true, aiInterval: 600, interval: 30 };
const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'authorization, content-type, x-api-key', 'access-control-allow-methods': 'POST, OPTIONS' };

async function setup(page: Page, settings: Record<string, unknown>) {
  await page.addInitScript(s => {
    localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s));
    (window as unknown as { __deadlockTestHooks: object }).__deadlockTestHooks = {};
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { configurable: true, value: async () => {
      const image = new Image(); image.src = '/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg'; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = 1600; canvas.height = 900; const ctx = canvas.getContext('2d')!;
      const draw = () => ctx.drawImage(image, 0, 0); draw(); setInterval(draw, 100); return canvas.captureStream(10);
    } });
  }, settings);
  const reviews: string[] = [];
  await page.route('https://gateway.test/**', async (route: Route) => {
    const req = route.request();
    if (req.method() === 'OPTIONS') { await route.fulfill({ status: 204, headers: cors }); return; }
    const body = JSON.parse(req.postData() || '{}');
    const user = String(body.messages?.at(-1)?.content || '');
    // Only the end-of-game review is counted (the live AI coach waits 10 minutes between calls here).
    if (user.includes('MATCH DIGEST')) reviews.push(user);
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ model: 'mock-review-model', choices: [{ message: { role: 'assistant', content: JSON.stringify(REVIEW) }, finish_reason: 'stop' }] }) });
  });
  return reviews;
}
function team() { return Array.from({ length: 12 }, (_, i) => ({ ally: i < 6, slot: i % 6, dead: false, bar_visible: true, health_ratio: 1, networth_observations: 0, observations: 1, position_observations: 0 })); }
/** 13+ minutes of Grey Talon (clock 10:00-23:20, a frame every 4 s) with two deaths; wall time from `wall0`. */
function match(wall0: number) {
  const lane = () => ({ push: .5, ally_troopers: 3, enemy_troopers: 3 });
  const out: Array<{ s: Record<string, unknown>; wall: number }> = [];
  for (let c = 600; c <= 1400; c += 4) {
    const rel = c - 600, dead = (rel >= 200 && rel < 240) || (rel >= 400 && rel < 432);
    out.push({ wall: wall0 + rel * 1000, s: { game_clock_secs: c, screen: dead ? 'Dead' : 'Gameplay', dead, hero: 'hero_orion', souls: rel < 200 ? 4000 : 2500, respawn_secs: dead ? Math.max(1, (rel < 300 ? 240 : 432) - rel) : null,
      death_cause: dead ? 'Melee' : null, hp: dead ? null : 900, max_hp: dead ? null : 1000, health_observations: 3, ally_networth: 30000 + rel * 50, enemy_networth: 30000 + rel * 45, enemy_heroes: ['hero_frank'], hero_status: team(),
      minimap: dead ? null : { observations: 3, you: { x: .3, y: .6 }, your_lane: 'Green', your_progress: .5, green: lane(), blue: lane(), yellow: lane(), midboss_up: false, allies: [], enemies: [], enemy_count: 0, heroes: [] } } });
  }
  return out;
}
/** rec13's end: dead (respawn 9 -> 1), the scoreboard, a black frame, the MVP screens, the summary read as Shop. */
function rec13End(after: number) {
  const out: Array<{ s: Record<string, unknown>; wall: number }> = []; let w = after;
  const add = (ms: number, s: Record<string, unknown>) => { w += ms; out.push({ wall: w, s }); };
  add(4000, { game_clock_secs: 1404, screen: 'Dead', dead: true, hero: 'hero_orion', respawn_secs: 9 });
  add(8000, { game_clock_secs: 1412, screen: 'Dead', dead: true, hero: 'hero_orion', respawn_secs: 1 });
  add(5000, { game_clock_secs: 1417, screen: 'Scoreboard', dead: false, hero: 'hero_orion' });
  add(5000, { screen: 'Loading' });
  for (let i = 0; i < 4; i++) add(2500, { screen: 'Unknown' });
  for (let i = 0; i < 4; i++) add(4000, { game_clock_secs: 1434 + 4 * i, screen: 'Shop', dead: false, hero: 'hero_orion', souls: null });
  return out;
}
const feed = (page: Page, frames: Array<{ s: Record<string, unknown>; wall: number }>) => page.evaluate(fs => {
  const h = (window as unknown as { __deadlockTestHooks: { liveFrame: (s: unknown, w: number) => void } }).__deadlockTestHooks;
  for (const f of fs) h.liveFrame(f.s, f.wall);
}, frames);

test('a live match ending in loading and post-game screens (no Defeat frame) is reviewed once: Coach notice, Player page', async ({ page }) => {
  const reviews = await setup(page, ai);
  await page.goto('./');
  const m = match(Date.now() - 900_000);
  await feed(page, m);
  expect(reviews.length).toBe(0); // never during gameplay
  await feed(page, rec13End(m.at(-1)!.wall));
  await expect.poll(() => reviews.length, { timeout: 20000 }).toBe(1);
  expect(reviews[0]).toContain('Hero: Grey Talon'); expect(reviews[0]).toContain('DEATHS (3)'); // two in the match, the last one as it ended
  await expect(page.locator('#review-notice')).toContainText('Your end-of-game review is ready (Grey Talon', { timeout: 20000 });
  // More post-game screens, then the end-of-game camera at the final clock: no second review.
  await feed(page, [{ s: { screen: 'Unknown' }, wall: Date.now() + 1000 }, { s: { screen: 'PostGame' }, wall: Date.now() + 3000 }, { s: { game_clock_secs: 1420, screen: 'Gameplay', hero: 'hero_orion' }, wall: Date.now() + 5000 }]);
  await page.waitForTimeout(1500); expect(reviews.length).toBe(1);
  await page.locator('#review-notice a').click();
  await expect(page.locator('#review-list .review-item')).toHaveCount(1);
  await expect(page.locator('#review-list')).toContainText('Strong Grey Talon game from range.');
  await expect(page.locator('#player-matches')).toContainText('Grey Talon'); // the Player profile closed the match too
  // Asking again opens the stored review.
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('already reviewed'); expect(reviews.length).toBe(1);
});

test('Stop after 10+ minutes of a live match closes it and requests one review', async ({ page }) => {
  const reviews = await setup(page, ai);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#session-state')).toHaveText('Coaching live', { timeout: 110000 });
  // Paused: no captured frame interleaves with the fed match.
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await feed(page, match(Date.now() + 1000));
  expect(reviews.length).toBe(0);
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect.poll(() => reviews.length, { timeout: 20000 }).toBe(1);
  await expect(page.locator('#review-notice')).toContainText('Your end-of-game review is ready', { timeout: 20000 });
  await page.waitForTimeout(1000); expect(reviews.length).toBe(1);
});

test('with the AI coach off, the end of a match says why there is no review (Coach notice and Player page)', async ({ page }) => {
  const reviews = await setup(page, { ...ai, aiEnabled: false });
  await page.goto('./');
  const m = match(Date.now() - 900_000); await feed(page, m); await feed(page, rec13End(m.at(-1)!.wall));
  await expect(page.locator('#review-notice')).toContainText('No AI review: the AI coach is off');
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#review-list')).toContainText('The AI coach is off');
  expect(reviews.length).toBe(0);
});

test('Review my last match closes a live match the profile never ended (no end screen, capture not running)', async ({ page }) => {
  const reviews = await setup(page, ai);
  await page.goto('./');
  await feed(page, match(Date.now() - 900_000)); // the match just stops: no loading frames, no Stop
  expect(reviews.length).toBe(0);
  await page.locator('[data-page="player"]').click();
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('Match review ready (Grey Talon', { timeout: 20000 });
  expect(reviews.length).toBe(1);
  await expect(page.locator('#player-matches')).toContainText('Grey Talon');
});
