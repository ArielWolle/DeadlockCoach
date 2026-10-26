import { test, expect, type Page } from '@playwright/test';
import { fileURLToPath } from 'node:url';
const fixture = fileURLToPath(new URL('../../public/assets/fixtures/00059_1792966931596.jpg', import.meta.url));

const colorProfile = {
  schema_version: 1, id: 'test-game', name: 'Test game · 4:3', adapter: 'generic',
  viewport: { aspect_ratio: null, max_width: 1600 },
  nodes: [
    { id: 'green', op: { type: 'color_ratio', rgb: [0, 200, 0], tolerance: 0 } },
    { id: 'percent', inputs: ['green'], op: { type: 'parse', parser: 'number', scale: 100 } },
    { id: 'stable', inputs: ['percent'], op: { type: 'confirm', frames: 2, tolerance: 1, max_age_ms: 20000 } },
  ], outputs: { 'player.fill_percent': 'stable' },
};
async function applyProfile(page: Page, profile = colorProfile) {
  await page.goto('./#pipeline');
  await page.getByLabel('Game profile JSON').fill(JSON.stringify(profile));
  await page.getByRole('button', { name: 'Apply profile', exact: true }).click();
  await expect(page.locator('#profile-status')).toContainText('Profile validated and active', { timeout: 110000 });
}
async function reference(page: Page, template = false) {
  await page.evaluate(async ({ template }) => {
    const canvas = document.createElement('canvas'); canvas.width = template ? 16 : 800; canvas.height = template ? 16 : 600;
    const ctx = canvas.getContext('2d')!;
    if (template) {
      const pixels = ctx.createImageData(16, 16);
      for (let y = 0; y < 16; y++) for (let x = 0; x < 16; x++) {
        const i = (y * 16 + x) * 4, v = (x * 37 + y * 71 + x * y * 13) % 251;
        pixels.data.set([v, v, v, 255], i);
      } ctx.putImageData(pixels, 0, 0);
    } else { ctx.fillStyle = '#000'; ctx.fillRect(0, 0, 800, 600); ctx.fillStyle = 'rgb(0,200,0)'; ctx.fillRect(0, 0, 400, 600); }
    const blob = await new Promise<Blob>(resolve => canvas.toBlob(b => resolve(b!), 'image/png'));
    const data = new DataTransfer(); data.items.add(new File([blob], 'sample.png', { type: 'image/png' }));
    const input = document.getElementById(template ? 'profile-template-file' : 'profile-reference') as HTMLInputElement;
    input.files = data.files; input.dispatchEvent(new Event('change', { bubbles: true }));
  }, { template });
  if (!template) await expect(page.locator('#profile-status')).toContainText('Reference image loaded');
}
async function run(page: Page) {
  await page.getByRole('button', { name: 'Run reference frame' }).click();
  await expect(page.locator('#profile-status')).toContainText('Reference frame processed');
}

test('generic 4:3 profile runs without neural/game assets and publishes exportable JSONL events', async ({ page }) => {
  const unwanted: string[] = [];
  page.on('request', req => { if (/\/models\/|templates\.bin/.test(req.url())) unwanted.push(req.url()); });
  await page.addInitScript(() => {
    (window as unknown as { observations: unknown[] }).observations = [];
    window.addEventListener('coach:perception', e => (window as unknown as { observations: unknown[] }).observations.push((e as CustomEvent).detail));
  });
  await applyProfile(page); await reference(page); await run(page);
  await expect(page.locator('#profile-readings')).toContainText('PENDING');
  await run(page); await expect(page.locator('#profile-readings')).toContainText('OK · 50');
  expect(await page.locator('#preview').evaluate(e => [(e as HTMLCanvasElement).width, (e as HTMLCanvasElement).height])).toEqual([800, 600]);
  expect(unwanted).toEqual([]);
  const events = await page.evaluate(() => (window as unknown as { observations: Array<{ fields: Record<string, { value: unknown; score: unknown }> }> }).observations);
  expect(events.at(-1)!.fields['player.fill_percent'].value).toBe(50);
  expect(events.at(-1)!.fields['player.fill_percent'].score).toBeNull();
  const downloading = page.waitForEvent('download'); await page.getByRole('button', { name: 'Export observations JSONL' }).click();
  const download = await downloading; const stream = await download.createReadStream(); const chunks: Buffer[] = [];
  for await (const chunk of stream!) chunks.push(Buffer.from(chunk));
  const records = Buffer.concat(chunks).toString().trim().split('\n').map(line => JSON.parse(line));
  expect(records).toHaveLength(2); expect(records[1].fields['player.fill_percent'].value).toBe(50);
  await page.reload(); await expect(page.locator('#profile-active')).toContainText('Test game · 4:3');
  await expect(page.getByRole('link', { name: 'Map intelligence' })).not.toBeVisible();
});

test('invalid graph leaves the active profile usable and its error is inspectable', async ({ page }) => {
  await applyProfile(page);
  const invalid = structuredClone(colorProfile); invalid.id = 'bad-profile';
  (invalid.nodes[0] as unknown as { inputs: string[] }).inputs = ['green'];
  await page.getByLabel('Game profile JSON').fill(JSON.stringify(invalid));
  await page.getByRole('button', { name: 'Apply profile', exact: true }).click();
  await expect(page.locator('#profile-status')).toContainText('cycle');
  await expect(page.locator('#profile-active')).toContainText('Test game · 4:3');
  await reference(page); await run(page); await run(page);
  await expect(page.locator('#profile-readings')).toContainText('OK · 50');
  await expect(page.locator('#engine-state')).toHaveText('LOCAL ENGINE READY');
});

test('generic OCR profile supports visual field regions, confirmation and stage diagnostics', async ({ page }) => {
  const gameAssets: string[] = []; page.on('request', r => { if (/templates\.bin/.test(r.url())) gameAssets.push(r.url()); });
  await page.goto('./#pipeline'); await page.getByLabel('Starter profile').selectOption('deadlock-hud-fields');
  await page.getByRole('button', { name: 'Load draft' }).click();
  await page.getByRole('button', { name: 'Apply profile', exact: true }).click();
  await expect(page.locator('#profile-status')).toContainText('Profile validated and active', { timeout: 110000 });
  await page.locator('#profile-reference').setInputFiles(fixture); await expect(page.locator('#profile-status')).toContainText('Reference image loaded');
  await page.getByRole('button', { name: 'Select region on reference' }).click();
  await expect(page.getByRole('dialog', { name: 'Select field region' })).toBeVisible();
  await expect(page.locator('#crop-summary')).toContainText('96×31');
  await page.getByRole('button', { name: 'Use this region' }).click();
  await page.getByRole('button', { name: 'Apply profile', exact: true }).click();
  await expect(page.locator('#profile-status')).toContainText('Profile validated and active');
  await run(page); await run(page);
  await expect(page.locator('#profile-readings')).toContainText('OK · 313');
  await expect(page.locator('#profile-readings')).toContainText('OK · 1732');
  await expect(page.locator('#profile-traces')).toContainText('clock_text');
  await expect(page.locator('#profile-traces')).toContainText('Unknown');
  expect(gameAssets).toEqual([]);
  await page.screenshot({ path: 'test-results/matching-lab.png', fullPage: true });
});

test('generic live capture accepts non-16:9 sources and emits no Deadlock advice', async ({ page }) => {
  await page.addInitScript(profile => {
    localStorage.setItem('coach-perception-profile-v1', JSON.stringify(profile));
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { value: async () => {
      const canvas = document.createElement('canvas'); canvas.width = 800; canvas.height = 600;
      const ctx = canvas.getContext('2d')!; const draw = () => { ctx.fillStyle = '#000'; ctx.fillRect(0, 0, 800, 600); ctx.fillStyle = 'rgb(0,200,0)'; ctx.fillRect(0, 0, 400, 600); };
      draw(); setInterval(draw, 200); return canvas.captureStream(5);
    } });
  }, colorProfile);
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#generic-readings')).toContainText('OK · 50', { timeout: 30000 });
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.locator('#cue-list')).toContainText('No calls yet');
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
});

test('template images become portable profile assets and can locate an on-screen anchor', async ({ page }) => {
  await applyProfile(page); await reference(page, true);
  await page.getByLabel('Template ID').fill('anchor');
  await page.getByRole('button', { name: 'Add searchable template' }).click();
  await expect(page.locator('#profile-status')).toContainText('Template and search stage added');
  const draft = JSON.parse(await page.getByLabel('Game profile JSON').inputValue());
  expect(draft.templates[0].png_base64).toBeTruthy();
  await page.getByRole('button', { name: 'Apply profile', exact: true }).click();
  await expect(page.locator('#profile-status')).toContainText('Profile validated and active');
  await page.evaluate(async () => {
    const draft = JSON.parse((document.getElementById('profile-json') as HTMLTextAreaElement).value);
    const icon = new Image(); icon.src = `data:image/png;base64,${draft.templates[0].png_base64}`; await icon.decode();
    const canvas = document.createElement('canvas'); canvas.width = 800; canvas.height = 600;
    const ctx = canvas.getContext('2d')!; ctx.fillStyle = '#222'; ctx.fillRect(0, 0, 800, 600); ctx.drawImage(icon, 32, 32);
    const blob = await new Promise<Blob>(resolve => canvas.toBlob(b => resolve(b!), 'image/png'));
    const data = new DataTransfer(); data.items.add(new File([blob], 'screen.png', { type: 'image/png' }));
    const input = document.getElementById('profile-reference') as HTMLInputElement; input.files = data.files; input.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await expect(page.locator('#profile-status')).toContainText('Reference image loaded'); await run(page);
  await expect(page.locator('#profile-readings')).toContainText('ui.anchor');
  await expect(page.locator('#profile-readings')).toContainText('OK · anchor');
});
