import { expect, test } from './support/test';
import { expectNoHorizontalOverflow, openApp } from './support/ui';

test.describe('Docker-reported subnet overlap panel', () => {
  test('shows no false overlap findings for distinct adjacent real Docker CIDRs', async ({
    page,
    docker,
  }) => {
    await docker.createNetwork('overlap-alpha', {
      subnets: ['10.42.0.0/24'],
    });
    await docker.createNetwork('overlap-beta', {
      subnets: ['10.43.0.0/24'],
    });

    await openApp(page);

    const panel = page.getByTestId('overlap-panel');
    await expect(panel).toBeVisible();
    await expect(panel).toHaveAttribute('data-finding-count', '0');
    await expect(page.getByTestId('overlap-badge')).toHaveText('No overlaps');
    await expect(page.getByTestId('overlap-summary')).toContainText(
      'No overlapping Docker-reported CIDRs',
    );

    await page.getByTestId('overlap-toggle').click();
    await expect(page.getByTestId('overlap-empty')).toBeVisible();
    await expect(page.getByTestId('overlap-note')).toContainText(
      'Adjacent CIDRs are not overlaps',
    );
    await expect(page.getByTestId('overlap-note')).toContainText(
      'does not prove routing, reachability, or traffic',
    );

    await page.setViewportSize({ width: 390, height: 844 });
    await expectNoHorizontalOverflow(page);
    const panelBox = await panel.boundingBox();
    expect(panelBox).not.toBeNull();
    expect(panelBox!.width).toBeLessThanOrEqual(390);
    await expect(page.getByTestId('topology-graph')).toBeVisible();
  });
});
