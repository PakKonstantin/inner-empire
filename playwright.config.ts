import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end configuration.
 *
 * These drive the real frontend — the actual components, stores, editor and
 * router — against a faithful in-memory double of the Tauri command surface.
 * That covers the flows a user performs and the wiring between them, which is
 * what breaks in practice.
 *
 * It does not drive the packaged desktop binary; that needs `tauri-driver` and
 * a WebDriver session per platform, which the release job is the right place
 * for. The Rust suite already exercises the backend against a real filesystem,
 * so the two halves are covered by the tooling suited to each.
 */
export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  timeout: 30_000,
  expect: { timeout: 7_000 },

  use: {
    baseURL: 'http://127.0.0.1:1421',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        // Some environments ship a Chromium that does not match the revision
        // this Playwright expects. Honouring an explicit path lets the suite
        // run against whatever is installed; CI leaves it unset and uses the
        // browser `playwright install` fetched.
        ...(process.env.CHROMIUM_PATH
          ? { launchOptions: { executablePath: process.env.CHROMIUM_PATH } }
          : {}),
      },
    },
  ],

  webServer: {
    // The production build, so the tests exercise what ships rather than the
    // development server's transforms.
    command: 'pnpm build && pnpm exec vite preview --port 1421 --strictPort',
    url: 'http://127.0.0.1:1421',
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
