import { expect, test } from './support/test';
import {
  expectedAttachments,
  expectedContainer,
  expectedEdgeInternalIds,
  expectedGraphLabel,
  expectedHighlightedNetworkNodes,
  expectedHighlightedNodes,
  expectedNetwork,
  expectedNetworkCidrs,
  expectedNetworkEdgeInternalIds,
  expectedPortBindings,
} from './support/fixtures';
import {
  clickGraphBackground,
  clickGraphNode,
  detailsPanel,
  expectContainerDetails,
  expectGraph,
  expectNoSelection,
  expectSelection,
  graphNode,
  listButton,
  openApp,
  readGraphSnapshot,
} from './support/ui';

test.describe('real Docker topology rendered through the real backend', () => {
  test('renders 2, 3, and 4 container networks with nine inspected edges', async ({
    page,
    docker,
  }) => {
    const two = await docker.createNetwork('two-containers');
    const three = await docker.createNetwork('three-containers');
    const four = await docker.createNetwork('four-containers');

    for (let index = 1; index <= 2; index += 1) {
      await docker.createContainer(`two-${index}`, {
        networks: [two.name],
      });
    }
    for (let index = 1; index <= 3; index += 1) {
      await docker.createContainer(`three-${index}`, {
        networks: [three.name],
      });
    }
    for (let index = 1; index <= 4; index += 1) {
      await docker.createContainer(`four-${index}`, {
        networks: [four.name],
      });
    }

    const expected = docker.expectedTopology();
    expect(expected.edges).toHaveLength(9);
    expect(expected.containers).toHaveLength(9);

    const snapshot = await openApp(page);
    await expectGraph(page, expected);

    await expect(page.getByTestId('count-total-containers')).toHaveText(
      `Containers total: ${expected.containers.length}`,
    );
    await expect(page.getByTestId('count-visible-containers')).toHaveText(
      `Containers visible: ${expected.containers.length}`,
    );
    await expect(page.getByTestId('count-total-networks')).toHaveText(
      `Networks total: ${expected.networks.length}`,
    );
    await expect(page.getByTestId('count-visible-networks')).toHaveText(
      `Networks visible: ${expected.networks.length}`,
    );

    expect(snapshot.edgeCount).toBe(9);
    for (const container of expected.containers) {
      expect(graphNode(snapshot, container.id).label).toBe(
        expectedGraphLabel(container.name, container.id),
      );
    }
    for (const network of [two, three, four]) {
      const expectedNetworkRow = expectedNetwork(expected, network.id);
      expect(graphNode(snapshot, network.id).label).toBe(
        expectedGraphLabel(expectedNetworkRow.name, network.id),
      );
    }
  });

  test('keeps an unattached network visible and connects a multi-homed container to both networks', async ({
    page,
    docker,
  }) => {
    const shared = await docker.createNetwork('shared');
    const extra = await docker.createNetwork('extra');
    const idle = await docker.createNetwork('unattached', { internal: true });

    const single = await docker.createContainer('single-homed', {
      networks: [shared.name],
    });
    const multi = await docker.createContainer('multi-homed', {
      networks: [shared.name, extra.name],
    });

    const expected = docker.expectedTopology();
    expect(expected.edges).toHaveLength(3);
    expect(
      expected.edges.some((edge) => edge.startsWith(`${multi.id}->`)),
    ).toBe(true);
    expect(
      expected.edges.some(
        (edge) =>
          edge.includes(idle.id) ||
          edge.startsWith(`${idle.id}->`) ||
          edge.endsWith(`->${idle.id}`),
      ),
    ).toBe(false);

    let snapshot = await openApp(page);
    snapshot = await expectGraph(page, expected);

    const idleNode = graphNode(snapshot, idle.id);
    expect(idleNode.kind).toBe('network');
    expect(
      snapshot.edges.some((edge) => edge.rawTargetId === idle.id),
    ).toBe(false);
    expect(idleNode.classes).not.toContain('selected');

    await clickGraphNode(page, graphNode(snapshot, multi.id));
    await expectSelection(page, {
      selected: [multi.id],
      highlighted: expectedHighlightedNodes(expected, multi.id),
      highlightedEdges: expectedEdgeInternalIds(expected, multi.id),
    });

    const multiDetails = detailsPanel(page);
    await expect(multiDetails).toContainText(multi.name);
    for (const attachment of expectedAttachments(
      expectedContainer(expected, multi.id),
      expected.networks,
    )) {
      await expect(
        page.locator(
          `[data-testid="details-container-network-${attachment.name}"]`,
        ),
      ).toBeVisible();
      await expect(
        page.getByTestId(`details-container-network-ipv4-${attachment.name}`),
      ).toHaveText(`IPv4: ${attachment.ipv4Address}`);
      await expect(
        page.getByTestId(`details-container-network-ipv6-${attachment.name}`),
      ).toHaveText(`IPv6: ${attachment.ipv6Address || '—'}`);
    }

    await listButton(page, 'network', idle.id).click();
    await expectSelection(page, {
      selected: [idle.id],
      highlighted: [idle.id],
      highlightedEdges: [],
    });
    const idleDetails = detailsPanel(page);
    await expect(page.getByTestId('details-network-id')).toHaveText(idle.id);
    await expect(page.getByTestId('details-network-internal')).toHaveText(
      'Yes',
    );
    await expect(
      page.getByTestId('details-network-member-empty'),
    ).toBeVisible();

    expect(single.id).not.toBe(multi.id);
  });

  test('keeps created and exited containers visible without attachment edges', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('inactive');
    const created = await docker.createContainer('created', {
      networks: [network.name],
      start: false,
    });
    const exited = await docker.createContainer('exited', {
      networks: [network.name],
      command: ['sh', '-c', 'exit 7'],
      waitForState: 'exited',
    });
    const running = await docker.createContainer('running', {
      networks: [network.name],
    });

    const exitedInspect = docker.inspectContainer(exited.id);
    expect(exitedInspect.State?.Status).toBe('exited');
    expect(exitedInspect.State?.ExitCode).toBe(7);

    const expected = docker.expectedTopology();
    expect(expected.edges).toEqual([`${running.id}->${network.id}`]);
    expect(expected.containers.map((container) => container.id).sort()).toEqual(
      [created.id, exited.id, running.id].sort(),
    );

    const snapshot = await openApp(page);
    await expectGraph(page, expected);
    expect(graphNode(snapshot, created.id).kind).toBe('container');
    expect(graphNode(snapshot, exited.id).kind).toBe('container');

    await listButton(page, 'container', exited.id).click();
    await expect(page.getByTestId('details-container-status')).toHaveText(
      /Exited \(7\)/,
    );
    await expectSelection(page, {
      selected: [exited.id],
      highlighted: [exited.id],
      highlightedEdges: [],
    });

    await listButton(page, 'container', created.id).click();
    await expect(page.getByTestId('details-container-status')).toHaveText(
      /^Created$/,
    );
    await expectSelection(page, {
      selected: [created.id],
      highlighted: [created.id],
      highlightedEdges: [],
    });
  });

  test('keeps paused containers rendered with their network edges', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('paused');
    const paused = await docker.createContainer('paused-container', {
      networks: [network.name],
      pause: true,
    });
    const running = await docker.createContainer('running-peer', {
      networks: [network.name],
    });

    const expected = docker.expectedTopology();
    const pausedExpected = expectedContainer(expected, paused.id);
    expect(pausedExpected.inspect.State?.Status).toBe('paused');
    expect(pausedExpected.status).toContain('(Paused)');
    expect(expected.edges).toEqual(
      [`${paused.id}->${network.id}`, `${running.id}->${network.id}`].sort(),
    );

    const snapshot = await openApp(page);
    await expectGraph(page, expected);

    await clickGraphNode(page, graphNode(snapshot, paused.id));
    await expect(page.getByTestId('details-container-status')).toHaveText(
      /\(Paused\)/,
    );
    await expectSelection(page, {
      selected: [paused.id],
      highlighted: expectedHighlightedNodes(expected, paused.id),
      highlightedEdges: expectedEdgeInternalIds(expected, paused.id),
    });
  });

  test('canvas clicks highlight real nodes and show ports, IP, and MAC details', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('detail');
    const detailed = await docker.createContainer('published-details', {
      networks: [network.name],
      publishPort: 8080,
    });
    const peer = await docker.createContainer('peer-details', {
      networks: [network.name],
    });

    const expected = docker.expectedTopology();
    const detailedExpected = expectedContainer(expected, detailed.id);
    const networkExpected = expectedNetwork(expected, network.id);
    const portBindings = expectedPortBindings(detailedExpected);
    expect(portBindings.length).toBeGreaterThan(0);
    expect(portBindings.every((binding) => binding.public !== null)).toBe(true);
    expect(portBindings.map((binding) => binding.kind)).toContain('loopback');

    const createdSeconds = Math.floor(
      Date.parse(detailedExpected.inspect.Created) / 1000,
    );
    const subnet =
      networkExpected.inspect.IPAM?.Config?.[0]?.Subnet ?? '—';

    let snapshot = await openApp(page);
    snapshot = await expectGraph(page, expected);

    await clickGraphNode(page, graphNode(snapshot, detailed.id));
    await expectSelection(page, {
      selected: [detailed.id],
      highlighted: expectedHighlightedNodes(expected, detailed.id),
      highlightedEdges: expectedEdgeInternalIds(expected, detailed.id),
    });
    await expectContainerDetails(page, {
      id: detailed.id,
      name: detailed.name,
      image: detailedExpected.inspect.Config?.Image ?? 'alpine:3.20',
      state: detailedExpected.inspect.State?.Status ?? null,
      statusPattern: /^Up /,
      createdIso: new Date(createdSeconds * 1000).toISOString(),
      createdSeconds,
      portBindings,
      attachments: expectedAttachments(detailedExpected, expected.networks),
      currentBindingCount: portBindings.filter(
        (binding) => binding.public !== null,
      ).length,
    });

    const nextSnapshot = await readGraphSnapshot(page);
    if (!nextSnapshot) {
      throw new Error('graph bridge disappeared before the second canvas click');
    }
    snapshot = nextSnapshot;
    await clickGraphNode(page, graphNode(snapshot, network.id));
    await expectSelection(page, {
      selected: [network.id],
      highlighted: expectedHighlightedNetworkNodes(expected, network.id),
      highlightedEdges: expectedNetworkEdgeInternalIds(expected, network.id),
    });

    await expect(page.getByTestId('details-network-id')).toHaveText(network.id);
    await expect(page.getByTestId('details-network-driver')).toHaveText(
      networkExpected.inspect.Driver ?? 'bridge',
    );
    await expect(page.getByTestId('details-network-scope')).toHaveText(
      networkExpected.inspect.Scope ?? 'local',
    );
    await expect(page.getByTestId('details-network-subnet')).toHaveText(
      subnet,
    );
    const cidrs = expectedNetworkCidrs(networkExpected);
    for (const [index, cidr] of cidrs.entries()) {
      await expect(
        page.getByTestId(`details-network-cidr-${index}`),
      ).toContainText(cidr.subnet || '—');
      await expect(
        page.getByTestId(`details-network-cidr-${index}`),
      ).toContainText(`Gateway: ${cidr.gateway || '—'}`);
    }
    await expect(page.getByTestId('details-network-internal')).toHaveText(
      networkExpected.inspect.Internal ? 'Yes' : 'No',
    );

    for (const member of [detailed, peer]) {
      const memberItem = page.locator(
        `[data-testid="details-network-member-${member.id}"]`,
      );
      await expect(memberItem).toBeVisible();
      await expect(memberItem).toContainText(member.name);
    }

    await clickGraphBackground(page);
    await expectNoSelection(page);
    await expect(page.getByTestId('details-empty')).toBeVisible();
  });

  test('shows nullable metadata for a created container and a real click on the built-in none network', async ({
    page,
    docker,
  }) => {
    const network = await docker.createNetwork('nullable');
    const created = await docker.createContainer('created-nullable', {
      networks: [network.name],
      start: false,
    });

    const expected = docker.expectedTopology();
    const createdExpected = expectedContainer(expected, created.id);
    expect(createdExpected.inspect.State?.Status).toBe('created');
    expect(expected.edges).toEqual([]);
    expect(expectedPortBindings(createdExpected)).toEqual([]);

    const noneNetwork = expected.networks.find(
      (candidate) => candidate.name === 'none',
    );
    if (!noneNetwork) {
      throw new Error('the isolated Docker daemon did not expose the none network');
    }
    const noneSubnet = noneNetwork.inspect.IPAM?.Config?.[0]?.Subnet ?? null;
    expect(noneSubnet).toBeNull();

    const createdSeconds = Math.floor(
      Date.parse(createdExpected.inspect.Created) / 1000,
    );

    let snapshot = await openApp(page);
    snapshot = await expectGraph(page, expected);

    await clickGraphNode(page, graphNode(snapshot, created.id));
    await expect(page.getByTestId('details-container-status')).toHaveText(
      /^Created$/,
    );
    await expect(page.getByTestId('details-container-ports')).toContainText(
      'None',
    );
    await expectSelection(page, {
      selected: [created.id],
      highlighted: [created.id],
      highlightedEdges: [],
    });
    await expectContainerDetails(page, {
      id: created.id,
      name: created.name,
      image: createdExpected.inspect.Config?.Image ?? 'alpine:3.20',
      state: 'created',
      statusPattern: /^Created$/,
      createdIso: new Date(createdSeconds * 1000).toISOString(),
      createdSeconds,
      portBindings: [],
      attachments: expectedAttachments(createdExpected, expected.networks),
      currentBindingCount: 0,
    });
    for (const attachment of expectedAttachments(createdExpected, expected.networks)) {
      const item = page.locator(
        `[data-testid="details-container-network-${attachment.name}"]`,
      );
      await expect(item).toContainText('IPv4: —');
      await expect(item).toContainText('IPv6: —');
      await expect(item).toContainText('MAC: —');
      await expect(
        page.getByTestId(
          `details-container-network-membership-${attachment.name}`,
        ),
      ).toContainText('retained attachment metadata only');
    }

    const nextSnapshot = await readGraphSnapshot(page);
    if (!nextSnapshot) {
      throw new Error('graph bridge disappeared before the none-network click');
    }
    snapshot = nextSnapshot;
    await clickGraphNode(page, graphNode(snapshot, noneNetwork.id));
    await expectSelection(page, {
      selected: [noneNetwork.id],
      highlighted: [noneNetwork.id],
      highlightedEdges: [],
    });
    await expect(page.getByTestId('details-network-id')).toHaveText(
      noneNetwork.id,
    );
    await expect(page.getByTestId('details-network-driver')).toHaveText(
      noneNetwork.inspect.Driver ?? 'null',
    );
    await expect(page.getByTestId('details-network-scope')).toHaveText(
      noneNetwork.inspect.Scope ?? 'local',
    );
    await expect(page.getByTestId('details-network-subnet')).toHaveText('—');
    await expect(page.getByTestId('details-network-internal')).toHaveText(
      noneNetwork.inspect.Internal ? 'Yes' : 'No',
    );
    await expect(
      page.getByTestId('details-network-member-empty'),
    ).toBeVisible();
  });
});
