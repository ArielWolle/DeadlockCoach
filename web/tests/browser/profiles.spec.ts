import { test, expect, type Page } from '@playwright/test';

// The Matching lab (profile authoring) is gone; a stored non-Deadlock detection profile (an older
// draft, or a fixture like this one) still runs, and Settings › Diagnostics leads back to the default.
const colorProfile = {
  schema_version: 1, id: 'test-game', name: 'Test game · 4:3', adapter: 'generic',
  viewport: { aspect_ratio: null, max_width: 1600 },
  nodes: [
    { id: 'green', op: { type: 'color_ratio', rgb: [0, 200, 0], tolerance: 0 } },
    { id: 'percent', inputs: ['green'], op: { type: 'parse', parser: 'number', scale: 100 } },
    { id: 'stable', inputs: ['percent'], op: { type: 'confirm', frames: 2, tolerance: 1, max_age_ms: 20000 } },
  ], outputs: { 'player.fill_percent': 'stable' },
};
async function storedProfile(page: Page) {
  await page.addInitScript(profile => {
    if (!sessionStorage.getItem('profile-seeded')) { sessionStorage.setItem('profile-seeded', '1'); localStorage.setItem('coach-perception-profile-v1', JSON.stringify(profile)); }
    (window as unknown as { observations: unknown[] }).observations = [];
    window.addEventListener('coach:perception', e => (window as unknown as { observations: unknown[] }).observations.push((e as CustomEvent).detail));
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { value: async () => {
      const canvas = document.createElement('canvas'); canvas.width = 800; canvas.height = 600;
      const ctx = canvas.getContext('2d')!; const draw = () => { ctx.fillStyle = '#000'; ctx.fillRect(0, 0, 800, 600); ctx.fillStyle = 'rgb(0,200,0)'; ctx.fillRect(0, 0, 400, 600); };
      draw(); setInterval(draw, 200); return canvas.captureStream(5);
    } });
  }, colorProfile);
}

test('a stored generic profile accepts non-16:9 live sources and emits no Deadlock advice', async ({ page }) => {
  await storedProfile(page);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect.poll(() => page.evaluate(() => {
    const last = (window as unknown as { observations: Array<{ fields: Record<string, { value: unknown }> }> }).observations.at(-1);
    return last?.fields['player.fill_percent']?.value ?? null;
  }), { timeout: 30000 }).toBe(50);
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.locator('#cue-list')).toContainText('No calls yet');
  await expect(page.getByRole('link', { name: 'Live match' })).not.toBeVisible();
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
});

test('Settings › Diagnostics names a non-default detection profile and restores the Deadlock default', async ({ page }) => {
  await storedProfile(page);
  await page.goto('./#settings');
  await page.getByText('Diagnostics (for bug reports)').click();
  await expect(page.locator('#detection-profile')).toContainText('Detection profile: Test game · 4:3');
  await page.getByRole('button', { name: 'Use default detection' }).click();
  await expect(page.locator('#notice')).toContainText('Detection: Deadlock', { timeout: 110000 });
  await expect(page.locator('#detection-profile')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Live match' })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('coach-perception-profile-v1'))).toBe(null);
  await page.reload();
  await expect(page.getByRole('link', { name: 'Live match' })).toBeVisible();
});
