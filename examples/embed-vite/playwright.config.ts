import { defineConfig, devices } from '@playwright/test';

const BUILD_PORT = 4180;
const DEV_PORT = 4181;

// The example consumer, both ways a consumer runs it: the production build
// (`vite build` + `vite preview`) and the dev server, whose dependency
// pre-bundling (`optimizeDeps`) is where a worker inside node_modules can
// break. Every browser of the repo's matrix against each.
const browsers = [
  { name: 'chromium', use: devices['Desktop Chrome'] },
  { name: 'firefox', use: devices['Desktop Firefox'] },
  { name: 'webkit', use: devices['Desktop Safari'] },
] as const;
const servers = [
  { name: 'build', port: BUILD_PORT },
  { name: 'dev', port: DEV_PORT },
] as const;

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: { trace: 'retain-on-failure' },
  projects: servers.flatMap((server) =>
    browsers.map((browser) => ({
      name: `${browser.name}-${server.name}`,
      use: { ...browser.use, baseURL: `http://127.0.0.1:${server.port}` },
    })),
  ),
  webServer: [
    {
      command: `npm run build && npm run preview -- --port ${BUILD_PORT}`,
      url: `http://127.0.0.1:${BUILD_PORT}`,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: `npm run dev -- --port ${DEV_PORT}`,
      url: `http://127.0.0.1:${DEV_PORT}`,
      reuseExistingServer: !process.env.CI,
    },
  ],
});
