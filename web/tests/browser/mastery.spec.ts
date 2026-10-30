import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFileSync } from 'node:fs';

// Skills and mastery on the Player page (src/mastery.ts, player-page.ts skills()).
const stored = {
  schema: 1, matchIds: ['m2', 'm1'],
  skills: {
    shop_on_death: { performed: 5, prompted: 1, failed: 0, lastSeen: 1, mastered: true, masteredAt: 1, manual: null, history: [{ id: 'm2', ok: 3, prompted: 1, fail: 0 }, { id: 'm1', ok: 2, prompted: 0, fail: 0 }] },
    urn_play: { performed: 1, prompted: 0, failed: 1, lastSeen: 1, mastered: false, masteredAt: null, manual: null, history: [{ id: 'm2', ok: 1, prompted: 0, fail: 1 }] },
  },
};

test('the Player page shows your level and the mastered / working on / not yet seen skills, with manual marks that persist', async ({ page }) => {
  await page.addInitScript(s => { if (!localStorage.getItem('deadlock-coach-skill-mastery-v1')) localStorage.setItem('deadlock-coach-skill-mastery-v1', s); }, JSON.stringify(stored));
  await page.goto('./');
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#player-level')).toHaveText('Your level: Basic');
  await expect(page.locator('#skills-mastered')).toContainText('Spending souls on the death screen');
  await expect(page.locator('#skills-mastered')).toContainText('Last 3 matches: 5 on your own, 1 after a reminder');
  await expect(page.locator('#skills-working')).toContainText('Soul Urn plays');
  await expect(page.locator('#skills-working')).toContainText('1 missed');
  await expect(page.locator('#skills-unseen')).toContainText('Leaving base at full health');
  const results = await new AxeBuilder({ page }).include('#page-player').withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations, results.violations.map(v => `${v.id}: ${v.nodes.map(n => n.target).join(', ')}`).join('; ')).toEqual([]);
  // "Coach me on this" moves a mastered skill back to Working on; "I have this" marks one mastered.
  await page.getByRole('button', { name: 'Coach me on this: Spending souls on the death screen' }).click();
  await expect(page.locator('#skills-working')).toContainText('Spending souls on the death screen');
  await expect(page.locator('#skills-working li[data-skill="shop_on_death"]')).toContainText('Coach me');
  await page.getByRole('button', { name: 'I have this: Leaving base at full health' }).click();
  await expect(page.locator('#skills-mastered li[data-skill="heal_before_leaving"]')).toContainText('Marked by you');
  await page.reload();
  await page.locator('[data-page="player"]').click();
  await expect(page.locator('#skills-mastered li[data-skill="heal_before_leaving"]')).toBeVisible();
  await expect(page.locator('#skills-working li[data-skill="shop_on_death"]')).toBeVisible();
  // Exported with the profile, under "mastery".
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name: 'Export profile' }).click()]);
  const exported = JSON.parse(readFileSync((await download.path())!, 'utf8'));
  expect(exported.mastery.skills.heal_before_leaving.manual).toBe('mastered');
  expect(exported.mastery.skills.shop_on_death.manual).toBe('learning');
  // "Let the coach decide" clears a mark.
  await page.locator('#skills-mastered li[data-skill="heal_before_leaving"]').getByRole('button', { name: 'Let the coach decide' }).click();
  await expect(page.locator('#skills-unseen li[data-skill="heal_before_leaving"]')).toBeVisible();
});
