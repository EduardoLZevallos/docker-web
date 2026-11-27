import { expect, test, type Page } from '@playwright/test';

const NETWORK_A_ID = 'a'.repeat(64);
const NETWORK_B_ID = 'b'.repeat(64);
const NETWORK_INVALID_ID = 'c'.repeat(64);
const CONTAINER_A_ID = '1'.repeat(64);
const CONTAINER_B_ID = '2'.repeat(64);

const topology = {
  containers: [
    {
      id: CONTAINER_A_ID,
      name: 'mock-alpha-container',
      image: 'alpine:3.20',
      status: 'Up 1 minute',
      state: 'running',
      created: 1_700_000_000,
      ports: [],
      networks: {
        'mock-alpha': {
          network_id: NETWORK_A_ID,
          ipv4_address: '10.0.0.2',
          ipv6_address: null,
          ip_address: '10.0.0.2',
          mac_address: '02:42:0a:00:00:02',
        },
      },
    },
    {
      id: CONTAINER_B_ID,
      name: 'mock-beta-container',
      image: 'alpine:3.20',
      status: 'Up 1 minute',
      state: 'running',
      created: 1_700_000_000,
      ports: [],
      networks: {
        'mock-beta': {
          network_id: NETWORK_B_ID,
          ipv4_address: '10.0.0.3',
          ipv6_address: null,
          ip_address: '10.0.0.3',
          mac_address: '02:42:0a:00:00:03',
        },
      },
    },
  ],
  networks: [
    {
      id: NETWORK_A_ID,
      name: 'mock-alpha',
      driver: 'bridge',
      scope: 'local',
      subnet: '10.0.0.0/24',
      ipam_configs: [
        { subnet: '10.0.0.0/24', gateway: '10.0.0.1' },
        { subnet: 'fd00::/64', gateway: 'fd00::1' },
      ],
      internal: false,
      containers: [],
    },
    {
      id: NETWORK_B_ID,
      name: 'mock-beta',
      driver: 'bridge',
      scope: 'local',
      subnet: '10.0.0.0/24',
      ipam_configs: [{ subnet: '10.0.0.0/24', gateway: '10.0.0.1' }],
      internal: false,
      containers: [],
    },
    {
      id: NETWORK_INVALID_ID,
      name: 'mock-invalid-ipam',
      driver: 'bridge',
      scope: 'local',
      subnet: 'not-a-cidr',
      ipam_configs: [{ subnet: 'not-a-cidr', gateway: null }],
      internal: false,
      containers: [],
    },
  ],
  edges: [
    { source: CONTAINER_A_ID, target: NETWORK_A_ID },
    { source: CONTAINER_B_ID, target: NETWORK_B_ID },
  ],
  timestamp: '2026-11-27T12:00:00Z',
};

async function mockApi(
  page: Page,
  options: { failTopologyAfter?: number } = {},
): Promise<void> {
  let topologyRequests = 0;
  await page.route('**/api/**', async (route) => {
    const pathname = new URL(route.request().url()).pathname;
    if (pathname === '/api/config') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          theme: 'light',
          demo_mode: false,
          discovery_interval: 30,
        }),
      });
      return;
    }
    if (pathname === '/api/topology') {
      topologyRequests += 1;
      if (
        options.failTopologyAfter !== undefined &&
        topologyRequests > options.failTopologyAfter
      ) {
        await route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({ error: 'mocked topology outage' }),
        });
        return;
      }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(topology),
      });
      return;
    }
    await route.abort();
  });
}

test.describe('mocked API only: impossible Docker subnet overlap', () => {

  test('labels the mock, shows overlap coverage, and clears filters when a finding selects either network', async ({
    page,
  }) => {
    await mockApi(page);
    await page.goto('/');
    await expect(page.getByTestId('overlap-panel')).toBeVisible();
    await expect(page.getByTestId('overlap-badge')).toHaveText('Overlap found');
    await expect(page.getByTestId('overlap-summary')).toHaveText(
      '1 overlapping CIDR pair across 2 evaluated networks. 1 network has incomplete CIDR coverage.',
    );

    await page.getByTestId('overlap-toggle').click();
    await expect(page.getByTestId('overlap-body')).toBeVisible();
    await expect(page.getByTestId('overlap-note')).toContainText(
      'does not prove routing, reachability, or traffic',
    );

    const findings = page.getByTestId('overlap-finding');
    await expect(findings).toHaveCount(1);
    await expect(findings).toHaveAttribute('data-family', 'ipv4');
    await expect(findings).toHaveAttribute('data-relation', 'equal');
    await expect(findings).toContainText('mock-alpha');
    await expect(findings).toContainText('mock-beta');
    await expect(findings).toContainText('10.0.0.0/24');

    const invalidCoverage = page.getByTestId(
      `overlap-coverage-network-${NETWORK_INVALID_ID}`,
    );
    await expect(invalidCoverage).toHaveAttribute(
      'data-coverage-status',
      'invalid',
    );
    await expect(invalidCoverage).toContainText('not-a-cidr');
    await expect(invalidCoverage).toContainText('was not compared');

    await page.getByTestId('search-input').fill('definitely-not-present');
    await page.getByTestId('network-filter').selectOption(NETWORK_A_ID);
    await page.getByTestId('published-filter').check();
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 0',
    );

    const selectBeta = findings.locator(
      `[data-network-id="${NETWORK_B_ID}"]`,
    );
    await selectBeta.click();

    await expect(page.getByTestId('search-input')).toHaveValue('');
    await expect(page.getByTestId('network-filter')).toHaveValue('');
    await expect(page.getByTestId('published-filter')).not.toBeChecked();
    await expect(page.getByTestId('details-network-id')).toHaveText(
      NETWORK_B_ID,
    );
    await expect(
      page.locator(`[data-testid="node-select-network-${NETWORK_B_ID}"]`),
    ).toHaveAttribute('aria-pressed', 'true');
    await expect(selectBeta).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 2',
    );

    const selectAlpha = findings.locator(
      `[data-network-id="${NETWORK_A_ID}"]`,
    );
    await selectAlpha.click();
    await expect(page.getByTestId('details-network-id')).toHaveText(
      NETWORK_A_ID,
    );
    await expect(
      page.locator(`[data-testid="node-select-network-${NETWORK_A_ID}"]`),
    ).toHaveAttribute('aria-pressed', 'true');
    await expect(selectAlpha).toHaveAttribute('aria-pressed', 'true');

    const screenshotPath = process.env.OVERLAP_SCREENSHOT_PATH?.trim();
    if (screenshotPath) {
      await page.screenshot({ path: screenshotPath, fullPage: true });
    }
  });
  test('keeps overlap findings visible and marks them stale after a topology refresh failure', async ({
    page,
  }) => {
    await mockApi(page, { failTopologyAfter: 1 });
    await page.goto('/');
    await expect(page.getByTestId('overlap-panel')).toBeVisible();
    await expect(page.getByTestId('overlap-badge')).toHaveText('Overlap found');
    await page.getByTestId('overlap-toggle').click();
    await expect(page.getByTestId('overlap-finding')).toHaveCount(1);

    await page.getByTestId('refresh-button').click();
    await expect(page.getByTestId('topology-error')).toContainText(
      'The last successful topology remains visible.',
    );
    await expect(page.getByTestId('overlap-stale')).toBeVisible();
    await expect(page.getByTestId('overlap-panel')).toHaveAttribute(
      'data-stale',
      'true',
    );
    await expect(page.getByTestId('overlap-badge')).toHaveText('Overlap found');
    await expect(page.getByTestId('overlap-finding')).toHaveCount(1);
  });
});
