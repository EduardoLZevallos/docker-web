import { defineConfig, devices } from '@playwright/test';

const PORT = 5173;
const FRONTEND_URL = `http://localhost:${PORT}`;

function backendOrigin(): string {
  const raw = process.env.BACKEND_URL?.trim();
  if (!raw) {
    throw new Error(
      'BACKEND_URL is required for browser E2E; pass the exact backend origin, without /api.',
    );
  }

  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new Error(`BACKEND_URL must be an absolute http(s) origin; received "${raw}".`);
  }
  if (
    (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') ||
    parsed.pathname !== '/' ||
    parsed.search ||
    parsed.hash
  ) {
    throw new Error(
      `BACKEND_URL must be an http(s) origin without path, query, or fragment; received "${raw}".`,
    );
  }
  return parsed.origin;
}

const BACKEND_URL = backendOrigin();
const UI_PREBUILT = process.env.E2E_UI_PREBUILT === 'true';
const PREVIEW_PID_FILE = process.env.E2E_PREVIEW_PID_FILE?.trim();
const PREVIEW_COMMAND = PREVIEW_PID_FILE
  ? `echo $$ > ${JSON.stringify(PREVIEW_PID_FILE)}; exec npm run preview`
  : 'npm run preview';
const WEB_SERVER_COMMAND = UI_PREBUILT
  ? PREVIEW_COMMAND
  : `npm run build:e2e && ${PREVIEW_COMMAND}`;
const inheritedEnv = Object.fromEntries(
  Object.entries(process.env).filter(
    (entry): entry is [string, string] => typeof entry[1] === 'string',
  ),
);

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: false,
  timeout: 90_000,
  expect: {
    timeout: 10_000,
  },
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
  ],
  outputDir: process.env.PLAYWRIGHT_OUTPUT_DIR ?? 'test-results',
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
    command: WEB_SERVER_COMMAND,
    url: FRONTEND_URL,
    reuseExistingServer: false,
    timeout: 180_000,
    env: {
      ...inheritedEnv,
      BACKEND_URL,
      VITE_E2E: 'true',
    },
  },
});
