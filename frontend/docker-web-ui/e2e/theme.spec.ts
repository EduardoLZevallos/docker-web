import { expect, test } from './support/test';
import { expectedHighlightedNodes, expectedEdgeInternalIds } from './support/fixtures';
import {
  expectGraph,
  expectNoHorizontalOverflow,
  expectSelection,
  listButton,
  openApp,
  readGraphSnapshot,
} from './support/ui';

test.describe('theme, keyboard, fit, and responsive behavior', () => {
  test('persists a keyboard-toggled theme across a reload', async ({
    page,
    docker,
  }) => {
    await docker.createNetwork('theme');
    const app = page.getByTestId('app-root');
    const toggle = page.getByTestId('theme-toggle');

    await openApp(page);
    const initialTheme = await app.getAttribute('data-theme');
    expect(initialTheme === 'light' || initialTheme === 'dark').toBe(true);
    const toggledTheme = initialTheme === 'dark' ? 'light' : 'dark';

    await expect(toggle).toHaveAttribute(
      'aria-pressed',
      initialTheme === 'dark' ? 'true' : 'false',
    );

    await toggle.focus();
    await page.keyboard.press('Space');
    await expect(app).toHaveAttribute('data-theme', toggledTheme);
    await expect(toggle).toHaveAttribute(
      'aria-pressed',
      toggledTheme === 'dark' ? 'true' : 'false',
    );
    await expect
      .poll(async () =>
        page.evaluate(() => window.localStorage.getItem('docker-web-theme')),
      )
      .toBe(toggledTheme);

    await page.reload();
    await expect(app).toBeVisible();
    await expect(app).toHaveAttribute('data-theme', toggledTheme);
    await expect(toggle).toHaveAttribute(
      'aria-pressed',
      toggledTheme === 'dark' ? 'true' : 'false',
    );
    await expect
      .poll(async () =>
        page.evaluate(() => window.localStorage.getItem('docker-web-theme')),
      )
      .toBe(toggledTheme);
  });

  test('supports keyboard list selection and the fit control', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('keyboard');
    const container = await docker.createContainer('keyboard-container', {
      networks: [network.name],
    });

    const expected = docker.expectedTopology();
    const snapshot = await openApp(page);
    await expectGraph(page, expected);

    const button = listButton(page, 'container', container.id);
    await button.focus();
    await page.keyboard.press('Enter');
    await expect(button).toHaveAttribute('aria-pressed', 'true');
    await expectSelection(page, {
      selected: [container.id],
      highlighted: expectedHighlightedNodes(expected, container.id),
      highlightedEdges: expectedEdgeInternalIds(expected, container.id),
    });
    await expect(page.getByTestId('details-container-id')).toHaveText(
      container.id,
    );

    const canvas = page.getByTestId('topology-graph');
    const box = await canvas.boundingBox();
    if (!box) {
      throw new Error('topology graph canvas has no bounding box');
    }
    const initialViewport = { zoom: snapshot.zoom, pan: snapshot.pan };
    const panStartX = box.x + 12;
    const panStartY = box.y + 12;
    await page.mouse.move(panStartX, panStartY);
    await page.mouse.down();
    await page.mouse.move(panStartX + 140, panStartY + 70, { steps: 10 });
    await page.mouse.up();

    await expect
      .poll(async () => (await readGraphSnapshot(page))?.pan, {
        timeout: 10_000,
      })
      .not.toEqual(initialViewport.pan);

    const fit = page.getByTestId('fit-graph-button');
    await fit.focus();
    await page.keyboard.press('Enter');
    await expect
      .poll(
        async () => {
          const current = await readGraphSnapshot(page);
          if (!current) {
            return false;
          }
          const nodesInsideCanvas = current.nodes.every(
            (node) =>
              node.renderedPosition.x >= 0 &&
              node.renderedPosition.y >= 0 &&
              node.renderedPosition.x <= current.canvas.width &&
              node.renderedPosition.y <= current.canvas.height,
          );
          const movedFromInitial =
            Math.abs(current.zoom - initialViewport.zoom) > 0.001 ||
            Math.abs(current.pan.x - initialViewport.pan.x) > 0.5 ||
            Math.abs(current.pan.y - initialViewport.pan.y) > 0.5;
          return nodesInsideCanvas && movedFromInitial;
        },
        { timeout: 10_000 },
      )
      .toBe(true);
    await expect(button).toHaveAttribute('aria-pressed', 'true');
  });

  test('keeps the graph, list, and details usable at a narrow viewport width', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('responsive');
    const container = await docker.createContainer('responsive-container', {
      networks: [network.name],
    });
    const expected = docker.expectedTopology();

    await page.setViewportSize({ width: 390, height: 844 });
    await openApp(page);
    await expectGraph(page, expected);
    await expectNoHorizontalOverflow(page);

    const graphBox = await page.getByTestId('topology-graph').boundingBox();
    expect(graphBox).not.toBeNull();
    expect(graphBox!.width).toBeGreaterThan(250);
    expect(graphBox!.width).toBeLessThanOrEqual(390);

    const aside = page.locator('aside[aria-label="Topology browser"]');
    const asideBox = await aside.boundingBox();
    expect(asideBox).not.toBeNull();
    expect(asideBox!.y).toBeGreaterThan(graphBox!.y + graphBox!.height);

    await expect(page.getByTestId('search-input')).toBeVisible();
    await expect(page.getByTestId('theme-toggle')).toBeVisible();
    await expect(page.getByTestId('details-panel')).toBeVisible();

    await listButton(page, 'container', container.id).click();
    await expect(listButton(page, 'container', container.id)).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await expect(page.getByTestId('details-container-id')).toHaveText(
      container.id,
    );
    await expectNoHorizontalOverflow(page);
  });
});
