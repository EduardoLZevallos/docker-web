import { expect, test } from './support/test';
import {
  expectedAttachments,
  expectedContainer,
  expectedNetwork,
  expectedNetworkCidrs,
  expectedPortBindings,
  filterExpectedTopology,
} from './support/fixtures';
import {
  expectContainerDetails,
  expectGraph,
  listButton,
  openApp,
} from './support/ui';

test.describe('network membership and published-binding filters', () => {
  test('filters by stable network ID and keeps an empty selected network visible', async ({
    page,
    docker,
  }) => {
    const alpha = await docker.createNetwork('filter-alpha');
    const beta = await docker.createNetwork('filter-beta');
    const empty = await docker.createNetwork('filter-empty', { internal: true });

    const alphaOne = await docker.createContainer('alpha-one', {
      networks: [alpha.name],
    });
    const alphaTwo = await docker.createContainer('alpha-two', {
      networks: [alpha.name],
    });
    const betaOne = await docker.createContainer('beta-one', {
      networks: [beta.name],
    });

    const expected = docker.expectedTopology();
    await openApp(page);
    await expectGraph(page, expected);

    const networkFilter = page.getByTestId('network-filter');
    await expect(
      networkFilter.locator(`option[value="${alpha.id}"]`),
    ).toContainText(alpha.id);
    await expect(
      networkFilter.locator(`option[value="${empty.id}"]`),
    ).toContainText(empty.id);

    await networkFilter.selectOption(alpha.id);
    await expect(page.getByTestId('network-filter-id')).toHaveText(
      `Stable network ID: ${alpha.id}`,
    );
    await expectGraph(
      page,
      filterExpectedTopology(expected, '', { networkId: alpha.id }),
    );
    await expect(networkFilter).toHaveValue(alpha.id);
    await expect(page.getByTestId('count-total-containers')).toHaveText(
      `Containers total: ${expected.containers.length}`,
    );
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 2',
    );
    await expect(listButton(page, 'container', alphaOne.id)).toBeVisible();
    await expect(listButton(page, 'container', alphaTwo.id)).toBeVisible();
    await expect(listButton(page, 'container', betaOne.id)).toHaveCount(0);

    await networkFilter.selectOption(empty.id);
    await expect(page.getByTestId('network-filter-id')).toHaveText(
      `Stable network ID: ${empty.id}`,
    );
    await expectGraph(page, {
      nodes: [`network:${empty.id}`],
      edges: [],
    });
    await expect(page.getByTestId('search-no-results')).toHaveCount(0);
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      'Containers visible: 0',
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      'Networks visible: 1',
    );
    await expect(listButton(page, 'network', empty.id)).toBeVisible();

    await listButton(page, 'network', empty.id).click();
    await expect(page.getByTestId('details-network-id')).toHaveText(empty.id);
    await expect(
      page.getByTestId('details-network-member-empty'),
    ).toBeVisible();
    await expect(page.getByTestId('details-network-membership-note')).toContainText(
      'Docker network membership only',
    );

    await page.getByTestId('filter-reset').click();
    await expect(networkFilter).toHaveValue('');
    await expectGraph(page, expected);
  });

  test('composes network and published-binding filters with the existing search', async ({
    page,
    docker,
  }) => {
    const frontend = await docker.createNetwork('compose-frontend');
    const backend = await docker.createNetwork('compose-backend');

    const webPublished = await docker.createContainer('web-published', {
      networks: [frontend.name],
      publishPort: 8080,
    });
    const webPrivate = await docker.createContainer('web-private', {
      networks: [frontend.name],
    });
    const apiPublished = await docker.createContainer('api-published', {
      networks: [backend.name],
      bindings: [
        {
          hostIp: '0.0.0.0',
          hostPort: 0,
          containerPort: 9090,
        },
      ],
    });
    const apiPaused = await docker.createContainer('api-paused', {
      networks: [backend.name],
      pause: true,
      bindings: [
        {
          hostIp: '127.0.0.1',
          hostPort: 0,
          containerPort: 9091,
        },
      ],
    });
    const apiExited = await docker.createContainer('api-exited', {
      networks: [backend.name],
      command: ['sh', '-c', 'exit 3'],
      waitForState: 'exited',
    });

    const expected = docker.expectedTopology();
    expect(
      expectedPortBindings(expectedContainer(expected, apiPaused.id)).some(
        (binding) => binding.public !== null,
      ),
    ).toBe(true);

    const snapshot = await openApp(page);
    await expectGraph(page, expected);
    await expect(
      page.getByTestId('graph-membership-note'),
    ).toContainText('not traffic or reachability');

    await page.getByTestId('published-filter').check();
    await expectGraph(
      page,
      filterExpectedTopology(expected, '', { publishedOnly: true }),
    );
    await expect(listButton(page, 'container', webPublished.id)).toBeVisible();
    await expect(listButton(page, 'container', apiPublished.id)).toBeVisible();
    await expect(listButton(page, 'container', apiPaused.id)).toBeVisible();
    await expect(listButton(page, 'container', webPrivate.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiExited.id)).toHaveCount(0);
    await page.getByTestId('published-filter').uncheck();
    await expectGraph(page, expected);

    await page.getByTestId('search-input').fill('web');
    await expectGraph(
      page,
      filterExpectedTopology(expected, 'web', { networkId: frontend.id }),
    );

    await page.getByTestId('network-filter').selectOption(frontend.id);
    await expectGraph(
      page,
      filterExpectedTopology(expected, 'web', { networkId: frontend.id }),
    );
    await expect(listButton(page, 'container', webPublished.id)).toBeVisible();
    await expect(listButton(page, 'container', webPrivate.id)).toBeVisible();

    await page.getByTestId('published-filter').check();
    await expectGraph(
      page,
      filterExpectedTopology(expected, 'web', {
        networkId: frontend.id,
        publishedOnly: true,
      }),
    );
    await expect(listButton(page, 'container', webPublished.id)).toBeVisible();
    await expect(listButton(page, 'container', webPrivate.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiPublished.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiPaused.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiExited.id)).toHaveCount(0);
    await expect(page.getByTestId('search-summary')).toContainText(
      `network ${frontend.name} (${frontend.id})`,
    );
    await expect(page.getByTestId('search-summary')).toContainText(
      'published bindings',
    );
    await expect(page.getByTestId('search-summary')).toContainText(
      'search "web"',
    );

    await page.getByTestId('network-filter').selectOption(backend.id);
    await expectGraph(
      page,
      filterExpectedTopology(expected, 'web', {
        networkId: backend.id,
        publishedOnly: true,
      }),
    );
    await expect(listButton(page, 'container', apiPublished.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiPaused.id)).toHaveCount(0);
    await expect(listButton(page, 'container', apiExited.id)).toHaveCount(0);
    await expect(
      page.getByTestId('network-list-empty'),
    ).toHaveCount(0);

    await page.getByTestId('filter-reset').click();
    await expect(page.getByTestId('search-input')).toHaveValue('');
    await expect(page.getByTestId('published-filter')).not.toBeChecked();
    await expectGraph(page, expected);
    expect(snapshot.nodeCount).toBe(expected.nodes.length);
  });

  test('shows real per-IP metadata, every IPAM CIDR, and exact host binding classes', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('binding-details', {
      ipv6: true,
      subnets: ['10.42.0.0/24', 'fd42::/64'],
    });
    const bindings = [
      { hostIp: '127.0.0.1', hostPort: 0, containerPort: 8080 },
      { hostIp: '127.10.20.30', hostPort: 0, containerPort: 8081 },
      { hostIp: '0.0.0.0', hostPort: 0, containerPort: 8082 },
      { hostIp: '::1', hostPort: 0, containerPort: 8083 },
    ];
    const container = await docker.createContainer('binding-container', {
      networks: [network.name],
      bindings,
    });

    const expected = docker.expectedTopology();
    const expectedContainerRow = expectedContainer(expected, container.id);
    const expectedNetworkRow = expectedNetwork(expected, network.id);
    const portBindings = expectedPortBindings(expectedContainerRow);
    const attachments = expectedAttachments(expectedContainerRow, expected.networks);
    expect(
      attachments.some((attachment) => attachment.ipv6Address.length > 0),
    ).toBe(true);
    const kinds = new Set(portBindings.map((binding) => binding.kind));
    expect(kinds).toContain('loopback');
    expect(kinds).toContain('wildcard');
    expect(portBindings.some((binding) => binding.hostIp === '::1')).toBe(true);

    await openApp(page);
    await expectGraph(page, expected);

    await listButton(page, 'container', container.id).click();
    const createdSeconds = Math.floor(
      Date.parse(expectedContainerRow.inspect.Created) / 1000,
    );
    await expectContainerDetails(page, {
      id: container.id,
      name: container.name,
      image: expectedContainerRow.inspect.Config?.Image ?? 'alpine:3.20',
      state: 'running',
      statusPattern: /^Up /,
      createdIso: new Date(createdSeconds * 1000).toISOString(),
      createdSeconds,
      portBindings,
      attachments,
      currentBindingCount: portBindings.filter(
        (binding) => binding.public !== null,
      ).length,
    });

    const ipv6Binding = portBindings.find(
      (binding) => binding.hostIp === '::1',
    );
    if (!ipv6Binding) {
      throw new Error('Docker did not report the IPv6 loopback binding');
    }
    expect(ipv6Binding.binding).toContain('[::1]');
    await expect(page.getByTestId('details-container-ports')).toContainText(
      '[::1]',
    );

    await listButton(page, 'network', network.id).click();
    const cidrs = expectedNetworkCidrs(expectedNetworkRow);
    expect(cidrs.map((cidr) => cidr.subnet).sort()).toEqual(
      ['10.42.0.0/24', 'fd42::/64'].sort(),
    );
    await expect(
      page.locator('[data-testid^="details-network-cidr-"]'),
    ).toHaveCount(cidrs.length);
    for (const [index, cidr] of cidrs.entries()) {
      await expect(
        page.getByTestId(`details-network-cidr-${index}`),
      ).toContainText(cidr.subnet);
      await expect(
        page.getByTestId(`details-network-cidr-${index}`),
      ).toContainText(`Gateway: ${cidr.gateway || '—'}`);
    }
    await expect(page.getByTestId('details-network-id')).toHaveText(network.id);
  });

  test('keeps inactive attachments and ports as retained metadata without current bindings', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('inactive-metadata');
    const exited = await docker.createContainer('exited-retained', {
      networks: [network.name],
      command: ['sh', '-c', 'exit 4'],
      waitForState: 'exited',
    });

    const expected = docker.expectedTopology();
    const expectedRow = expectedContainer(expected, exited.id);
    const portBindings = expectedPortBindings(expectedRow);
    expect(portBindings).toEqual([]);
    expect(expected.edges).toEqual([]);

    await openApp(page);
    await expectGraph(page, expected);

    await listButton(page, 'container', exited.id).click();
    const createdSeconds = Math.floor(
      Date.parse(expectedRow.inspect.Created) / 1000,
    );
    await expectContainerDetails(page, {
      id: exited.id,
      name: exited.name,
      image: expectedRow.inspect.Config?.Image ?? 'alpine:3.20',
      state: 'exited',
      statusPattern: /^Exited \(4\)/,
      createdIso: new Date(createdSeconds * 1000).toISOString(),
      createdSeconds,
      portBindings,
      attachments: expectedAttachments(expectedRow, expected.networks),
      currentBindingCount: 0,
    });
    await expect(
      page.getByTestId('details-container-binding-state'),
    ).toContainText('retained Docker metadata only');
    await expect(
      page.getByTestId('details-container-binding-count'),
    ).toContainText('not a listening or reachability check');
    for (const attachment of expectedAttachments(expectedRow, expected.networks)) {
      await expect(
        page.getByTestId(
          `details-container-network-membership-${attachment.name}`,
        ),
      ).toContainText('retained attachment metadata only');
    }
    await expect(
      page.getByTestId('details-container-membership-note'),
    ).toContainText('do not prove');
  });
});
