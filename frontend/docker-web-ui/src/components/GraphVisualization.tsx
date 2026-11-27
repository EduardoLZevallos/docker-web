import { useEffect, useRef } from 'react';
import cytoscape from 'cytoscape';
import type { NodeSelection, NodeKind, ThemeName, Topology } from '../types';
import { installGraphBridge } from '../e2e/graphBridge';
import './GraphVisualization.css';

const CONTAINER_NODE_PREFIX = 'container:';
const NETWORK_NODE_PREFIX = 'network:';

interface EmptyState {
  testId: string;
  title: string;
  description: string;
}

interface GraphVisualizationProps {
  topology: Topology | null;
  theme: ThemeName;
  selection: NodeSelection | null;
  onSelect: (selection: NodeSelection | null) => void;
  emptyState: EmptyState;
}

interface GraphNodeData {
  id: string;
  rawId: string;
  kind: NodeKind;
  label: string;
}

interface GraphEdgeData {
  id: string;
  source: string;
  target: string;
  rawSource: string;
  rawTarget: string;
}

type GraphElementDefinition =
  | { group: 'nodes'; data: GraphNodeData }
  | { group: 'edges'; data: GraphEdgeData };

function containerGraphId(rawId: string): string {
  return `${CONTAINER_NODE_PREFIX}${rawId}`;
}

function networkGraphId(rawId: string): string {
  return `${NETWORK_NODE_PREFIX}${rawId}`;
}

export function compactGraphLabel(name: string, id: string): string {
  const value = name || id;
  if (value.length <= 28) {
    return value;
  }
  const parts = value.split('-').filter(Boolean);
  if (parts.length > 2) {
    const suffix = parts.slice(-2).join('-');
    if (suffix.length <= 28) {
      return suffix;
    }
  }
  return `${value.slice(0, 12)}…${value.slice(-13)}`;
}

function buildGraphElements(topology: Topology): GraphElementDefinition[] {
  const elements: GraphElementDefinition[] = [];
  const containerIds = new Map<string, string>();
  const networkIds = new Map<string, string>();

  for (const container of topology.containers) {
    const internalId = containerGraphId(container.id);
    containerIds.set(container.id, internalId);
    elements.push({
      group: 'nodes',
      data: {
        id: internalId,
        rawId: container.id,
        kind: 'container',
        label: compactGraphLabel(container.name, container.id),
      },
    });
  }

  for (const network of topology.networks) {
    const internalId = networkGraphId(network.id);
    networkIds.set(network.id, internalId);
    elements.push({
      group: 'nodes',
      data: {
        id: internalId,
        rawId: network.id,
        kind: 'network',
        label: compactGraphLabel(network.name, network.id),
      },
    });
  }

  const resolveNode = (rawId: string): string | undefined =>
    containerIds.get(rawId) ?? networkIds.get(rawId);

  const seenEdges = new Set<string>();
  for (const edge of topology.edges) {
    const source = resolveNode(edge.source);
    const target = resolveNode(edge.target);
    if (!source || !target) {
      continue;
    }

    const id = `edge:${source}->${target}`;
    if (seenEdges.has(id)) {
      continue;
    }
    seenEdges.add(id);
    elements.push({
      group: 'edges',
      data: {
        id,
        source,
        target,
        rawSource: edge.source,
        rawTarget: edge.target,
      },
    });
  }

  return elements;
}

function applyGraphElements(
  cy: cytoscape.Core,
  topology: Topology,
): boolean {
  const definitions = buildGraphElements(topology);
  const nextIds = new Set(definitions.map((definition) => definition.data.id));
  const stale = cy.elements().filter((element) => !nextIds.has(element.id()));
  let structuralChange = stale.length > 0;

  if (stale.length > 0) {
    stale.remove();
  }

  for (const definition of definitions) {
    const existing = cy.getElementById(definition.data.id);
    if (existing.length > 0) {
      existing.data(definition.data);
    } else {
      cy.add(definition as cytoscape.ElementDefinition);
      structuralChange = true;
    }
  }

  return structuralChange;
}

function graphStyle(theme: ThemeName): cytoscape.StylesheetJson {
  const dark = theme === 'dark';
  const background = dark ? '#0f172a' : '#ffffff';
  const containerColor = dark ? '#3b82f6' : '#2563eb';
  const networkColor = dark ? '#ef4444' : '#c026d3';
  const edgeColor = dark ? '#64748b' : '#64748b';
  const nodeText = dark ? '#ffffff' : '#0f172a';
  const highlightColor = '#f59e0b';

  return [
    {
      selector: 'node',
      style: {
        label: 'data(label)',
        color: nodeText,
        'font-family':
          'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
        'font-size': 10,
        'font-weight': 600,
        'text-halign': 'center',
        'text-valign': 'center',
        'text-wrap': 'wrap',
        'text-max-width': '96px',
        'text-overflow-wrap': 'anywhere',
        'text-outline-width': 2,
        'text-outline-color': background,
        'border-width': 2,
        'border-color': background,
        'transition-property': 'border-color, border-width',
        'transition-duration': 150,
      },
    },
    {
      selector: 'node[kind = "container"]',
      style: {
        shape: 'round-rectangle',
        width: 64,
        height: 48,
        'background-color': containerColor,
      },
    },
    {
      selector: 'node[kind = "network"]',
      style: {
        shape: 'diamond',
        width: 58,
        height: 58,
        'background-color': networkColor,
      },
    },
    {
      selector: 'edge',
      style: {
        width: 1.5,
        'line-color': edgeColor,
        'curve-style': 'bezier',
        opacity: 0.8,
        'transition-property': 'line-color, width, opacity',
        'transition-duration': 150,
      },
    },
    {
      selector: 'node.highlighted',
      style: {
        'border-width': 3,
        'border-color': highlightColor,
      },
    },
    {
      selector: 'node.selected',
      style: {
        'border-width': 5,
        'border-color': highlightColor,
      },
    },
    {
      selector: 'edge.highlighted',
      style: {
        width: 3,
        'line-color': highlightColor,
        opacity: 1,
      },
    },
  ];
}

function applySelection(
  cy: cytoscape.Core,
  selection: NodeSelection | null,
): void {
  cy.elements().removeClass('selected highlighted');

  if (!selection) {
    return;
  }

  const internalId =
    selection.kind === 'container'
      ? containerGraphId(selection.id)
      : networkGraphId(selection.id);
  const node = cy.getElementById(internalId);

  if (node.length === 0) {
    return;
  }

  node.addClass('selected');
  node.closedNeighborhood().addClass('highlighted');
}

function layoutInitialGraph(cy: cytoscape.Core): void {
  if (cy.elements().length === 0) {
    return;
  }

  cy.layout({
    name: 'breadthfirst',
    directed: false,
    spacingFactor: 1.8,
    avoidOverlap: true,
    padding: 36,
    animate: false,
    fit: true,
  }).run();
}

function viewportCenter(cy: cytoscape.Core): { x: number; y: number } {
  const zoom = cy.zoom();
  return {
    x: (cy.width() / 2 - cy.pan().x) / zoom,
    y: (cy.height() / 2 - cy.pan().y) / zoom,
  };
}

function GraphVisualization({
  topology,
  theme,
  selection,
  onSelect,
  emptyState,
}: GraphVisualizationProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const cyRef = useRef<cytoscape.Core | null>(null);
  const onSelectRef = useRef(onSelect);
  const bridgeReadyRef = useRef(false);
  const hasLaidOutRef = useRef(false);
  const positionCacheRef = useRef(
    new Map<string, { x: number; y: number }>(),
  );
  const initialThemeRef = useRef(theme);

  useEffect(() => {
    onSelectRef.current = onSelect;
  }, [onSelect]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const cy = cytoscape({
      container,
      elements: [],
      style: graphStyle(initialThemeRef.current),
      minZoom: 0.1,
      maxZoom: 4,
      wheelSensitivity: 0.15,
      boxSelectionEnabled: false,
      selectionType: 'single',
      layout: { name: 'preset' },
    });
    cyRef.current = cy;
    cy.resize();

    cy.on('tap', 'node', (event) => {
      const node = event.target;
      const rawId = node.data('rawId');
      const kind = node.data('kind');
      if (
        typeof rawId === 'string' &&
        (kind === 'container' || kind === 'network')
      ) {
        onSelectRef.current({ kind, id: rawId });
      }
    });

    cy.on('tap', (event) => {
      if (event.target === cy) {
        onSelectRef.current(null);
      }
    });

    const resizeObserver = new ResizeObserver(() => {
      cy.resize();
    });
    resizeObserver.observe(container);

    const uninstallBridge = installGraphBridge(
      cy,
      () => bridgeReadyRef.current,
    );

    return () => {
      uninstallBridge();
      resizeObserver.disconnect();
      cy.destroy();
      cyRef.current = null;
    };
  }, []);

  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) {
      return;
    }

    cy.style().fromJson(graphStyle(theme)).update();
  }, [theme]);

  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) {
      return;
    }

    if (!topology) {
      cy.elements().remove();
      bridgeReadyRef.current = false;
      return;
    }

    for (const node of cy.nodes()) {
      positionCacheRef.current.set(node.id(), { ...node.position() });
    }

    const structuralChange = applyGraphElements(cy, topology);
    bridgeReadyRef.current = true;

    if (!hasLaidOutRef.current && cy.elements().length > 0) {
      layoutInitialGraph(cy);
      hasLaidOutRef.current = true;
      for (const node of cy.nodes()) {
        positionCacheRef.current.set(node.id(), { ...node.position() });
      }
    } else if (structuralChange) {
      const missingNodes = cy.nodes().filter((node) => {
        const cached = positionCacheRef.current.get(node.id());
        if (cached) {
          node.position(cached);
          return false;
        }
        return true;
      });

      if (missingNodes.length > 0) {
        const center = viewportCenter(cy);
        const radiusStep = 80;
        missingNodes.forEach((node, index) => {
          const ring = Math.floor(index / 8);
          const angle = ((index % 8) / 8) * Math.PI * 2;
          const radius = radiusStep + ring * 70;
          node.position({
            x: center.x + Math.cos(angle) * radius,
            y: center.y + Math.sin(angle) * radius,
          });
          positionCacheRef.current.set(node.id(), { ...node.position() });
        });
      }
    }

    applySelection(cy, selection);
  }, [topology, selection]);

  const handleFitGraph = () => {
    const cy = cyRef.current;
    if (cy && cy.elements().length > 0) {
      cy.fit(cy.elements(), 36);
    }
  };

  const nodeCount = topology
    ? topology.containers.length + topology.networks.length
    : 0;
  const showEmptyState = nodeCount === 0;

  return (
    <section
      className={`graph-visualization ${theme === 'dark' ? 'dark' : 'light'}`}
      aria-labelledby="graph-heading"
    >
      <header className="graph-header">
        <div>
          <h2 id="graph-heading">Docker network membership</h2>
          <p className="graph-subtitle" data-testid="graph-membership-note">
            Container and network nodes with backend-supplied attachment edges.
            Edges mean Docker network membership only, not traffic or
            reachability.
          </p>
        </div>
        <div className="graph-header-actions">
          <div className="graph-legend" aria-label="Graph legend">
            <span className="legend-item">
              <span className="legend-icon container-icon" aria-hidden="true" />
              Container
            </span>
            <span className="legend-item">
              <span className="legend-icon network-icon" aria-hidden="true" />
              Network
            </span>
            <span className="legend-item">
              <span className="legend-icon edge-icon" aria-hidden="true" />
              Membership edge
            </span>
          </div>
          <button
            type="button"
            className="graph-fit-button"
            onClick={handleFitGraph}
            data-testid="fit-graph-button"
          >
            Fit graph
          </button>
        </div>
      </header>

      <div className="graph-canvas-wrapper">
        <div
          ref={containerRef}
          className="cytoscape-container"
          data-testid="topology-graph"
          role="region"
          aria-label="Interactive container and network topology graph"
        />
        {showEmptyState && (
          <div
            className="graph-empty-state"
            data-testid={emptyState.testId}
            role="status"
          >
            <h3>{emptyState.title}</h3>
            <p>{emptyState.description}</p>
          </div>
        )}
      </div>
    </section>
  );
}

export default GraphVisualization;
