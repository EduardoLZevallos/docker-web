import type cytoscape from 'cytoscape';
import type { NodeKind } from '../types';

export interface GraphBridgeNodeSnapshot {
  internalId: string;
  rawId: string;
  kind: NodeKind;
  label: string;
  renderedPosition: {
    x: number;
    y: number;
  };
  classes: string[];
}

export interface GraphBridgeEdgeSnapshot {
  internalId: string;
  rawSourceId: string;
  rawTargetId: string;
  sourceInternalId: string;
  targetInternalId: string;
}

export interface GraphBridgeSnapshot {
  ready: boolean;
  nodeCount: number;
  edgeCount: number;
  nodes: GraphBridgeNodeSnapshot[];
  edges: GraphBridgeEdgeSnapshot[];
  selectedNodeRawIds: string[];
  selectedNodeInternalIds: string[];
  highlightedNodeRawIds: string[];
  highlightedNodeInternalIds: string[];
  highlightedEdgeInternalIds: string[];
  zoom: number;
  pan: {
    x: number;
    y: number;
  };
  canvas: {
    width: number;
    height: number;
  };
}

export interface GraphBridge {
  readonly version: 1;
  getSnapshot(): GraphBridgeSnapshot;
}

declare global {
  interface Window {
    __DOCKER_WEB_GRAPH__?: GraphBridge;
  }
}

export const isGraphBridgeEnabled = import.meta.env.VITE_E2E === 'true';

function readString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function readNodeKind(value: unknown): NodeKind {
  return value === 'network' ? 'network' : 'container';
}

function snapshotGraph(
  cy: cytoscape.Core,
  ready: boolean,
): GraphBridgeSnapshot {
  const nodes = cy.nodes().map(
    (node): GraphBridgeNodeSnapshot => ({
      internalId: node.id(),
      rawId: readString(node.data('rawId')),
      kind: readNodeKind(node.data('kind')),
      label: readString(node.data('label')),
      renderedPosition: {
        x: node.renderedPosition().x,
        y: node.renderedPosition().y,
      },
      classes: node.classes(),
    }),
  );

  const edges = cy.edges().map(
    (edge): GraphBridgeEdgeSnapshot => ({
      internalId: edge.id(),
      rawSourceId: readString(edge.data('rawSource')),
      rawTargetId: readString(edge.data('rawTarget')),
      sourceInternalId: edge.source().id(),
      targetInternalId: edge.target().id(),
    }),
  );

  const selectedNodes = cy.nodes('.selected');
  const highlightedNodes = cy.nodes('.highlighted');
  const highlightedEdges = cy.edges('.highlighted');

  return {
    ready,
    nodeCount: nodes.length,
    edgeCount: edges.length,
    nodes,
    edges,
    selectedNodeRawIds: selectedNodes
      .map((node) => readString(node.data('rawId')))
      .sort(),
    selectedNodeInternalIds: selectedNodes.map((node) => node.id()).sort(),
    highlightedNodeRawIds: highlightedNodes
      .map((node) => readString(node.data('rawId')))
      .sort(),
    highlightedNodeInternalIds: highlightedNodes.map((node) => node.id()).sort(),
    highlightedEdgeInternalIds: highlightedEdges.map((edge) => edge.id()).sort(),
    zoom: cy.zoom(),
    pan: {
      x: cy.pan().x,
      y: cy.pan().y,
    },
    canvas: {
      width: cy.width(),
      height: cy.height(),
    },
  };
}

export function installGraphBridge(
  cy: cytoscape.Core,
  isReady: () => boolean,
): () => void {
  if (!isGraphBridgeEnabled) {
    return () => undefined;
  }

  const bridge: GraphBridge = {
    version: 1,
    getSnapshot: () => snapshotGraph(cy, isReady()),
  };

  window.__DOCKER_WEB_GRAPH__ = bridge;

  return () => {
    if (window.__DOCKER_WEB_GRAPH__ === bridge) {
      delete window.__DOCKER_WEB_GRAPH__;
    }
  };
}
