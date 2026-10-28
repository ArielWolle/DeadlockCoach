import { test, expect, type Page, type Route } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// The gateway is mocked with page.route; block the service worker so every request is routable.
test.use({ serviceWorkers: 'block' });
const KEY = 'sk-test-PLAYWRIGHT-SECRET-4242';
const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'authorization, content-type, x-api-key, anthropic-version, anthropic-dangerous-direct-browser-access', 'access-control-allow-methods': 'POST, OPTIONS' };
const ADVICE = 'Spend your 3,400 souls now. Buy before Graves catches you in Blue.';
// Automatic advice passes the repeat/souls gate (checkAdvice), so the live test uses a non-souls call.
const LIVE_ADVICE = 'Walk out with a teammate and hold max range behind your front line.';

function settings(provider: 'openai' | 'anthropic') {
  return { aiEnabled: true, aiProvider: provider, aiBaseUrl: provider === 'openai' ? 'https://gateway.test/v1' : 'https://anthropic.test', aiModel: 'mock-model',
    aiKey: KEY, aiSpeak: true, record: true };
}
async function mockGateway(page: Page, host: string, reply: (body: Record<string, unknown>) => unknown, status = 200) {
  const seen: Array<{ headers: Record<string, string>; body: string }> = [];
  await page.route(`https://${host}/**`, async (route: Route) => {
    const req = route.request();
    if (req.method() === 'OPTIONS') { await route.fulfill({ status: 204, headers: cors }); return; }
    seen.push({ headers: req.headers(), body: req.postData() || '' });
    await route.fulfill({ status, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify(reply(JSON.parse(req.postData() || '{}'))) });
  });
  return seen;
}
async function replay(page: Page) {
  await page.goto('./#replay');
  const frames = [600, 605].map(game_clock_secs => ({ game_clock_secs, screen: 'Gameplay', hero: 'hero_haze', souls: 3400, hp: 520, max_hp: 1000, enemy_heroes: ['hero_graves'] }));
  await page.locator('#replay-files').setInputFiles({ name: 'ai-scenario.jsonl', mimeType: 'application/x-ndjson', buffer: Buffer.from(frames.map(f => JSON.stringify(f)).join('\n')) });
  await page.getByRole('button', { name: 'Analyze selected files' }).click();
  await expect(page.locator('#replay-status')).toContainText('Replay complete.', { timeout: 110000 });
  await page.locator('[data-page="coach"]').click();
}
async function expectNoKeyAnywhere(page: Page, logs: string[]) {
  // Page markup, form values and every localStorage entry except the settings record itself.
  const leaked = await page.evaluate(key => {
    const html = document.documentElement.outerHTML + [...document.querySelectorAll('input,textarea')].map(e => (e as HTMLInputElement).value).join(' ');
    const storage = Object.keys(localStorage).filter(k => k !== 'deadlock-coach-settings-v1').map(k => localStorage.getItem(k)).join(' ');
    return [html.includes(key), storage.includes(key)];
  }, KEY);
  expect(leaked).toEqual([false, false]);
  expect(logs.join('\n')).not.toContain(KEY);
}
async function exportRecording(page: Page) {
  // Seed one recorded frame so the export is not empty, then download it.
  await page.evaluate(() => new Promise<void>((resolve, reject) => {
    const req = indexedDB.open('deadlock-coach-recordings-v1', 1);
    req.onupgradeneeded = () => req.result.createObjectStore('frames', { keyPath: 'id', autoIncrement: true });
    req.onerror = () => reject(req.error);
    req.onsuccess = () => { const tx = req.result.transaction('frames', 'readwrite'); tx.objectStore('frames').add({ created: Date.now(), timestamp: 0, snapshot: { game_clock_secs: 605 }, cues: [], image: new Blob(['x'], { type: 'image/jpeg' }) }); tx.oncomplete = () => { req.result.close(); resolve(); }; tx.onerror = () => reject(tx.error); };
  }));
  await page.locator('[data-page="replay"]').click();
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export frames + snapshots' }).click();
  return readFile(await (await download).path(), 'utf8');
}

for (const provider of ['openai', 'anthropic'] as const) {
  test(`${provider} gateway: Ask the coach now shows sourced AI advice, lists it as AI and never exports the key`, async ({ page }) => {
    const logs: string[] = []; page.on('console', m => logs.push(m.text())); page.on('pageerror', e => logs.push(e.message));
    await page.addInitScript(s => localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s)), settings(provider));
    const json = JSON.stringify({ speak: ADVICE, priority: 'High', why: '3,400 unspent souls at 10:05; Graves on the enemy team', focus: 'souls' });
    const seen = provider === 'openai'
      ? await mockGateway(page, 'gateway.test', () => ({ model: 'mock-model-2026', choices: [{ message: { role: 'assistant', content: '```json\n' + json + '\n```' } }] }))
      : await mockGateway(page, 'anthropic.test', () => ({ model: 'mock-model-2026', content: [{ type: 'text', text: json }] }));
    await replay(page);
    await expect(page.locator('#ai-state')).toHaveText('READY');
    await page.getByRole('button', { name: 'Ask the coach now' }).click();
    await expect(page.locator('#ai-advice')).toHaveText(ADVICE);
    await expect(page.locator('#ai-why')).toContainText('3,400 unspent souls');
    await expect(page.locator('#ai-meta')).toContainText(/On demand · 10:05 · \d+ ms · mock-model-2026 · ~\d+ input tokens/);
    const row = page.locator('#cue-list li.ai-cue').first();
    await expect(row).toContainText(ADVICE); await expect(row.locator('.ai-mark')).toHaveText('AI');
    // Request shape: key only in the auth header, the state summary and knowledge in the body.
    expect(seen).toHaveLength(1);
    const body = JSON.parse(seen[0].body);
    expect(seen[0].body).not.toContain(KEY);
    if (provider === 'openai') {
      expect(seen[0].headers.authorization).toBe(`Bearer ${KEY}`);
      expect(body.model).toBe('mock-model'); expect(body.messages.map((m: { role: string }) => m.role)).toEqual(['system', 'user']);
      expect(body.messages[1].content).toContain('Unspent souls 3,400');
      // Souls are worded against the usual buy at 10:05 (3,200 tier), with the souls gate stated.
      expect(body.messages[1].content).toMatch(/can afford the usual 3,200 buy for 10:05 · Souls advice: (allowed|not allowed \(the rule coach already said it)/);
    } else {
      expect(seen[0].headers['x-api-key']).toBe(KEY); expect(seen[0].headers['anthropic-version']).toBe('2023-06-01');
      expect(seen[0].headers['anthropic-dangerous-direct-browser-access']).toBe('true');
      expect(body.system).toContain('elite Deadlock coach'); expect(body.messages[0].content).toContain('You: Haze');
    }
    expect(body.max_tokens).toBe(200);
    // The AI cue is in the cue log and the recording export, and the key is in neither.
    const exported = await exportRecording(page);
    expect(exported).toContain(ADVICE); expect(JSON.parse(exported).spoken.some((c: { source: string }) => c.source === 'ai')).toBe(true);
    expect(exported).not.toContain(KEY);
    await page.locator('[data-page="settings"]').click();
    await expect(page.locator('#diagnostics')).toContainText('mock-model');
    await expectNoKeyAnywhere(page, logs);
  });
}

test('settings save the AI gateway, test connection reports HTTP errors without the key, and clear key works', async ({ page }) => {
  const logs: string[] = []; page.on('console', m => logs.push(m.text()));
  await mockGateway(page, 'gateway.test', () => ({ error: { message: `Incorrect API key provided: ${KEY}` } }), 401);
  await page.goto('./#settings');
  await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
  await page.getByLabel('Enable the AI coach').check();
  await page.getByLabel('Base URL').fill('https://gateway.test/v1');
  await page.getByLabel('Model', { exact: true }).fill('mock-model');
  await page.getByLabel('API key').fill(KEY);
  await page.getByLabel('Advice interval (seconds)').fill('20');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await page.reload();
  await expect(page.getByLabel('Enable the AI coach')).toBeChecked();
  await expect(page.getByLabel('Advice interval (seconds)')).toHaveValue('20');
  await expect(page.getByLabel('API key')).toHaveValue('');
  await expect(page.getByLabel('API key')).toHaveAttribute('placeholder', /key is saved/);
  await page.getByRole('button', { name: 'Test connection' }).click();
  await expect(page.locator('#ai-test-result')).toContainText('Gateway error: HTTP 401');
  await expect(page.locator('#ai-test-result')).not.toContainText(KEY);
  await page.getByLabel('Extra request headers').fill('[1]');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.locator('#notice')).toContainText('Extra headers must be a JSON object');
  await page.getByLabel('Extra request headers').fill('');
  await page.getByRole('button', { name: 'Clear key' }).click();
  await expect(page.locator('#ai-test-result')).toHaveText('API key removed from this browser.');
  expect(await page.evaluate(() => localStorage.getItem('deadlock-coach-settings-v1'))).not.toContain(KEY);
  expect(logs.join('\n')).not.toContain(KEY);
});

test('a CORS or network failure is reported as such in the AI panel', async ({ page }) => {
  await page.addInitScript(s => localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s)), settings('openai'));
  await page.route('https://gateway.test/**', route => route.abort('failed'));
  await page.goto('./');
  await page.getByRole('button', { name: 'Ask the coach now' }).click();
  await expect(page.locator('#ai-state')).toHaveText('ERROR');
  await expect(page.locator('#ai-error')).toContainText('Network or CORS failure reaching https://gateway.test');
  await expect(page.locator('#ai-error')).not.toContainText(KEY);
});

test('live coaching: periodic AI advice is spoken through the speaker, and pause stops new requests', async ({ page }) => {
  await page.addInitScript(s => {
    localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s));
    const w = window as unknown as { spoken: string[] }; w.spoken = [];
    if (window.speechSynthesis) speechSynthesis.speak = (u: SpeechSynthesisUtterance) => { w.spoken.push(u.text); setTimeout(() => u.onend?.(new Event('end') as SpeechSynthesisEvent), 20); };
    Object.defineProperty(navigator.mediaDevices, 'getDisplayMedia', { value: async () => {
      const image = new Image(); image.src = '/DeadlockCoach/assets/fixtures/00059_1792966931596.jpg'; await image.decode();
      const canvas = document.createElement('canvas'); canvas.width = 1600; canvas.height = 900; const ctx = canvas.getContext('2d')!;
      const draw = () => ctx.drawImage(image, 0, 0); draw(); setInterval(draw, 100); return canvas.captureStream(10);
    } });
  }, { ...settings('openai'), aiInterval: 15, record: false });
  const seen = await mockGateway(page, 'gateway.test', () => ({ choices: [{ message: { content: JSON.stringify({ speak: LIVE_ADVICE, priority: 'Normal', why: 'live test', focus: 'positioning' }) } }] }));
  await page.goto('./'); await page.getByRole('button', { name: 'Start coaching' }).click();
  await expect(page.locator('#session-state')).toHaveText('Coaching live', { timeout: 110000 });
  // The first periodic request waits a full interval after gameplay is first observed.
  await expect.poll(() => seen.length, { timeout: 60000 }).toBeGreaterThanOrEqual(1);
  expect(JSON.parse(seen[0].body).messages[1].content).toContain('Trigger: periodic');
  await expect(page.locator('#ai-meta')).toContainText('Periodic');
  await expect.poll(() => page.evaluate(() => (window as unknown as { spoken: string[] }).spoken), { timeout: 10000 }).toContain(LIVE_ADVICE);
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  const count = seen.length; await page.waitForTimeout(17000);
  expect(seen.length).toBe(count);
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
});