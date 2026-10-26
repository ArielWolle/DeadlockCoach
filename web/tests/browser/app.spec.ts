import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../../', import.meta.url));

test('dashboard navigation, mobile layout, knowledge search and keyboard landmarks', async ({ page }) => {
  await page.goto('./');
  await expect(page.getByRole('heading', { name: /Read the game/ })).toBeVisible();
  await page.keyboard.press('Tab'); await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused();
  await page.keyboard.press('Enter'); await expect(page.locator('#main')).toBeFocused();
  await page.getByRole('link', { name: 'Field guide' }).click();
  await page.getByLabel('Search the guide').fill('urn');
  await expect(page.locator('.guide-link').first()).toBeVisible();
  await page.locator('.guide-link').first().click();
  await expect(page.locator('#knowledge-body')).toContainText(/urn/i);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  await page.screenshot({ path: 'test-results/mobile-guide.png', fullPage: true });
});

test('settings persist, mute works and empty recording export is handled', async ({ page }) => {
  await page.goto('./#settings');
  await page.getByLabel('Statistical item suggestions').uncheck();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await page.reload(); await expect(page.getByLabel('Statistical item suggestions')).not.toBeChecked();
  await page.getByRole('button', { name: 'Mute voice' }).click();
  await expect(page.getByRole('button', { name: 'Unmute voice' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('link', { name: 'Replay lab' }).click();
  await page.getByRole('button', { name: 'Export snapshots', exact: true }).click();
  await expect(page.locator('#notice')).toHaveText('No recorded frames to export.');
});

test('real WASM initializes without isolation and replays recorded snapshots', async ({ page }) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  await page.goto('./#replay');
  expect(await page.evaluate(() => window.crossOriginIsolated)).toBe(false);
  await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.locator('#replay-status')).toContainText('Included match replay complete', { timeout: 110000 });
  await expect(page.locator('#engine-state')).toHaveText('LOCAL ENGINE READY');
  await expect(page.locator('.replay-result').first()).toContainText('19:13');
  await expect(page.locator('#cue-list')).toContainText(/Midboss|Green lane/);
  expect(errors).toEqual([]);
  await page.goto('./#map');
  await page.screenshot({ path: 'test-results/map-dashboard.png', fullPage: true });
});

test('offline shell and cached models can reopen and replay without an application server', async ({ page, context }) => {
  await page.goto('./#replay');
  await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.locator('#replay-status')).toContainText('Included match replay complete', { timeout: 110000 });
  await page.evaluate(async () => { await navigator.serviceWorker.ready; });
  await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker.controller)).toBe(true);
  await context.setOffline(true);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Replay lab' })).toBeVisible();
  await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.locator('#replay-status')).toContainText('Included match replay complete', { timeout: 110000 });
});

test('browser OCR matches native clock, souls and inventory on real screenshots', async ({ page }, testInfo) => {
  await page.goto('./#replay');
  const fixture = path.join(root, 'public/assets/fixtures/00059_1792966931596.jpg');
  await page.getByLabel('Bring your gameplay into the lab.').setInputFiles([fixture, fixture]);
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.', { timeout: 110000 });
  await expect(page.locator('#metric-clock')).toHaveText('5:13');
  await expect(page.locator('#metric-souls')).toHaveText('1,732');
  const snap = JSON.parse(await page.locator('#snapshot-json').textContent() || '{}');
  expect(snap.items).toContain('upgrade_pristine_emblem');
  expect(snap.screen).toBe('Gameplay');
  const timing = await page.locator('#analysis-time').textContent();
  console.info(`Browser OCR last recorded frame: ${timing}`);
  await testInfo.attach('browser-ocr-timing', { body: JSON.stringify({ timing, note: 'Headless recorded-frame timing, not game FPS or background latency' }), contentType: 'application/json' });
});

test('mocked browser capture processes real pixels, pauses, stops and releases the stream', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { configurable: true, value: async () => {
      const image = new Image(); image.src = '/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg'; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = 1600; canvas.height = 900;
      const ctx = canvas.getContext('2d')!;
      const draw = () => ctx.drawImage(image, 0, 0, 1600, 900); draw();
      const handle = setInterval(draw, 200);
      const stream = canvas.captureStream(5);
      (window as unknown as { testStream: MediaStream }).testStream = stream;
      stream.getVideoTracks()[0].addEventListener('ended', () => clearInterval(handle));
      return stream;
    } });
  });
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#metric-souls')).toHaveText('1,732', { timeout: 110000 });
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(page.locator('#session-state')).toHaveText('Coaching paused');
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { testStream: MediaStream }).testStream.getTracks().every(t => t.readyState === 'ended'))).toBe(true);
});

test.describe('cold network failure', () => {
test.use({ serviceWorkers: 'block' });
test('model failures are visible and can be retried rather than silently disabling analysis', async ({ page }) => {
  await page.route('**/assets/manifest.json', route => route.fulfill({ status: 503, body: 'Unavailable' }));
  await page.goto('./#replay'); await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.locator('#notice')).toContainText('HTTP 503');
  await expect(page.locator('#engine-state')).toHaveText('ENGINE ERROR');
  await expect(page.getByRole('button', { name: 'Analyze selected files' })).toBeEnabled();
});
});

test('canceling model initialization releases the mode and a later replay can recover', async ({ page }) => {
  await page.route('**/assets/models/text-detection.rten', async route => {
    await new Promise(resolve => setTimeout(resolve, 700));
    try { await route.continue(); } catch { /* worker may have been terminated */ }
  });
  await page.goto('./#replay');
  await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.getByRole('button', { name: 'Cancel replay' })).toBeEnabled();
  await page.getByRole('button', { name: 'Cancel replay' }).click();
  await expect(page.locator('#replay-status')).toHaveText('Replay canceled.');
  await expect(page.getByRole('button', { name: 'Load included match' })).toBeEnabled();
  await page.unroute('**/assets/models/text-detection.rten');
  await page.getByRole('button', { name: 'Load included match' }).click();
  await expect(page.locator('#replay-status')).toContainText('Included match replay complete', { timeout: 110000 });
});

test('recording archives export, import images and snapshots, and delete locally', async ({ page }, testInfo) => {
  await page.goto('./#replay');
  // Seed a real IndexedDB recording as if produced by the live loop, using a bundled frame.
  await page.evaluate(async () => {
    const image = await (await fetch('/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg')).blob();
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const r = indexedDB.open('deadlock-coach-recordings-v1', 1);
      r.onupgradeneeded = () => r.result.createObjectStore('frames', { keyPath: 'id', autoIncrement: true });
      r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error);
    });
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('frames', 'readwrite'); tx.objectStore('frames').add({ created: Date.now(), timestamp: Date.now(),
        snapshot: { game_clock_secs: 313, souls: 1732, screen: 'Gameplay' }, cues: [], image });
      tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error);
    }); db.close();
  });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect(page.locator('#recording-stats')).toContainText('1 frames');
  const downloading = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export frames + snapshots' }).click();
  const download = await downloading, exported = testInfo.outputPath('recording.json'); await download.saveAs(exported);
  await page.getByLabel('Bring your gameplay into the lab.').setInputFiles(exported);
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.', { timeout: 110000 });
  await expect(page.locator('.replay-result').first()).toContainText('Recording frame 1');
  await expect(page.locator('#metric-souls')).toHaveText('1,732');
  await page.getByRole('button', { name: 'Delete recordings' }).click();
  await expect(page.locator('#recording-stats')).toContainText('0 frames');
});

test('voice test invokes browser speech and mute cancels the queue', async ({ page }) => {
  await page.addInitScript(() => {
    const history: string[] = []; (window as unknown as { speechCalls: string[] }).speechCalls = history;
    Object.defineProperty(window, 'speechSynthesis', { configurable: true, value: {
      getVoices: () => [], addEventListener: () => {}, cancel: () => history.push('cancel'),
      speak: (utterance: SpeechSynthesisUtterance) => { history.push(utterance.text); utterance.onend?.(new Event('end') as SpeechSynthesisEvent); },
    } });
  });
  await page.goto('./'); await page.getByRole('button', { name: 'Test voice' }).click();
  expect(await page.evaluate(() => (window as unknown as { speechCalls: string[] }).speechCalls)).toContain('Coach audio test. Your next move starts here.');
  await page.getByRole('button', { name: 'Mute voice' }).click();
  expect(await page.evaluate(() => (window as unknown as { speechCalls: string[] }).speechCalls)).toContain('cancel');
});

test('all dashboard routes pass automated accessibility checks', async ({ page }) => {
  await page.goto('./');
  for (const route of ['coach', 'map', 'match', 'replay', 'knowledge', 'settings']) {
    await page.goto(`./#${route}`);
    const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
    expect(results.violations, `${route}: ${results.violations.map(v => `${v.id}: ${v.nodes.map(n => n.target).join(', ')}`).join('; ')}`).toEqual([]);
  }
});
