import type {
  Container,
  ContainerNetworkEndpoint,
  ContainerPort,
  DisplayTopology,
  IpamConfig,
  Network,
  Topology,
} from '../types';

export type HostBindingKind = 'unknown' | 'wildcard' | 'loopback' | 'specific';

export interface HostBindingClassification {
  kind: HostBindingKind;
  address: string | null;
  label: string;
}

export interface NetworkMember {
  container: Container;
  endpoint: ContainerNetworkEndpoint | null;
  endpointName: string | null;
}

export interface ContainerNetworkAttachment {
  name: string;
  endpoint: ContainerNetworkEndpoint;
  network: Network | null;
  isMember: boolean;
}

export function isCurrentBindingState(state: string | null | undefined): boolean {
  return state === 'running' || state === 'paused';
}

export function isEdgeEligibleState(state: string | null | undefined): boolean {
  return state === 'running' || state === 'paused' || state === 'restarting';
}

export function isInactiveState(state: string | null | undefined): boolean {
  return !isEdgeEligibleState(state);
}

export function hasCurrentPublishedBinding(container: Container): boolean {
  return (
    isCurrentBindingState(container.state) &&
    container.ports.some((port) => port.public !== null)
  );
}

export function currentPublishedBindings(
  container: Container,
): ContainerPort[] {
  if (!isCurrentBindingState(container.state)) {
    return [];
  }
  return container.ports.filter((port) => port.public !== null);
}

export function containerIsMemberOfNetwork(
  topology: Topology,
  containerId: string,
  networkId: string,
): boolean {
  return topology.edges.some(
    (edge) =>
      (edge.source === containerId && edge.target === networkId) ||
      (edge.source === networkId && edge.target === containerId),
  );
}

function stripAddressBrackets(value: string): string {
  if (value.startsWith('[') && value.endsWith(']')) {
    return value.slice(1, -1);
  }
  return value;
}

function stripZone(value: string): string {
  const zoneIndex = value.indexOf('%');
  return zoneIndex >= 0 ? value.slice(0, zoneIndex) : value;
}

function parseIpv4(value: string): number[] | null {
  const match = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(value);
  if (!match) {
    return null;
  }
  const octets = match.slice(1).map(Number);
  return octets.every((octet) => octet >= 0 && octet <= 255) ? octets : null;
}

function isZeroIpv6(value: string): boolean {
  return value === '::' || /^0*:0*:0*:0*:0*:0*:0*:0*$/.test(value);
}

function isIpv6Loopback(value: string): boolean {
  if (value === '::1') {
    return true;
  }
  return /^0*:0*:0*:0*:0*:0*:0*:0*1$/.test(value);
}

function classifyIpv4(octets: number[]): HostBindingKind | null {
  if (octets.every((octet) => octet === 0)) {
    return 'wildcard';
  }
  if (octets[0] === 127) {
    return 'loopback';
  }
  return null;
}

export function classifyHostBinding(
  hostIp: string | null | undefined,
): HostBindingClassification {
  const raw = hostIp?.trim() ?? '';
  if (!raw) {
    return { kind: 'unknown', address: null, label: 'host IP unknown' };
  }

  const address = stripZone(stripAddressBrackets(raw)).toLowerCase();
  const ipv4 = parseIpv4(address);
  if (ipv4) {
    const kind = classifyIpv4(ipv4) ?? 'specific';
    return { kind, address: raw, label: hostBindingKindLabel(kind) };
  }

  if (address.startsWith('::ffff:')) {
    const mapped = parseIpv4(address.slice('::ffff:'.length));
    if (mapped) {
      const kind = classifyIpv4(mapped) ?? 'specific';
      return { kind, address: raw, label: hostBindingKindLabel(kind) };
    }
  }

  if (address.includes(':')) {
    if (isZeroIpv6(address)) {
      return { kind: 'wildcard', address: raw, label: hostBindingKindLabel('wildcard') };
    }
    if (isIpv6Loopback(address)) {
      return { kind: 'loopback', address: raw, label: hostBindingKindLabel('loopback') };
    }
  }

  return { kind: 'specific', address: raw, label: hostBindingKindLabel('specific') };
}

export function hostBindingKindLabel(kind: HostBindingKind): string {
  switch (kind) {
    case 'unknown':
      return 'host IP unknown';
    case 'wildcard':
      return 'wildcard';
    case 'loopback':
      return 'loopback';
    case 'specific':
      return 'specific';
  }
}

export function formatHostAddress(hostIp: string | null | undefined): string {
  const raw = hostIp?.trim() ?? '';
  if (!raw) {
    return 'unknown';
  }
  const unwrapped = stripAddressBrackets(raw);
  return unwrapped.includes(':') && !unwrapped.includes('/')
    ? `[${unwrapped}]`
    : unwrapped;
}

export function formatPort(port: ContainerPort): string {
  const mapping = port.public === null ? '' : `${port.public}:`;
  return `${mapping}${port.private}/${port.protocol}`;
}

export function formatPortBinding(port: ContainerPort): string {
  if (port.public === null) {
    return `${port.private}/${port.protocol} · not published by Docker`;
  }
  const host = classifyHostBinding(port.host_ip);
  if (host.kind === 'unknown') {
    return `${port.public}:${port.private}/${port.protocol} · host IP unknown`;
  }
  return `${formatHostAddress(port.host_ip)}:${port.public}:${port.private}/${port.protocol} · ${host.label}`;
}

export function filterTopology(
  topology: Topology,
  rawQuery: string,
  options: {
    networkId?: string | null;
    publishedOnly?: boolean;
    pinnedNetworkId?: string | null;
  } = {},
): DisplayTopology {
  const query = rawQuery.trim();
  const networkId = options.networkId ?? null;
  const publishedOnly = options.publishedOnly ?? false;
  const pinnedNetworkId = options.pinnedNetworkId ?? null;
  const totalContainers = topology.containers.length;
  const totalNetworks = topology.networks.length;
  const active = Boolean(query || networkId || publishedOnly);

  if (!active) {
    return {
      topology,
      active: false,
      query: '',
      networkId: null,
      publishedOnly: false,
      hasMatches: totalContainers > 0,
      totalContainers,
      totalNetworks,
      visibleContainers: totalContainers,
      visibleNetworks: totalNetworks,
    };
  }

  const normalizedQuery = query.toLowerCase();
  const matchedContainers = topology.containers.filter((container) => {
    if (
      normalizedQuery &&
      !container.name.toLowerCase().includes(normalizedQuery) &&
      !container.status.toLowerCase().includes(normalizedQuery)
    ) {
      return false;
    }
    if (networkId && !containerIsMemberOfNetwork(topology, container.id, networkId)) {
      return false;
    }
    if (publishedOnly && !hasCurrentPublishedBinding(container)) {
      return false;
    }
    return true;
  });
  const matchedContainerIds = new Set(
    matchedContainers.map((container) => container.id),
  );

  const allNetworkIds = new Set(topology.networks.map((network) => network.id));
  const connectedNetworkIds = new Set<string>();

  for (const edge of topology.edges) {
    if (
      matchedContainerIds.has(edge.source) &&
      allNetworkIds.has(edge.target)
    ) {
      connectedNetworkIds.add(edge.target);
    }
    if (
      matchedContainerIds.has(edge.target) &&
      allNetworkIds.has(edge.source)
    ) {
      connectedNetworkIds.add(edge.source);
    }
  }

  if (networkId && allNetworkIds.has(networkId)) {
    connectedNetworkIds.add(networkId);
  }
  if (pinnedNetworkId && allNetworkIds.has(pinnedNetworkId)) {
    connectedNetworkIds.add(pinnedNetworkId);
  }

  const networks = topology.networks.filter((network) =>
    connectedNetworkIds.has(network.id),
  );
  const includedIds = new Set<string>([
    ...matchedContainerIds,
    ...connectedNetworkIds,
  ]);
  const edges = topology.edges.filter(
    (edge) => includedIds.has(edge.source) && includedIds.has(edge.target),
  );

  return {
    topology: {
      containers: matchedContainers,
      networks,
      edges,
      timestamp: topology.timestamp,
    },
    active: true,
    query,
    networkId,
    publishedOnly,
    hasMatches:
      matchedContainers.length > 0 ||
      Boolean(networkId && allNetworkIds.has(networkId)) ||
      Boolean(pinnedNetworkId && allNetworkIds.has(pinnedNetworkId)),
    totalContainers,
    totalNetworks,
    visibleContainers: matchedContainers.length,
    visibleNetworks: networks.length,
  };
}

export function getNetworkMembers(
  topology: Topology,
  networkId: string,
): Container[] {
  return getNetworkMembersWithAttachments(topology, networkId).map(
    (member) => member.container,
  );
}

export function getNetworkMembersWithAttachments(
  topology: Topology,
  networkId: string,
): NetworkMember[] {
  const network = topology.networks.find(
    (candidate) => candidate.id === networkId,
  );
  const containersById = new Map(
    topology.containers.map((container) => [container.id, container]),
  );
  const memberIds = new Set<string>();

  for (const edge of topology.edges) {
    if (edge.source === networkId) {
      const container = containersById.get(edge.target);
      if (container) {
        memberIds.add(container.id);
      }
    }
    if (edge.target === networkId) {
      const container = containersById.get(edge.source);
      if (container) {
        memberIds.add(container.id);
      }
    }
  }

  return [...memberIds]
    .map((id) => containersById.get(id))
    .filter((container): container is Container => Boolean(container))
    .sort(
      (left, right) =>
        left.name.localeCompare(right.name) ||
        left.id.localeCompare(right.id),
    )
    .map((container) => {
      const endpointEntry = Object.entries(container.networks).find(
        ([name, endpoint]) =>
          (endpoint.network_id !== null && endpoint.network_id === networkId) ||
          (network !== undefined && name === network.name),
      );
      return {
        container,
        endpoint: endpointEntry ? endpointEntry[1] : null,
        endpointName: endpointEntry ? endpointEntry[0] : null,
      };
    });
}

export function getContainerNetworkAttachments(
  topology: Topology,
  container: Container,
): ContainerNetworkAttachment[] {
  return Object.entries(container.networks)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([name, endpoint]) => {
      const matchingNetworks = topology.networks.filter(
        (network) =>
          (endpoint.network_id !== null &&
            network.id === endpoint.network_id) ||
          network.name === name,
      );
      const network =
        matchingNetworks.find(
          (candidate) =>
            endpoint.network_id !== null &&
            candidate.id === endpoint.network_id,
        ) ??
        (matchingNetworks.length === 1 ? matchingNetworks[0] : null);
      return {
        name,
        endpoint,
        network,
        isMember: network
          ? containerIsMemberOfNetwork(topology, container.id, network.id)
          : false,
      };
    });
}

export function getNetworkIpamConfigs(network: Network): IpamConfig[] {
  if (network.ipam_configs && network.ipam_configs.length > 0) {
    return network.ipam_configs;
  }
  if (network.subnet) {
    return [{ subnet: network.subnet, gateway: null }];
  }
  return [];
}

export function formatUnixSeconds(seconds: number): string {
  if (!Number.isFinite(seconds)) {
    return '—';
  }
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) {
    return '—';
  }
  return date.toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  });
}

export function formatTimestamp(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp || '—';
  }
  return date.toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  });
}

export function sortContainers(containers: Container[]): Container[] {
  return [...containers].sort(
    (left, right) =>
      left.name.localeCompare(right.name) ||
      left.id.localeCompare(right.id),
  );
}

export function sortNetworks(networks: Network[]): Network[] {
  return [...networks].sort(
    (left, right) =>
      left.name.localeCompare(right.name) ||
      left.id.localeCompare(right.id),
  );
}

export function textOrDash(value: string | null | undefined): string {
  return value && value.length > 0 ? value : '—';
}
