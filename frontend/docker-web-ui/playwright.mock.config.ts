import { defineConfig, devices } from '@playwright/test';

const PORT = 5173;
const FRONTEND_URL = `http://localhost:${PORT}`;

const inheritedEnv = Object.fromEntries(
  Object.entries(process.env).filter(
    (entry): entry is [string, string] => typeof entry[1] === 'string',
  ),
);

export default defineConfig({
  testDir: './tests/mocked',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: false,
  timeout: 90_000,
  expect: {
    timeout: 10_000,
  },
  reporter: [['list']],
  outputDir:
    process.env.PLAYWRIGHT_MOCK_OUTPUT_DIR ?? 'test-results-mocked',
  use: {
    baseURL: FRONTEND_URL,
    headless: true,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
      },
    },
  ],
  webServer: {
    command: 'npm run build:e2e && npm run preview',
    url: FRONTEND_URL,
    reuseExistingServer: false,
    timeout: 180_000,
    env: {
      ...inheritedEnv,
      VITE_E2E: 'true',
    },
  },
});
