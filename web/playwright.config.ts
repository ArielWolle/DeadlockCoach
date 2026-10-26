import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/browser', timeout: 120000, expect: { timeout: 10000 },
  fullyParallel: false, workers: 1, retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: { baseURL: 'http://127.0.0.1:4173/DeadlockCoach/', browserName: 'chromium',
    trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  webServer: { command: 'npm run preview', url: 'http://127.0.0.1:4173/DeadlockCoach/',
    reuseExistingServer: !process.env.CI, timeout: 30000 },
});
