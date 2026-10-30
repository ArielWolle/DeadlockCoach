import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Recordings split by game (recording-games.ts) and the "Download each game's recording when it
// ends" setting. A whole match cannot be captured in a test: recorded frames are seeded into
// IndexedDB under the live game id (__deadlockTestHooks.recordingGame) and the match is played
// through __deadlockTestHooks.liveFrame (main.ts liveMatchFrame, the same end-of-match path).
test.use({ serviceWorkers: 'block' });
const base = { record: true, aiEnabled: false, aiReview: false, interval: 30 };

async function setup(page: Page, settings: Record<string, unknown>) {
  await page.addInitScript(s => {
    localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(s));
    (window as unknown as { __deadlockTestHooks: object }).__deadlockTestHooks = {};
  }, settings);
  await page.goto('./');
}
/** 13+ minutes of Grey Talon, a frame every 4 s, ending in a Defeat banner and the post-game screen. */
function match(wall0: number) {
  const out: Array<{ s: Record<string, unknown>; wall: number }> = [];
  for (let c = 600; c <= 1400; c += 4) out.push({ wall: wall0 + (c - 600) * 1000, s: { game_clock_secs: c, screen: 'Gameplay', dead: false, hero: 'hero_orion', souls: 900, hp: 900, max_hp: 1000, health_observations: 3 } });
  const w = out.at(-1)!.wall;
  out.push({ wall: w + 4000, s: { game_clock_secs: 1404, screen: 'Scoreboard', hero: 'hero_orion', match_result: 'loss' } });
  out.push({ wall: w + 9000, s: { screen: 'PostGame' } });
  return out;
}
const gameId = (page: Page) => page.evaluate(() => (window as unknown as { __deadlockTestHooks: { recordingGame: () => string } }).__deadlockTestHooks.recordingGame());
const feed = (page: Page, frames: Array<{ s: Record<string, unknown>; wall: number }>) => page.evaluate(fs => {
  const h = (window as unknown as { __deadlockTestHooks: { liveFrame: (s: unknown, w: number) => void } }).__deadlockTestHooks;
  for (const f of fs) h.liveFrame(f.s, f.wall);
}, frames);
/** Store frames as the live loop would (`game` undefined = recorded before games were split). */
const seed = (page: Page, frames: Array<{ created: number; snapshot: Record<string, unknown>; game?: string }>) => page.evaluate(fs => new Promise<void>((resolve, reject) => {
  const req = indexedDB.open('deadlock-coach-recordings-v1', 1);
  req.onupgradeneeded = () => req.result.createObjectStore('frames', { keyPath: 'id', autoIncrement: true });
  req.onerror = () => reject(req.error);
  req.onsuccess = () => {
    const tx = req.result.transaction('frames', 'readwrite');
    for (const f of fs) tx.objectStore('frames').add({ ...f, timestamp: 0, cues: [], image: new Blob(['jpeg-bytes'], { type: 'image/jpeg' }) });
    tx.oncomplete = () => { req.result.close(); resolve(); }; tx.onerror = () => reject(tx.error);
  };
}), frames);
const frames = (n: number, t0: number, game?: string, clock0 = 600, hero = 'hero_orion') =>
  Array.from({ length: n }, (_, i) => ({ created: t0 + i * 4000, game, snapshot: { game_clock_secs: clock0 + i * 4, screen: 'Gameplay', hero } }));

test('with automatic download on, a finished game is saved as one file with only its own frames', async ({ page }) => {
  await setup(page, { ...base, autoDownloadRecording: true });
  const first = await gameId(page), t0 = Date.now() - 900_000;
  await seed(page, [...frames(5, t0 - 3_600_000, 'g-older-game', 600, 'hero_artist'), ...frames(12, t0, first)]);
  const downloading = page.waitForEvent('download', { timeout: 20000 });
  await feed(page, match(t0));
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/^deadlock-recording-\d{4}-\d\d-\d\d-\d{4}-grey-talon-loss\.json$/);
  const file = JSON.parse(await readFile(await download.path(), 'utf8'));
  expect(file.frames).toHaveLength(12); // not the older Violet game
  expect(file.game).toMatchObject({ key: first, hero: 'hero_orion', result: 'loss' });
  await expect(page.locator('#notice-text')).toContainText("Saved this game's recording (12 frames");
  // Frames after the end belong to a new game.
  expect(await gameId(page)).not.toBe(first);
});

test('with automatic download off, nothing downloads; the shelf lists each game with Download and Delete', async ({ page }) => {
  await setup(page, base);
  let downloads = 0; page.on('download', () => { downloads++; });
  const t0 = Date.now() - 7_200_000;
  // Two older untagged games (a clock restart splits them) and one tagged game.
  await seed(page, [...frames(6, t0, undefined, 900), ...frames(4, t0 + 60_000, undefined, 30, 'hero_artist'), ...frames(8, t0 + 3_600_000, 'g-tagged')]);
  await feed(page, match(Date.now() - 900_000));
  await page.waitForTimeout(2500); expect(downloads).toBe(0);
  await page.locator('[data-page="replay"]').click();
  const rows = page.locator('#recording-games .recorded-game');
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText('Game 3 · Grey Talon'); await expect(rows.nth(0)).toContainText('8 frames');
  await expect(rows.nth(1)).toContainText('Game 2'); await expect(rows.nth(1)).toContainText('clock 0:30–0:42');
  await expect(rows.nth(2)).toContainText('Game 1 · Grey Talon'); await expect(rows.nth(2)).toContainText('6 frames');
  const downloading = page.waitForEvent('download');
  await rows.nth(1).getByRole('button', { name: 'Download' }).click();
  const file = JSON.parse(await readFile(await (await downloading).path(), 'utf8'));
  expect(file.frames).toHaveLength(4);
  await rows.nth(2).getByRole('button', { name: 'Delete' }).click();
  await expect(rows).toHaveCount(2);
  await expect(page.locator('#recording-stats')).toContainText('12 frames');
  await expect(page.locator('#notice-text')).toContainText('Game 1 · Grey Talon deleted.');
});
