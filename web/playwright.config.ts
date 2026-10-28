import { defineConfig } from '@playwright/test';
// PW_PORT lets several checkouts run their browser suites side by side (default 4173).
const port = Number(process.env.PW_PORT || 4173);
const url = `http://127.0.0.1:${port}/DeadlockCoach/`;
export default defineConfig({
  testDir: './tests/browser', timeout: 120000, expect: { timeout: 10000 },
  fullyParallel: false, workers: 1, retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: { baseURL: url, browserName: 'chromium',
    trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  webServer: { command: `npm run preview -- --port ${port} --strictPort`, url,
    reuseExistingServer: !process.env.CI, timeout: 30000 },
});
