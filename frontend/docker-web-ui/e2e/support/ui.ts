import { expect, type Locator, type Page } from '@playwright/test';
import type {
  ExpectedAttachment,
  ExpectedPortBinding,
  ExpectedTopology,
} from './fixtures';

export interface GraphNodeSnapshot {
  internalId: string;
  rawId: string;
  kind: 'container' | 'network';
  label: string;
  renderedPosition: { x: number; y: number };
  classes: string[];
}

export interface GraphEdgeSnapshot {
  internalId: string;
  rawSourceId: string;
  rawTargetId: string;
  sourceInternalId: string;
  targetInternalId: string;
}

export interface GraphSnapshot {
  ready: boolean;
  nodeCount: number;
  edgeCount: number;
  nodes: GraphNodeSnapshot[];
  edges: GraphEdgeSnapshot[];
  selectedNodeRawIds: string[];
  selectedNodeInternalIds: string[];
  highlightedNodeRawIds: string[];
  highlightedNodeInternalIds: string[];
  highlightedEdgeInternalIds: string[];
  zoom: number;
  pan: { x: number; y: number };
  canvas: { width: number; height: number };
}

declare global {
  interface Window {
    __DOCKER_WEB_GRAPH__?: {
      readonly version: 1;
      getSnapshot(): GraphSnapshot;
    };
  }
}

const GRAPH_TEST_ID = 'topology-graph';
const WAIT_TIMEOUT_MS = 30_000;

export async function readGraphSnapshot(
  page: Page,
): Promise<GraphSnapshot | null> {
  return page.evaluate(() => {
    const bridge = window.__DOCKER_WEB_GRAPH__;
    return bridge ? bridge.getSnapshot() : null;
  });
}

export async function waitForBridgeReady(page: Page): Promise<GraphSnapshot> {
  await page.waitForFunction(
    () => Boolean(window.__DOCKER_WEB_GRAPH__?.getSnapshot().ready),
    undefined,
    { timeout: WAIT_TIMEOUT_MS },
  );
  const snapshot = await readGraphSnapshot(page);
  if (!snapshot) {
    throw new Error('the E2E graph bridge disappeared after becoming ready');
  }
  return snapshot;
}

export async function openAppShell(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.getByTestId('app-root')).toBeVisible();
  await expect(page.getByTestId('loading-indicator')).toHaveCount(0, {
    timeout: WAIT_TIMEOUT_MS,
  });
}

export async function openApp(page: Page): Promise<GraphSnapshot> {
  await openAppShell(page);
  return waitForBridgeReady(page);
}

function snapshotShape(snapshot: GraphSnapshot) {
  return {
    ready: snapshot.ready,
    nodes: snapshot.nodes
      .map((node) => `${node.kind}:${node.rawId}`)
      .sort(),
    edges: snapshot.edges
      .map((edge) => `${edge.rawSourceId}->${edge.rawTargetId}`)
      .sort(),
  };
}

export async function expectGraph(
  page: Page,
  expected: Pick<ExpectedTopology, 'nodes' | 'edges'>,
): Promise<GraphSnapshot> {
  await expect
    .poll(
      async () => {
        const snapshot = await readGraphSnapshot(page);
        return snapshot ? snapshotShape(snapshot) : null;
      },
      {
        message: 'graph should match the topology derived from Docker',
        timeout: WAIT_TIMEOUT_MS,
      },
    )
    .toEqual({
      ready: true,
      nodes: [...expected.nodes].sort(),
      edges: [...expected.edges].sort(),
    });

  const snapshot = await readGraphSnapshot(page);
  if (!snapshot) {
    throw new Error('graph bridge is unavailable after matching Docker');
  }
  expect(snapshot.nodeCount).toBe(expected.nodes.length);
  expect(snapshot.edgeCount).toBe(expected.edges.length);
  return snapshot;
}

export function graphNode(snapshot: GraphSnapshot, rawId: string): GraphNodeSnapshot {
  const node = snapshot.nodes.find((candidate) => candidate.rawId === rawId);
  if (!node) {
    throw new Error(
      `graph node ${rawId} is not rendered; nodes=${snapshot.nodes
        .map((candidate) => `${candidate.kind}:${candidate.rawId}`)
        .join(',')}`,
    );
  }
  return node;
}

export async function clickGraphNode(
  page: Page,
  node: GraphNodeSnapshot,
): Promise<void> {
  const canvas = page.getByTestId(GRAPH_TEST_ID);

  for (let attempt = 1; attempt <= 3; attempt += 1) {
    const current = await readGraphSnapshot(page);
    const target = current?.nodes.find(
      (candidate) => candidate.rawId === node.rawId,
    );
    if (!target) {
      throw new Error(`graph node ${node.rawId} disappeared before it was clicked`);
    }

    const box = await canvas.boundingBox();
    if (!box) {
      throw new Error('topology graph canvas has no bounding box');
    }
    if (
      target.renderedPosition.x < 0 ||
      target.renderedPosition.y < 0 ||
      target.renderedPosition.x > box.width ||
      target.renderedPosition.y > box.height
    ) {
      throw new Error(
        `graph node ${target.rawId} is outside the canvas: ` +
          `position=${JSON.stringify(target.renderedPosition)} size=${box.width}x${box.height}`,
      );
    }

    await canvas.click({ position: target.renderedPosition });
    await expect
      .poll(
        async () =>
          (await readGraphSnapshot(page))?.selectedNodeRawIds.includes(
            target.rawId,
          ) ?? false,
        { timeout: 2_000 },
      )
      .toBe(true)
      .catch(() => undefined);
    if (
      (await readGraphSnapshot(page))?.selectedNodeRawIds.includes(target.rawId)
    ) {
      return;
    }
  }

  throw new Error(
    `clicking graph node ${node.rawId} did not select it after 3 attempts`,
  );
}

export async function clickGraphBackground(page: Page): Promise<void> {
  await page
    .getByTestId(GRAPH_TEST_ID)
    .click({ position: { x: 6, y: 6 } });
}

export function listButton(
  page: Page,
  kind: 'container' | 'network',
  rawId: string,
): Locator {
  return page.locator(`[data-testid="node-select-${kind}-${rawId}"]`);
}

export async function expectSelection(
  page: Page,
  expected: {
    selected: string[];
    highlighted: string[];
    highlightedEdges: string[];
  },
  timeout = WAIT_TIMEOUT_MS,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const snapshot = await readGraphSnapshot(page);
        if (!snapshot) {
          return null;
        }
        return {
          selected: [...snapshot.selectedNodeRawIds].sort(),
          highlighted: [...snapshot.highlightedNodeRawIds].sort(),
          highlightedEdges: [...snapshot.highlightedEdgeInternalIds].sort(),
        };
      },
      {
        message: 'graph selection and highlight classes should match',
        timeout,
      },
    )
    .toEqual({
      selected: [...expected.selected].sort(),
      highlighted: [...expected.highlighted].sort(),
      highlightedEdges: [...expected.highlightedEdges].sort(),
    });
}

export async function expectNoSelection(page: Page): Promise<void> {
  await expectSelection(page, {
    selected: [],
    highlighted: [],
    highlightedEdges: [],
  });
}

export function detailsPanel(page: Page): Locator {
  return page.getByTestId('details-panel');
}

function comparePortTuples(
  left: { text: string; kind: string },
  right: { text: string; kind: string },
): number {
  return (
    left.text.localeCompare(right.text) || left.kind.localeCompare(right.kind)
  );
}

export async function expectContainerDetails(
  page: Page,
  options: {
    id: string;
    name: string;
    image: string;
    state: string | null;
    statusPattern: RegExp;
    createdIso: string;
    createdSeconds: number;
    portBindings: ExpectedPortBinding[];
    attachments: ExpectedAttachment[];
    currentBindingCount: number;
  },
): Promise<void> {
  const panel = detailsPanel(page);
  await expect(panel.getByRole('heading', { level: 2 })).toHaveText(options.name);
  await expect(page.getByTestId('details-container-id')).toHaveText(options.id);
  await expect(page.getByTestId('details-container-image')).toHaveText(
    options.image,
  );
  await expect(page.getByTestId('details-container-state')).toHaveText(
    options.state ?? '—',
  );
  await expect(page.getByTestId('details-container-status')).toHaveText(
    options.statusPattern,
  );

  const created = page.getByTestId('details-container-created').locator('time');
  await expect(created).toHaveAttribute('datetime', options.createdIso);
  await expect(created).toHaveAttribute(
    'title',
    `${options.createdSeconds} Unix seconds`,
  );

  if (options.portBindings.length === 0) {
    await expect(page.getByTestId('details-container-ports')).toContainText(
      'None',
    );
  } else {
    const portItems = page.locator(
      '[data-testid^="details-container-port-"]',
    );
    const expectedPortTuples = options.portBindings
      .map((port) => ({
        text: `${port.mapping}${port.binding}`,
        kind: port.public === null ? 'not-published' : port.kind,
      }))
      .sort(comparePortTuples);
    await expect
      .poll(
        async () =>
          (
            await portItems.evaluateAll((items) =>
              items.map((item) => ({
                text: item.textContent?.trim() ?? '',
                kind: item.getAttribute('data-binding-kind') ?? '',
              })),
            )
          ).sort(comparePortTuples),
        {
          message:
            'rendered port rows should match Docker mappings and binding classes',
        },
      )
      .toEqual(expectedPortTuples);
  }
  await expect(
    page.getByTestId('details-container-binding-count'),
  ).toContainText(`Current host port bindings: ${options.currentBindingCount}`);

  for (const attachment of options.attachments) {
    const item = page.locator(
      `[data-testid="details-container-network-${attachment.name}"]`,
    );
    await expect(item).toContainText(attachment.name);
    await expect(
      page.getByTestId(`details-container-network-id-${attachment.name}`),
    ).toHaveText(`Network ID: ${attachment.networkId ?? '—'}`);
    await expect(
      page.getByTestId(`details-container-network-ipv4-${attachment.name}`),
    ).toHaveText(`IPv4: ${attachment.ipv4Address || '—'}`);
    await expect(
      page.getByTestId(`details-container-network-ipv6-${attachment.name}`),
    ).toHaveText(`IPv6: ${attachment.ipv6Address || '—'}`);
    await expect(
      page.getByTestId(`details-container-network-legacy-${attachment.name}`),
    ).toHaveText(`Legacy IP: ${attachment.legacyIpAddress || '—'}`);
    await expect(
      page.getByTestId(`details-container-network-mac-${attachment.name}`),
    ).toHaveText(`MAC: ${attachment.macAddress || '—'}`);
  }
}

export async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const metrics = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
    bodyScrollWidth: document.body.scrollWidth,
    innerWidth: window.innerWidth,
  }));
  expect(metrics.innerWidth).toBe(metrics.clientWidth);
  expect(metrics.scrollWidth).toBeLessThanOrEqual(metrics.clientWidth + 1);
  expect(metrics.bodyScrollWidth).toBeLessThanOrEqual(metrics.innerWidth + 1);
}
