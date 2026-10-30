import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFileSync } from 'node:fs';

function team(enemiesDead = 0) {
  return Array.from({ length: 12 }, (_, i) => ({ ally: i < 6, slot: i % 6, dead: i >= 6 && i - 6 < enemiesDead, bar_visible: true, health_ratio: 1,
    networth_observations: 0, observations: 1, position_observations: 0 }));
}
/** 13 minutes of Grey Talon on a lane with two deaths: alone with 4,000 souls to Victor's aura, then to melee. */
function match() {
  const lane = () => ({ push: .5, ally_troopers: 3, enemy_troopers: 3 });
  const frames: unknown[] = [];
  for (let c = 600; c < 1400; c += 4) {
    const rel = c - 600, dead = (rel >= 200 && rel < 240) || (rel >= 400 && rel < 432);
    const far = rel >= 180 && rel < 200;
    frames.push({ game_clock_secs: c, screen: dead ? 'Dead' : 'Gameplay', dead, hero: 'hero_orion', souls: rel < 200 ? 4000 : 2500,
      death_cause: dead ? (rel < 300 ? 'Aura of Suffering' : 'Melee') : null, hp: dead ? null : 900, max_hp: dead ? null : 1000, health_observations: 3,
      hero_status: team(),
      minimap: dead ? null : { observations: 3, you: { x: .3, y: .6 }, your_lane: 'Green', your_progress: .5, green: lane(), blue: lane(), yellow: lane(),
        midboss_up: false, allies: [], enemies: [], enemy_count: 0, heroes: [{ id: 1, ally: true, pos: far ? { x: .8, y: .2 } : { x: .33, y: .6 }, unseen_ms: 0, hits: 5 }] } });
  }
  return frames;
}

test('replay results build a persistent player profile with export, import, reset and no accessibility violations', async ({ page }) => {
  await page.goto('./#replay');
  await expect(page.locator('#replay-add-profile')).toBeDisabled();
  await page.locator('#replay-files').setInputFiles({ name: 'player-match.jsonl', mimeType: 'application/x-ndjson', buffer: Buffer.from(match().map(f => JSON.stringify(f)).join('\n')) });
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.', { timeout: 110000 });
  await page.locator('#replay-add-profile').click();
  await expect(page.locator('#notice')).toContainText('Added 1 match');
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#title-player')).toBeVisible();
  await expect(page.locator('#player-weaknesses')).toContainText('Killed at close range');
  await expect(page.locator('#player-weaknesses')).toContainText('Unspent souls');
  await expect(page.locator('#player-weaknesses')).toContainText('Grey Talon is long-range');
  // Strengths come first, with evidence, and the last match's plays are tagged as such.
  await expect(page.locator('#player-strengths')).toContainText('Kept your lane covered');
  await expect(page.locator('#player-strengths')).toContainText('Last match');
  await expect(page.locator('#player-strengths')).toContainText('Coach says: Keep it up');
  await expect(page.locator('#player-improved')).toContainText('Shown once there are two matches');
  expect(await page.locator('#player-strengths').evaluate(s => !!(s.compareDocumentPosition(document.getElementById('player-weaknesses')!) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
  await expect(page.locator('#player-matches tr')).toHaveCount(1);
  await expect(page.locator('#player-matches')).toContainText('Grey Talon');
  await expect(page.locator('#player-matches')).toContainText('Victor x1');
  // The match focus goal for the next match comes from this match's weaknesses (two close-range deaths holding souls).
  await expect(page.locator('#player-goal-next')).toContainText('Next match: No close-range deaths · Spend at the usual buy');
  await expect(page.locator('#player-goal-history')).toContainText('No scored goals yet');
  // The same replay is added only once.
  await page.locator('#player-add-replay').click();
  await expect(page.locator('#notice')).toContainText('Nothing added');
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations, results.violations.map(v => `${v.id}: ${v.nodes.map(n => n.target).join(', ')}`).join('; ')).toEqual([]);
  // Persists across a reload, in this browser only.
  await page.reload();
  await expect(page.locator('#player-weaknesses')).toContainText('Killed at close range');
  expect(await page.evaluate(() => (window as unknown as { deadlockPlayerProfile: { promptText: () => string } }).deadlockPlayerProfile.promptText())).toMatch(/^Player profile \(1 match, mostly Grey Talon/);
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: 'Export profile' }).click()]);
  const exported = readFileSync((await download.path())!, 'utf8');
  expect(JSON.parse(exported).matches).toHaveLength(1);
  page.once('dialog', d => void d.accept());
  await page.getByRole('button', { name: 'Reset profile' }).click();
  await expect(page.locator('#player-summary')).toContainText('No matches yet');
  await expect(page.locator('#player-weaknesses')).toContainText('No clear weaknesses');
  await page.reload();
  await expect(page.locator('#player-summary')).toContainText('No matches yet');
  await page.locator('#player-import-file').setInputFiles({ name: 'profile.json', mimeType: 'application/json', buffer: Buffer.from(exported) });
  await expect(page.locator('#notice')).toContainText('Imported 1 match');
  await expect(page.locator('#player-weaknesses')).toContainText('Killed at close range');
  // A match scored against its focus goal: the goal history and the matches table say met / missed.
  const scored = JSON.parse(exported); scored.matches[0] = { ...scored.matches[0], id: 'scored-1', startedAt: scored.matches[0].startedAt + 60000,
    goals: [{ id: 'close_range', text: 'No close-range deaths', met: false, detail: 'broken 2 times (13:20, 16:40) in 2 deaths' }, { id: 'respawn_deaths', text: 'No deaths within 60 s of a respawn', met: true, detail: '2 deaths, none broke it' }] };
  await page.locator('#player-import-file').setInputFiles({ name: 'scored.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(scored)) });
  await expect(page.locator('#notice')).toContainText('Imported 1 match');
  await expect(page.locator('#player-goal-history')).toContainText('1 of 2 goals met in the last 1 scored match.');
  await expect(page.locator('#player-goal-history li.missed')).toContainText('No close-range deaths (broken 2 times (13:20, 16:40) in 2 deaths)');
  await expect(page.locator('#player-goal-history li.met')).toContainText('No deaths within 60 s of a respawn');
  await expect(page.locator('#player-matches')).toContainText('Missed: No close-range deaths · Met: No deaths within 60 s of a respawn');
  const axe = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(axe.violations, axe.violations.map(v => v.id).join('; ')).toEqual([]);
});

test('the player profile setting persists and is on by default', async ({ page }) => {
  await page.goto('./#settings');
  const toggle = page.getByRole('checkbox', { name: /Player profile/ });
  await expect(toggle).toBeChecked();
  await toggle.uncheck(); await page.getByRole('button', { name: 'Save settings' }).click();
  await page.reload(); await expect(page.getByRole('checkbox', { name: /Player profile/ })).not.toBeChecked();
});
