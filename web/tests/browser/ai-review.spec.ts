import { test, expect, type Page, type Route } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFile } from 'node:fs/promises';

// End-of-game AI review (docs/ai-coach.md "End-of-game review"): the gateway is mocked with page.route.
test.use({ serviceWorkers: 'block' });
const KEY = 'sk-test-REVIEW-SECRET-77';
const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'authorization, content-type, x-api-key', 'access-control-allow-methods': 'POST, OPTIONS' };
const REVIEW = {
  summary: 'A much cleaner Grey Talon game: two deaths and you kept farming the lane.',
  went_well: ['Lane presence: you stayed on Green for most of the game', 'You never died within 40 seconds of a respawn'],
  improvements: [
    { title: 'Stop dying to Victor up close', evidence: '13:20 Aura of Suffering while alone', why_it_matters: 'Each death gives Victor souls and you lose 30+ seconds of map time', how_to_practice: 'Keep max range; step back the moment the aura ticks' },
    { title: 'Spend before you walk out', evidence: '13:20 you died holding 4,000 souls', why_it_matters: 'Unspent souls do nothing in a fight', how_to_practice: 'Shop whenever you hold the next tier' },
  ],
  keep_doing: ['Stay on your lane between fights: it kept your farm ahead'],
  next_game_focus: 'Spend your souls before every fight.',
  hero_tips: ['As Grey Talon, shoot from max range behind your front line'],
  spoken: 'This was a much cleaner game. You stayed on your lane and never threw a life right after respawning. The biggest fix is Victor: you died to his aura up close at 13:20. Stay at max range. Next game, spend your souls before every fight.',
};
function team() {
  return Array.from({ length: 12 }, (_, i) => ({ ally: i < 6, slot: i % 6, dead: false, bar_visible: true, health_ratio: 1, networth_observations: 0, observations: 1, position_observations: 0 }));
}
/** 13 minutes of Grey Talon on a lane with two deaths (as tests/browser/player.spec.ts). */
function frames() {
  const lane = () => ({ push: .5, ally_troopers: 3, enemy_troopers: 3 });
  const out: unknown[] = [];
  for (let c = 600; c < 1400; c += 4) {
    const rel = c - 600, dead = (rel >= 200 && rel < 240) || (rel >= 400 && rel < 432);
    out.push({ game_clock_secs: c, screen: dead ? 'Dead' : 'Gameplay', dead, hero: 'hero_orion', souls: rel < 200 ? 4000 : 2500,
      death_cause: dead ? (rel < 300 ? 'Aura of Suffering' : 'Melee') : null, hp: dead ? null : 900, max_hp: dead ? null : 1000, health_observations: 3,
      ally_networth: 30000 + rel * 50, enemy_networth: 30000 + rel * 45, enemy_heroes: ['hero_frank'], hero_status: team(),
      minimap: dead ? null : { observations: 3, you: { x: .3, y: .6 }, your_lane: 'Green', your_progress: .5, green: lane(), blue: lane(), yellow: lane(),
        midboss_up: false, allies: [], enemies: [], enemy_count: 0, heroes: [{ id: 1, ally: true, pos: { x: .33, y: .6 }, unseen_ms: 0, hits: 5 }] } });
  }
  return out;
}
async function setup(page: Page, settings: Record<string, unknown>, content: () => string) {
  await page.addInitScript(s => {
    localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s));
    const w = window as unknown as { spoken: string[]; speechDelay?: number; pending?: SpeechSynthesisUtterance[] };
    w.spoken = [];
    Object.defineProperty(window, 'speechSynthesis', { configurable: true, value: {
      speak: (u: SpeechSynthesisUtterance) => { w.spoken.push(u.text); (w.pending ||= []).push(u); setTimeout(() => u.onend?.(new Event('end') as SpeechSynthesisEvent), w.speechDelay ?? 5); },
      cancel: () => { for (const u of w.pending || []) u.onerror?.({ error: 'canceled' } as SpeechSynthesisErrorEvent); w.pending = []; }, getVoices: () => [], addEventListener: () => {}, removeEventListener: () => {}, onvoiceschanged: null,
    } });
  }, settings);
  const seen: Array<{ body: Record<string, any> }> = []; // eslint-disable-line @typescript-eslint/no-explicit-any
  await page.route('https://gateway.test/**', async (route: Route) => {
    const req = route.request();
    if (req.method() === 'OPTIONS') { await route.fulfill({ status: 204, headers: cors }); return; }
    seen.push({ body: JSON.parse(req.postData() || '{}') });
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ model: 'mock-review-model', choices: [{ message: { role: 'assistant', content: content() }, finish_reason: 'stop' }] }) });
  });
  return seen;
}
async function replay(page: Page) {
  await page.goto('./#replay');
  await page.locator('#replay-files').setInputFiles({ name: 'review-match.jsonl', mimeType: 'application/x-ndjson', buffer: Buffer.from(frames().map(f => JSON.stringify(f)).join('\n')) });
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.', { timeout: 110000 });
}
const ai = { aiEnabled: true, aiProvider: 'openai', aiBaseUrl: 'https://gateway.test/v1', aiModel: 'mock-model', aiKey: KEY, aiSpeak: true, aiReview: true };

test('Review my last match after a replay: renders the review, reads it aloud with Stop, stores it once per match, exports it', async ({ page }) => {
  const logs: string[] = []; page.on('console', m => logs.push(m.text())); page.on('pageerror', e => logs.push(e.message));
  const seen = await setup(page, ai, () => '```json\n' + JSON.stringify(REVIEW) + '\n```');
  await replay(page);
  expect(seen.length).toBe(0); // replays are never reviewed automatically
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#review-list')).toContainText('No match reviews yet');
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('Match review ready (Grey Talon', { timeout: 20000 });
  // The prompt: the review persona, the digest of this match and the review token budget.
  expect(seen.length).toBe(1);
  const [system, user] = seen[0].body.messages.map((m: { content: string }) => m.content);
  expect(system).toContain('elite Deadlock coach'); expect(user).toContain('MATCH DIGEST'); expect(user).toContain('Hero: Grey Talon');
  expect(user).toContain('DEATHS (2)'); expect(user).toContain('Aura of Suffering');
  // What went well leads the digest, and the prompt asks to open with it and end with keep doing + next focus.
  expect(user).toMatch(/STRENGTHS \(\d+: what went well, with evidence\)\n- /); expect(user.indexOf('STRENGTHS')).toBeLessThan(user.indexOf('DEATHS ('));
  expect(system).toContain('Lead with what went well'); expect(system).toContain('"keep_doing"');
  expect(seen[0].body.max_tokens).toBeGreaterThanOrEqual(3072);
  // Rendered: summary, what went well, improvements with evidence, next-game focus, hero tips.
  const item = page.locator('#review-list .review-item').first();
  await expect(item).toContainText(REVIEW.summary);
  await expect(item).toContainText('What went well'); await expect(item).toContainText('Lane presence');
  await expect(item).toContainText('Stop dying to Victor up close'); await expect(item).toContainText('Evidence: 13:20 Aura of Suffering');
  await expect(item).toContainText('Keep doing'); await expect(item).toContainText('Stay on your lane between fights');
  await expect(item).toContainText('Next game: Spend your souls before every fight.');
  await expect(item).toContainText('As Grey Talon, shoot from max range');
  await expect(item.locator('summary').first()).toContainText('Grey Talon');
  // Spoken automatically (Speak AI advice is on), sentence by sentence.
  await expect.poll(() => page.evaluate(() => (window as unknown as { spoken: string[] }).spoken.length)).toBeGreaterThanOrEqual(5);
  const spoken = await page.evaluate(() => (window as unknown as { spoken: string[] }).spoken);
  expect(spoken[0]).toBe('This was a much cleaner game.'); expect(spoken.join(' ')).toContain('Next game, spend your souls before every fight.');
  // Speak again, then Stop.
  await page.evaluate(() => { const w = window as unknown as { spoken: string[]; speechDelay: number }; w.spoken = []; w.speechDelay = 3000; });
  await item.getByRole('button', { name: /^Speak the review/ }).click();
  await expect(item.getByRole('button', { name: /^Stop reading/ })).toBeEnabled();
  await item.getByRole('button', { name: /^Stop reading/ }).click();
  await expect(item.getByRole('button', { name: /^Speak the review/ })).toBeEnabled();
  await page.waitForTimeout(3500);
  expect(await page.evaluate(() => (window as unknown as { spoken: string[] }).spoken.length)).toBe(1); // stopped after the first sentence
  // Coach page notice.
  await page.locator('[data-page="coach"]').click();
  await expect(page.locator('#review-notice')).toContainText('Your end-of-game review is ready (Grey Talon');
  // One review per match: asking again opens the stored one without a second request.
  await page.locator('[data-page="player"]').click();
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('already reviewed');
  expect(seen.length).toBe(1);
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map(v => `${v.id}: ${v.nodes.map(n => n.target).join(', ')}`)).toEqual([]);
  // Export, and stored across a reload; the key is never stored with the review.
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export reviews' }).click();
  const exported = JSON.parse(await readFile(await (await download).path(), 'utf8'));
  expect(exported.reviews).toHaveLength(1); expect(exported.reviews[0].review.summary).toBe(REVIEW.summary);
  expect(JSON.stringify(exported)).not.toContain(KEY);
  await page.reload();
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#review-list')).toContainText('Grey Talon');
  expect(logs.join('\n')).not.toContain(KEY);
});

test('a reply that is not JSON is shown as raw text', async ({ page }) => {
  const seen = await setup(page, ai, () => 'You played well. Keep it up.');
  await replay(page);
  await page.locator('[data-page="player"]').click();
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('raw reply is shown below', { timeout: 20000 });
  await expect(page.locator('#review-list pre')).toHaveText('You played well. Keep it up.');
  expect(seen.length).toBe(1);
});
test('with the AI coach off, Review my last match explains instead of sending; the setting is on by default', async ({ page }) => {
  const seen = await setup(page, { ...ai, aiEnabled: false, aiReview: undefined }, () => JSON.stringify(REVIEW));
  await replay(page);
  await page.locator('[data-page="player"]').click();
  await page.getByRole('button', { name: 'Review my last match' }).click();
  await expect(page.locator('#review-status')).toContainText('Turn on the AI coach in Settings');
  expect(seen.length).toBe(0);
  await page.locator('[data-page="settings"]').click();
  await expect(page.locator('#ai-review')).toBeChecked();
});