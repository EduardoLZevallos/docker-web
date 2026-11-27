import { expect, test } from './support/test';
import { expectGraph, listButton, openApp } from './support/ui';

test.describe('real Docker mutations and refresh behavior', () => {
  test('manual refresh reflects adding, stopping, and removing a container', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('manual-refresh');
    const first = await docker.createContainer('first', {
      networks: [network.name],
    });

    const initial = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, initial);

    const second = await docker.createContainer('second', {
      networks: [network.name],
    });
    const afterCreate = docker.expectedTopology();
    expect(afterCreate.edges).toHaveLength(2);

    await page.getByTestId('refresh-button').click();
    await expectGraph(page, afterCreate);
    await expect(listButton(page, 'container', second.id)).toBeVisible();

    docker.stop(second);
    const afterStop = docker.expectedTopology();
    expect(afterStop.edges).toEqual([`${first.id}->${network.id}`]);
    expect(
      afterStop.containers.find((container) => container.id === second.id)
        ?.status,
    ).toContain('Exited');

    await page.getByTestId('refresh-button').click();
    await expectGraph(page, afterStop);
    await listButton(page, 'container', second.id).click();
    await expect(page.getByTestId('details-container-status')).toHaveText(
      /Exited/,
    );

    docker.removeContainer(second);
    const afterRemove = docker.expectedTopology();
    expect(
      afterRemove.containers.some((container) => container.id === second.id),
    ).toBe(false);

    await page.getByTestId('refresh-button').click();
    await expectGraph(page, afterRemove);
    await expect(listButton(page, 'container', second.id)).toHaveCount(0);
  });

  test('automatic polling discovers a new container without a manual refresh', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('polling');
    const first = await docker.createContainer('first-polled', {
      networks: [network.name],
    });

    const initial = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, initial);
    await expect(page.getByTestId('refresh-interval')).toHaveText(
      'Automatic refresh every 5 seconds',
    );

    const before = await page
      .getByTestId('last-success-timestamp')
      .locator('time')
      .getAttribute('datetime');
    expect(before).not.toBeNull();

    const second = await docker.createContainer('second-polled', {
      networks: [network.name],
    });
    const afterCreate = docker.expectedTopology();
    expect(afterCreate.edges).toHaveLength(2);

    await expectGraph(page, afterCreate);
    await expect(listButton(page, 'container', second.id)).toBeVisible();
    await expect(listButton(page, 'container', first.id)).toBeVisible();
    await expect
      .poll(
        async () =>
          page
            .getByTestId('last-success-timestamp')
            .locator('time')
            .getAttribute('datetime'),
        { timeout: 20_000 },
      )
      .not.toBe(before);
  });
});
