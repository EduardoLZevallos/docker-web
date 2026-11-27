import { expect, test } from './support/test';
import { filterExpectedTopology } from './support/fixtures';
import {
  expectGraph,
  expectNoSelection,
  listButton,
  openApp,
} from './support/ui';

test.describe('container name and raw-status search', () => {
  test('matches container names case-insensitively and restores the full graph when cleared', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('search-name');
    const running = await docker.createContainer('web-running', {
      networks: [network.name],
    });
    await docker.createContainer('paused-worker', {
      networks: [network.name],
      pause: true,
    });
    await docker.createContainer('exited-worker', {
      networks: [network.name],
      command: ['sh', '-c', 'exit 7'],
      waitForState: 'exited',
    });

    const expected = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, expected);

    const query = 'WEB-RUNNING';
    const filtered = filterExpectedTopology(expected, query);
    await page.getByTestId('search-input').fill(query);
    await expectGraph(page, filtered);

    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 1',
    );
    await expect(page.getByTestId('count-total-containers')).toHaveText(
      `Containers total: ${expected.containers.length}`,
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      'Networks visible: 1',
    );
    await expect(page.getByTestId('search-summary')).toHaveText(
      `Showing 1 of ${expected.containers.length} containers and 1 of ${expected.networks.length} networks. Search: "${query}".`,
    );
    await expect(page.locator('[data-node-kind="container"]')).toHaveCount(1);
    await expect(page.locator('[data-node-kind="network"]')).toHaveCount(1);
    await expect(listButton(page, 'container', running.id)).toBeVisible();

    await page.getByTestId('search-clear').click();
    await expect(page.getByTestId('search-input')).toHaveValue('');
    await expectGraph(page, expected);
    await expect(page.locator('[data-node-kind="container"]')).toHaveCount(
      expected.containers.length,
    );
    await expect(page.getByTestId('search-summary')).toHaveText(
      `Showing all ${expected.containers.length} containers and ${expected.networks.length} networks.`,
    );
  });

  test('matches raw Docker statuses including paused and exited containers', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('search-status');
    await docker.createContainer('running-name', {
      networks: [network.name],
    });
    const paused = await docker.createContainer('paused-name', {
      networks: [network.name],
      pause: true,
    });
    const exited = await docker.createContainer('exited-name', {
      networks: [network.name],
      command: ['sh', '-c', 'exit 7'],
      waitForState: 'exited',
    });

    const expected = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, expected);

    const pausedQuery = 'PAUSED';
    const pausedFiltered = filterExpectedTopology(expected, pausedQuery);
    await page.getByTestId('search-input').fill(pausedQuery);
    await expectGraph(page, pausedFiltered);
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 1',
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      'Networks visible: 1',
    );
    await expect(listButton(page, 'container', paused.id)).toBeVisible();

    const exitedQuery = 'EXITED (7)';
    const exitedFiltered = filterExpectedTopology(expected, exitedQuery);
    await page.getByTestId('search-input').fill(exitedQuery);
    await expectGraph(page, exitedFiltered);
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 1',
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      'Networks visible: 0',
    );
    await expect(listButton(page, 'container', exited.id)).toBeVisible();
    await expect(page.getByTestId('network-list-empty')).toBeVisible();

    await page.getByTestId('search-clear').click();
    await expectGraph(page, expected);
  });

  test('shows a no-results state, clears the selection, and restores the topology', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('no-match');
    const container = await docker.createContainer('only-container', {
      networks: [network.name],
    });

    const expected = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, expected);

    await listButton(page, 'container', container.id).click();
    await expect(listButton(page, 'container', container.id)).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    const query = 'definitely-not-a-node';
    await page.getByTestId('search-input').fill(query);
    await expect(page.getByTestId('search-no-results')).toBeVisible();
    await expect(page.getByTestId('container-list-empty')).toBeVisible();
    await expect(page.getByTestId('network-list-empty')).toBeVisible();
    await expect(page.getByTestId('details-empty')).toBeVisible();
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 0',
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      'Networks visible: 0',
    );
    await expectGraph(page, { nodes: [], edges: [] });
    await expectNoSelection(page);

    await page.getByTestId('search-clear').click();
    await expect(page.getByTestId('search-no-results')).toHaveCount(0);
    await expect(page.getByTestId('search-input')).toHaveValue('');
    await expectGraph(page, expected);
    await expect(page.getByTestId('details-empty')).toBeVisible();
    await expect(page.getByTestId('search-summary')).toHaveText(
      `Showing all ${expected.containers.length} containers and ${expected.networks.length} networks.`,
    );
  });
});
