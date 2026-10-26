import { test, expect, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

async function mockSource(page: Page, borders: boolean) {
  await page.addInitScript(({ borders }) => {
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { configurable: true, value: async () => {
      const image = new Image(); image.src = '/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg'; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = 1920; canvas.height = 1200;
      const ctx = canvas.getContext('2d')!;
      let full = false;
      const draw = () => {
        ctx.fillStyle = borders ? '#000' : '#625544'; ctx.fillRect(0, 0, canvas.width, canvas.height);
        if (full) ctx.drawImage(image, 0, 0, 1600, 900);
        else if (borders) ctx.drawImage(image, 160, 150, 1600, 900);
        else ctx.drawImage(image, 0, 0, 1920, 1200);
      }; draw(); setInterval(draw, 200);
      const stream = canvas.captureStream(5);
      const state = window as unknown as { testStream: MediaStream; shares: number; resizeSource: () => void };
      state.testStream = stream; state.shares = (state.shares || 0) + 1;
      state.resizeSource = () => { full = true; canvas.width = 1600; canvas.height = 900; draw(); };
      return stream;
    } });
  }, { borders });
}

test('invalid viewport opens a live editor with mouse, keyboard and accessible cancellation', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1080 });
  await page.addInitScript(() => localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify({ autoCrop: false })));
  await mockSource(page, false);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  const dialog = page.getByRole('dialog', { name: 'Select your game viewport' });
  await expect(dialog).toBeVisible(); await expect(page.locator('#crop-summary')).toContainText('1920×1080');
  const frames = Number(await page.locator('#crop-source').getAttribute('data-frame'));
  await expect.poll(async () => Number(await page.locator('#crop-source').getAttribute('data-frame'))).toBeGreaterThan(frames);
  const box = page.locator('#crop-selection'), before = await box.boundingBox();
  const corner = page.getByRole('button', { name: 'Resize viewport from bottom right' });
  await corner.scrollIntoViewIfNeeded(); const handle = (await corner.boundingBox())!;
  await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2); await page.mouse.down();
  await page.mouse.move(handle.x - 140, handle.y - 80, { steps: 8 }); await page.mouse.up();
  expect((await box.boundingBox())!.width).toBeLessThan(before!.width - 80);
  await page.getByRole('button', { name: 'Move game viewport', exact: true }).focus();
  const previousLeft = await box.evaluate(el => parseFloat(el.style.left));
  await page.keyboard.press('Shift+ArrowRight');
  expect(await box.evaluate(el => parseFloat(el.style.left))).toBeGreaterThan(previousLeft);
  await page.getByRole('button', { name: 'Move viewport down', exact: true }).click();
  const violations = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(violations.violations).toEqual([]);
  await page.screenshot({ path: 'test-results/visual-crop-editor.png', fullPage: true });
  await page.keyboard.press('Escape'); await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { testStream: MediaStream }).testStream.getTracks().every(t => t.readyState === 'ended'))).toBe(true);
});

test('automatic top/side border cropping feeds correct HUD pixels and live edits reuse sharing', async ({ page }) => {
  await mockSource(page, true);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#metric-souls')).toHaveText('1,732', { timeout: 110000 });
  const crop = await page.evaluate(() => JSON.parse(localStorage.getItem('deadlock-coach-settings-v1')!).crop);
  expect(crop.x).toBeCloseTo(160 / 1920, 2); expect(crop.y).toBeCloseTo(150 / 1200, 2);
  expect(crop.w).toBeCloseTo(1600 / 1920, 2); expect(crop.h).toBeCloseTo(900 / 1200, 2);
  await page.getByRole('button', { name: 'Adjust viewport', exact: true }).click();
  const dialog = page.getByRole('dialog'); await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: 'Smaller', exact: true }).click();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('#session-state')).toHaveText('Coaching live');
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('deadlock-coach-settings-v1')!).crop)).toEqual(crop);
  await page.getByRole('button', { name: 'Adjust viewport', exact: true }).click();
  await page.getByRole('button', { name: 'Auto-detect borders' }).click();
  await expect(page.locator('#crop-editor-status')).toContainText('Black borders detected');
  await page.getByRole('button', { name: 'Apply viewport', exact: true }).click();
  await expect(dialog).not.toBeVisible(); await expect(page.locator('#session-state')).toHaveText('Coaching live');
  expect(await page.evaluate(() => (window as unknown as { shares: number }).shares)).toBe(1);
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
});

test('source resizing reruns automatic border detection and clears obsolete letterbox crop', async ({ page }) => {
  await mockSource(page, true);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#metric-souls')).toHaveText('1,732', { timeout: 110000 });
  await page.evaluate(() => (window as unknown as { resizeSource: () => void }).resizeSource());
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('deadlock-coach-settings-v1')!).crop), { timeout: 60000 })
    .toEqual({ x: 0, y: 0, w: 1, h: 1 });
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
});

test('letterboxed screenshot imports auto-crop without overwriting live capture preferences', async ({ page }) => {
  await page.goto('./#replay');
  await page.evaluate(async () => {
    const image = new Image(); image.src = '/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg'; await image.decode();
    const canvas = document.createElement('canvas'); canvas.width = 1600; canvas.height = 1000;
    const ctx = canvas.getContext('2d')!; ctx.fillStyle = '#000'; ctx.fillRect(0, 0, 1600, 1000); ctx.drawImage(image, 0, 50);
    const blob = await new Promise<Blob>(resolve => canvas.toBlob(b => resolve(b!), 'image/png'));
    const files = new DataTransfer(); files.items.add(new File([blob], 'frame-1.png', { type: 'image/png' }));
    files.items.add(new File([blob], 'frame-2.png', { type: 'image/png' }));
    (document.getElementById('replay-files') as HTMLInputElement).files = files.files;
  });
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toHaveText('Replay complete. Select a result to inspect it.', { timeout: 110000 });
  await expect(page.locator('#metric-souls')).toHaveText('1,732');
  await expect(page.locator('#metric-clock')).toHaveText('5:13');
  expect(await page.evaluate(() => localStorage.getItem('deadlock-coach-settings-v1'))).toBe(null);
});

test.describe('small-screen crop controls', () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });
  test('mobile editor fits the viewport and supports non-drag touch controls', async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify({ autoCrop: false })));
    await mockSource(page, false);
    await page.goto('./'); await page.getByRole('button', { name: 'Adjust viewport', exact: true }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await page.getByRole('button', { name: 'Smaller', exact: true }).tap();
    await page.getByRole('button', { name: 'Move viewport right', exact: true }).tap();
    await page.getByRole('button', { name: 'Auto-detect borders' }).tap();
    await expect(page.locator('#crop-editor-status')).toContainText('No reliable black borders');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: 'test-results/mobile-crop-editor.png', fullPage: true });
    await page.getByRole('button', { name: 'Cancel', exact: true }).tap();
    await expect(page.getByRole('dialog')).not.toBeVisible();
  });
});
