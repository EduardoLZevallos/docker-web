import { test as base, expect } from '@playwright/test';
import type { E2EEnvironment } from './env';
import { e2eEnvironment } from './env';
import { DockerFixture } from './fixtures';

interface BrowserE2EFixtures {
  e2eEnvironment: E2EEnvironment;
  docker: DockerFixture;
}

export const test = base.extend<BrowserE2EFixtures>({
  e2eEnvironment: async ({}, use) => {
    await use(e2eEnvironment());
  },
  docker: async ({ e2eEnvironment }, use, testInfo) => {
    const fixture = new DockerFixture(e2eEnvironment, testInfo.title);
    try {
      await use(fixture);
    } finally {
      await fixture.cleanup();
    }
  },
});

export { expect };
export type { E2EEnvironment };
