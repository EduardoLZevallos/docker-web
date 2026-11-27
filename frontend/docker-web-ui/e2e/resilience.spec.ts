import { expect, test } from './support/test';
import type { E2EEnvironment } from './support/env';
import { outerDocker } from './support/docker';
import {
  expectGraph,
  listButton,
  openApp,
  openAppShell,
  readGraphSnapshot,
  waitForBridgeReady,
} from './support/ui';

interface OuterInspect {
  Id: string;
  State?: {
    Paused?: boolean;
    Running?: boolean;
    Status?: string;
    StartedAt?: string;
  };
}

function inspectOuter(env: E2EEnvironment, id: string): OuterInspect {
  return JSON.parse(outerDocker(env, ['inspect', id]))[0] as OuterInspect;
}

async function waitOuterPaused(
  env: E2EEnvironment,
  id: string,
  paused: boolean,
): Promise<void> {
  await expect
    .poll(
      async () => inspectOuter(env, id).State?.Paused ?? false,
      { timeout: 20_000 },
    )
    .toBe(paused);
}

async function waitOuterRunning(
  env: E2EEnvironment,
  id: string,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const state = inspectOuter(env, id).State;
        return {
          paused: state?.Paused ?? false,
          running: state?.Running ?? false,
        };
      },
      { timeout: 20_000 },
    )
    .toEqual({ paused: false, running: true });
}

async function resumeOuter(
  env: E2EEnvironment,
  id: string,
): Promise<void> {
  const current = inspectOuter(env, id);
  if (current.State?.Paused) {
    outerDocker(env, ['unpause', id]);
  } else if (current.State?.Running !== true) {
    outerDocker(env, ['start', id]);
  }
  await waitOuterRunning(env, id);
}

async function resumeDind(env: E2EEnvironment): Promise<void> {
  const current = inspectOuter(env, env.dindId);
  if (current.State?.Paused) {
    outerDocker(env, ['unpause', env.dindId]);
  }
  await waitOuterPaused(env, env.dindId, false);
}

test.describe('real Docker outage and recovery', () => {
  test.afterEach(async ({ e2eEnvironment }) => {
    try {
      await resumeDind(e2eEnvironment);
    } catch {
      // The harness cleanup also unpauses its exact DinD container.
    }
    try {
      await resumeOuter(e2eEnvironment, e2eEnvironment.backendId);
    } catch {
      // The harness owns the backend container and removes it during cleanup.
    }
  });

  test('shows no fake topology during an initial DinD outage and recovers the real fixtures', async ({
    page,
    docker,
    e2eEnvironment,
  }) => {
    test.setTimeout(120_000);

    const network = await docker.createNetwork('initial-outage');
    const container = await docker.createContainer('initial-outage-container', {
      networks: [network.name],
    });
    const expected = docker.expectedTopology();

    outerDocker(e2eEnvironment, ['pause', e2eEnvironment.dindId]);
    try {
      await waitOuterPaused(e2eEnvironment, e2eEnvironment.dindId, true);

      await openAppShell(page);
      const error = page.getByTestId('topology-error');
      await expect(error).toBeVisible({ timeout: 30_000 });
      await expect(error).toContainText(
        'No topology data is available yet.',
      );
      await expect(error).toContainText(/503|Docker connection failed/i);
      await expect(page.getByTestId('topology-graph-empty')).toBeVisible();
      await expect(page.getByTestId('topology-graph-loading')).toHaveCount(0);
      await expect(page.getByTestId('demo-mode-notice')).toHaveCount(0);
      await expect(page.getByTestId('search-no-results')).toHaveCount(0);
      await expect(page.getByTestId('count-total-containers')).toHaveText(
        'Containers total: 0',
      );
      await expect(page.getByTestId('count-total-networks')).toHaveText(
        'Networks total: 0',
      );
      await expect(page.getByTestId('last-success-timestamp')).toContainText(
        'Never',
      );
      await expect(page.getByTestId('container-list-empty')).toHaveText(
        'Waiting for topology data.',
      );
      await expect(page.getByTestId('network-list-empty')).toHaveText(
        'Waiting for topology data.',
      );
      await expect(page.getByTestId('details-empty')).toBeVisible();

      const snapshot = await readGraphSnapshot(page);
      expect(snapshot).not.toBeNull();
      expect(snapshot?.ready).toBe(false);
      expect(snapshot?.nodes).toEqual([]);
      expect(snapshot?.edges).toEqual([]);
    } finally {
      await resumeDind(e2eEnvironment);
    }

    await expect(page.getByTestId('topology-error')).toHaveCount(0, {
      timeout: 30_000,
    });
    await waitForBridgeReady(page);
    await expectGraph(page, expected);
    await expect(listButton(page, 'container', container.id)).toBeVisible();
  });

  test('a paused backend leaves a pending refresh disabled and prevents duplicate requests', async ({
    page,
    docker,
    e2eEnvironment,
  }) => {
    test.setTimeout(90_000);

    const network = await docker.createNetwork('paused-backend');
    await docker.createContainer('before-paused-backend', {
      networks: [network.name],
    });
    const initial = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, initial);

    const apiRequests: string[] = [];
    page.on('request', (request) => {
      const path = new URL(request.url()).pathname;
      if (path.startsWith('/api/')) {
        apiRequests.push(path);
      }
    });

    const refresh = page.getByTestId('refresh-button');
    outerDocker(e2eEnvironment, ['pause', e2eEnvironment.backendId]);
    try {
      await waitOuterPaused(e2eEnvironment, e2eEnvironment.backendId, true);

      await refresh.click();
      await expect(page.getByTestId('refreshing-indicator')).toBeVisible({
        timeout: 10_000,
      });
      await expect(refresh).toBeDisabled();
      await expect(refresh).toHaveAttribute('aria-busy', 'true');
      await expect
        .poll(() => apiRequests.length, { timeout: 10_000 })
        .toBeGreaterThan(0);

      const pendingRequests = [...apiRequests];
      await refresh.click({ force: true }).catch(() => undefined);
      await page.waitForTimeout(750);
      expect(apiRequests).toEqual(pendingRequests);

      const added = await docker.createContainer(
        'created-while-backend-paused',
        { networks: [network.name] },
      );
      const recovered = docker.expectedTopology();
      expect(recovered.edges).toHaveLength(2);

      outerDocker(e2eEnvironment, ['unpause', e2eEnvironment.backendId]);
      await waitOuterRunning(e2eEnvironment, e2eEnvironment.backendId);

      await expect(page.getByTestId('refreshing-indicator')).toHaveCount(0, {
        timeout: 30_000,
      });
      await expect(refresh).toBeEnabled();
      await expect(refresh).toHaveAttribute('aria-busy', 'false');
      await expectGraph(page, recovered);
      await expect(listButton(page, 'container', added.id)).toBeVisible();
    } finally {
      await resumeOuter(e2eEnvironment, e2eEnvironment.backendId);
    }
  });

  test('recovers from a backend outage beyond the request timeout while retaining the stale topology', async ({
    page,
    docker,
    e2eEnvironment,
  }) => {
    test.setTimeout(120_000);

    const network = await docker.createNetwork('backend-outage');
    await docker.createContainer('before-backend-outage', {
      networks: [network.name],
    });
    const initial = docker.expectedTopology();

    await openApp(page);
    await expectGraph(page, initial);
    const staleTimestamp = await page
      .getByTestId('last-success-timestamp')
      .locator('time')
      .getAttribute('datetime');
    expect(staleTimestamp).not.toBeNull();
    const backendBefore = inspectOuter(e2eEnvironment, e2eEnvironment.backendId);

    outerDocker(e2eEnvironment, ['pause', e2eEnvironment.backendId]);
    try {
      await waitOuterPaused(e2eEnvironment, e2eEnvironment.backendId, true);

      const refresh = page.getByTestId('refresh-button');
      await refresh.click();

      await expect(page.getByTestId('config-warning')).toBeVisible({
        timeout: 20_000,
      });
      const error = page.getByTestId('topology-error');
      await expect(error).toBeVisible({ timeout: 20_000 });
      await expect(error).toContainText(
        'The last successful topology remains visible.',
      );
      await expectGraph(page, initial);
      await expect(
        page.getByTestId('last-success-timestamp').locator('time'),
      ).toHaveAttribute('datetime', staleTimestamp as string);
      await expect(refresh).toBeEnabled();

      const added = await docker.createContainer(
        'created-during-backend-outage',
        { networks: [network.name] },
      );
      const recovered = docker.expectedTopology();
      expect(recovered.edges).toHaveLength(2);

      outerDocker(e2eEnvironment, ['unpause', e2eEnvironment.backendId]);
      await waitOuterRunning(e2eEnvironment, e2eEnvironment.backendId);

      await expect(page.getByTestId('config-warning')).toHaveCount(0, {
        timeout: 30_000,
      });
      await expect(page.getByTestId('topology-error')).toHaveCount(0, {
        timeout: 30_000,
      });
      await expectGraph(page, recovered);
      await expect(listButton(page, 'container', added.id)).toBeVisible();

      const backendAfter = inspectOuter(
        e2eEnvironment,
        e2eEnvironment.backendId,
      );
      expect(backendAfter.Id).toBe(backendBefore.Id);
      expect(backendAfter.State?.StartedAt).toBe(backendBefore.State?.StartedAt);
      expect(backendAfter.State?.Running).toBe(true);
    } finally {
      await resumeOuter(e2eEnvironment, e2eEnvironment.backendId);
    }
  });

  test('keeps stale topology through a paused DinD outage and recovers on the same containers', async ({
    page,
    docker,
    e2eEnvironment,
  }) => {
    test.setTimeout(120_000);

    const network = await docker.createNetwork('outage');
    const container = await docker.createContainer('outage-container', {
      networks: [network.name],
    });

    const initial = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, initial);

    const backendBefore = inspectOuter(e2eEnvironment, e2eEnvironment.backendId);
    const dindBefore = inspectOuter(e2eEnvironment, e2eEnvironment.dindId);
    expect(dindBefore.State?.Running).toBe(true);
    const staleTimestamp = await page
      .getByTestId('last-success-timestamp')
      .locator('time')
      .getAttribute('datetime');
    expect(staleTimestamp).not.toBeNull();

    let dindPaused = false;
    try {
      outerDocker(e2eEnvironment, ['pause', e2eEnvironment.dindId]);
      dindPaused = true;
      await expect
        .poll(
          async () =>
            inspectOuter(e2eEnvironment, e2eEnvironment.dindId).State?.Paused ??
            false,
          { timeout: 20_000 },
        )
        .toBe(true);

      await expect(page.getByTestId('topology-error')).toBeVisible({
        timeout: 30_000,
      });
      await expect(page.getByTestId('topology-error')).toContainText(
        /503|Docker connection failed/i,
      );
      await expectGraph(page, initial);
      const staleSnapshot = await readGraphSnapshot(page);
      expect(
        staleSnapshot?.nodes
          .map((node) => `${node.kind}:${node.rawId}`)
          .sort(),
      ).toEqual([...initial.nodes].sort());
      await expect(
        page.getByTestId('last-success-timestamp').locator('time'),
      ).toHaveAttribute('datetime', staleTimestamp as string);
      await expect(page.getByTestId('topology-error')).toContainText(
        'The last successful topology remains visible.',
      );
    } finally {
      if (dindPaused) {
        await resumeDind(e2eEnvironment);
      }
    }

    await expect(page.getByTestId('topology-error')).toHaveCount(0, {
      timeout: 30_000,
    });
    await expectGraph(page, initial);

    const added = await docker.createContainer('after-recovery', {
      networks: [network.name],
    });
    const recovered = docker.expectedTopology();
    expect(recovered.edges).toHaveLength(2);

    await page.getByTestId('refresh-button').click();
    await expectGraph(page, recovered);
    await expect(
      page.locator(`[data-testid="node-select-container-${added.id}"]`),
    ).toBeVisible();
    await expect(
      page.locator(`[data-testid="node-select-container-${container.id}"]`),
    ).toBeVisible();

    const backendAfter = inspectOuter(e2eEnvironment, e2eEnvironment.backendId);
    expect(backendAfter.Id).toBe(backendBefore.Id);
    expect(backendAfter.State?.StartedAt).toBe(backendBefore.State?.StartedAt);
  });
});
