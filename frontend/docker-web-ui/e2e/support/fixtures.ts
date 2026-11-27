import { randomBytes } from 'node:crypto';
import type { E2EEnvironment } from './env';
import {
  dockerLines,
  innerDocker,
  innerDockerJson,
  sleep,
} from './docker';

export const E2E_IMAGE = 'alpine:3.20';
export const CASE_LABEL = 'com.docker-web.e2e.case';
const ELIGIBLE_STATES = new Set(['running', 'paused', 'restarting']);

export interface ContainerInspect {
  Id: string;
  Name: string;
  Created: string;
  State?: {
    Status?: string;
    Running?: boolean;
    Paused?: boolean;
    ExitCode?: number;
    StartedAt?: string;
  };
  Config?: {
    Image?: string;
  };
  NetworkSettings?: {
    Networks?: Record<
      string,
      {
        IPAddress?: string;
        GlobalIPv6Address?: string;
        MacAddress?: string;
        NetworkID?: string;
      }
    >;
    Ports?: Record<
      string,
      Array<{ HostIp?: string; HostPort?: string }> | null
    >;
  };
}

export interface NetworkInspect {
  Id: string;
  Name: string;
  Driver?: string;
  Scope?: string;
  Internal?: boolean;
  IPAM?: {
    Config?: Array<{ Subnet?: string; Gateway?: string }>;
  };
}

export interface FixtureNetwork {
  id: string;
  name: string;
}

export interface FixtureContainer {
  id: string;
  name: string;
}

export interface ExpectedContainer extends FixtureContainer {
  status: string;
  inspect: ContainerInspect;
}

export interface ExpectedNetwork extends FixtureNetwork {
  inspect: NetworkInspect;
}

export interface ExpectedEdge {
  containerId: string;
  networkId: string;
}

export interface ExpectedTopology {
  containers: ExpectedContainer[];
  networks: ExpectedNetwork[];
  edgePairs: ExpectedEdge[];
  nodes: string[];
  edges: string[];
}

export interface FixtureNetworkOptions {
  driver?: string;
  internal?: boolean;
  ipv6?: boolean;
  subnet?: string;
  subnets?: string[];
}

export interface FixturePortBinding {
  hostIp?: string;
  hostPort?: number | string;
  containerPort: number;
  protocol?: 'tcp' | 'udp' | 'sctp';
}

export interface FixtureContainerOptions {
  networks: string[];
  command?: string[];
  image?: string;
  start?: boolean;
  pause?: boolean;
  publishPort?: number;
  bindings?: FixturePortBinding[];
  waitForState?: string;
}

export interface ExpectedPortBinding {
  private: number;
  public: number | null;
  protocol: string;
  hostIp: string | null;
  kind: 'unknown' | 'wildcard' | 'loopback' | 'specific';
  mapping: string;
  binding: string;
}

export interface ExpectedAttachment {
  name: string;
  networkId: string | null;
  ipv4Address: string;
  ipv6Address: string;
  legacyIpAddress: string;
  macAddress: string;
}

function slugify(value: string): string {
  const slug = value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32);
  return slug || 'case';
}

function sortedNodes(
  containers: ExpectedContainer[],
  networks: ExpectedNetwork[],
): string[] {
  return [
    ...containers.map((container) => `container:${container.id}`),
    ...networks.map((network) => `network:${network.id}`),
  ].sort();
}

function sortedEdges(edges: ExpectedEdge[]): string[] {
  return edges
    .map((edge) => `${edge.containerId}->${edge.networkId}`)
    .sort();
}

export function expectedGraphLabel(name: string, id: string): string {
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

export function nodeKindForRawId(
  topology: ExpectedTopology,
  rawId: string,
): 'container' | 'network' | null {
  if (topology.containers.some((container) => container.id === rawId)) {
    return 'container';
  }
  if (topology.networks.some((network) => network.id === rawId)) {
    return 'network';
  }
  return null;
}

export function graphEdgeInternalId(
  topology: ExpectedTopology,
  edge: ExpectedEdge,
): string {
  const sourceKind = nodeKindForRawId(topology, edge.containerId);
  const targetKind = nodeKindForRawId(topology, edge.networkId);
  if (!sourceKind || !targetKind) {
    throw new Error(
      `edge ${edge.containerId}->${edge.networkId} references an unknown Docker node`,
    );
  }
  return `edge:${sourceKind}:${edge.containerId}->${targetKind}:${edge.networkId}`;
}

export function expectedContainer(
  topology: ExpectedTopology,
  id: string,
): ExpectedContainer {
  const container = topology.containers.find((candidate) => candidate.id === id);
  if (!container) {
    throw new Error(`container ${id} is not present in Docker inspect`);
  }
  return container;
}

export function expectedNetwork(
  topology: ExpectedTopology,
  id: string,
): ExpectedNetwork {
  const network = topology.networks.find((candidate) => candidate.id === id);
  if (!network) {
    throw new Error(`network ${id} is not present in Docker inspect`);
  }
  return network;
}

function expectedHostBindingKind(
  hostIp: string | null,
): ExpectedPortBinding['kind'] {
  if (!hostIp) {
    return 'unknown';
  }
  const raw = hostIp.trim();
  const address = raw.startsWith('[') && raw.endsWith(']')
    ? raw.slice(1, -1)
    : raw;
  const ipv4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(address);
  if (ipv4) {
    const octets = ipv4.slice(1).map(Number);
    if (octets.every((octet) => octet === 0)) {
      return 'wildcard';
    }
    if (octets[0] === 127) {
      return 'loopback';
    }
    return 'specific';
  }
  if (address === '::' || /^0*:0*:0*:0*:0*:0*:0*:0*$/.test(address)) {
    return 'wildcard';
  }
  if (address === '::1' || /^0*:0*:0*:0*:0*:0*:0*:0*1$/.test(address)) {
    return 'loopback';
  }
  return 'specific';
}

function expectedHostAddress(hostIp: string | null): string {
  if (!hostIp) {
    return 'unknown';
  }
  const raw = hostIp.trim();
  const unwrapped =
    raw.startsWith('[') && raw.endsWith(']') ? raw.slice(1, -1) : raw;
  return unwrapped.includes(':') ? `[${unwrapped}]` : unwrapped;
}

export function expectedPortBindings(
  container: ExpectedContainer,
): ExpectedPortBinding[] {
  const ports = container.inspect.NetworkSettings?.Ports ?? {};
  const rows: ExpectedPortBinding[] = [];

  for (const [key, bindings] of Object.entries(ports)) {
    const [privatePortText, protocol = 'tcp'] = key.split('/');
    const privatePort = Number(privatePortText);
    if (!bindings || bindings.length === 0) {
      rows.push({
        private: privatePort,
        public: null,
        protocol,
        hostIp: null,
        kind: 'unknown',
        mapping: `${privatePortText}/${protocol}`,
        binding: `${privatePortText}/${protocol} · not published by Docker`,
      });
      continue;
    }
    for (const binding of bindings) {
      const hostPort = binding.HostPort ? Number(binding.HostPort) : null;
      const hostIp = binding.HostIp?.trim() ? binding.HostIp : null;
      const kind = expectedHostBindingKind(hostIp);
      const mapping =
        hostPort === null
          ? `${privatePortText}/${protocol}`
          : `${hostPort}:${privatePortText}/${protocol}`;
      const bindingText =
        hostPort === null
          ? `${privatePortText}/${protocol} · not published by Docker`
          : kind === 'unknown'
            ? `${hostPort}:${privatePortText}/${protocol} · host IP unknown`
            : `${expectedHostAddress(hostIp)}:${hostPort}:${privatePortText}/${protocol} · ${kind}`;
      rows.push({
        private: privatePort,
        public: hostPort,
        protocol,
        hostIp,
        kind,
        mapping,
        binding: bindingText,
      });
    }
  }
  return rows.sort(
    (left, right) =>
      left.private - right.private ||
      (left.public ?? -1) - (right.public ?? -1) ||
      left.protocol.localeCompare(right.protocol) ||
      (left.hostIp ?? '').localeCompare(right.hostIp ?? ''),
  );
}

export function expectedPortTexts(container: ExpectedContainer): string[] {
  return expectedPortBindings(container).map((port) => port.mapping);
}

export function expectedAttachments(
  container: ExpectedContainer,
  networks: ExpectedNetwork[] = [],
): ExpectedAttachment[] {
  const networkIdsByName = new Map(
    networks.map((network) => [network.name, network.id]),
  );
  return Object.entries(container.inspect.NetworkSettings?.Networks ?? {})
    .map(([name, settings]) => {
      const inspectedId = settings.NetworkID?.trim() ?? '';
      return {
        name,
        networkId: inspectedId || networkIdsByName.get(name) || null,
        ipv4Address: settings.IPAddress ?? '',
        ipv6Address: settings.GlobalIPv6Address ?? '',
        legacyIpAddress: settings.IPAddress ?? '',
        macAddress: settings.MacAddress ?? '',
      };
    })
    .sort((left, right) => left.name.localeCompare(right.name));
}

export function expectedNetworkCidrs(
  network: ExpectedNetwork,
): Array<{ subnet: string; gateway: string }> {
  const configs = network.inspect.IPAM?.Config ?? [];
  return configs.map((config) => ({
    subnet: config.Subnet ?? '',
    gateway: config.Gateway ?? '',
  }));
}

export function expectedEdgeInternalIds(
  topology: ExpectedTopology,
  selectedContainerId: string,
): string[] {
  return topology.edgePairs
    .filter((edge) => edge.containerId === selectedContainerId)
    .map((edge) => graphEdgeInternalId(topology, edge))
    .sort();
}

export function expectedHighlightedNodes(
  topology: ExpectedTopology,
  selectedContainerId: string,
): string[] {
  const highlighted = new Set<string>([selectedContainerId]);
  for (const edge of topology.edgePairs) {
    if (edge.containerId === selectedContainerId) {
      highlighted.add(edge.networkId);
    }
  }
  return [...highlighted].sort();
}

export function expectedNetworkEdgeInternalIds(
  topology: ExpectedTopology,
  selectedNetworkId: string,
): string[] {
  return topology.edgePairs
    .filter((edge) => edge.networkId === selectedNetworkId)
    .map((edge) => graphEdgeInternalId(topology, edge))
    .sort();
}

export function expectedHighlightedNetworkNodes(
  topology: ExpectedTopology,
  selectedNetworkId: string,
): string[] {
  const highlighted = new Set<string>([selectedNetworkId]);
  for (const edge of topology.edgePairs) {
    if (edge.networkId === selectedNetworkId) {
      highlighted.add(edge.containerId);
    }
  }
  return [...highlighted].sort();
}

export interface ExpectedFilterOptions {
  networkId?: string | null;
  publishedOnly?: boolean;
  pinnedNetworkId?: string | null;
}

function hasCurrentPublishedBinding(container: ExpectedContainer): boolean {
  const state = container.inspect.State?.Status?.toLowerCase() ?? '';
  if (state !== 'running' && state !== 'paused') {
    return false;
  }
  return expectedPortBindings(container).some(
    (binding) => binding.public !== null,
  );
}

export function filterExpectedTopology(
  topology: ExpectedTopology,
  rawQuery: string,
  options: ExpectedFilterOptions = {},
): { nodes: string[]; edges: string[] } {
  const query = rawQuery.trim().toLowerCase();
  const networkId = options.networkId ?? null;
  const publishedOnly = options.publishedOnly ?? false;
  const pinnedNetworkId = options.pinnedNetworkId ?? null;
  const active = Boolean(query || networkId || publishedOnly);
  if (!active) {
    return { nodes: topology.nodes, edges: topology.edges };
  }

  const matchingContainers = topology.containers.filter((container) => {
    if (
      query &&
      !container.name.toLowerCase().includes(query) &&
      !container.status.toLowerCase().includes(query)
    ) {
      return false;
    }
    if (
      networkId &&
      !topology.edgePairs.some(
        (edge) =>
          edge.containerId === container.id && edge.networkId === networkId,
      )
    ) {
      return false;
    }
    if (publishedOnly && !hasCurrentPublishedBinding(container)) {
      return false;
    }
    return true;
  });
  const matchingIds = new Set(
    matchingContainers.map((container) => container.id),
  );
  const matchingEdges = topology.edgePairs.filter((edge) =>
    matchingIds.has(edge.containerId),
  );
  const includedNetworks = new Set(
    matchingEdges.map((edge) => edge.networkId),
  );
  if (networkId && topology.networks.some((network) => network.id === networkId)) {
    includedNetworks.add(networkId);
  }
  if (
    pinnedNetworkId &&
    topology.networks.some((network) => network.id === pinnedNetworkId)
  ) {
    includedNetworks.add(pinnedNetworkId);
  }

  return {
    nodes: [
      ...matchingContainers.map((container) => `container:${container.id}`),
      ...topology.networks
        .filter((network) => includedNetworks.has(network.id))
        .map((network) => `network:${network.id}`),
    ].sort(),
    edges: sortedEdges(
      matchingEdges.filter((edge) => includedNetworks.has(edge.networkId)),
    ),
  };
}

export function inspectDockerTopology(
  env: E2EEnvironment,
): ExpectedTopology {
  const containerIds = dockerLines(env, ['ps', '--all', '--quiet', '--no-trunc']);
  const networkIds = dockerLines(env, ['network', 'ls', '--quiet']);

  const containerInspections =
    containerIds.length > 0
      ? innerDockerJson<ContainerInspect[]>(env, ['inspect', ...containerIds])
      : [];
  const networkInspections =
    networkIds.length > 0
      ? innerDockerJson<NetworkInspect[]>(env, [
          'network',
          'inspect',
          ...networkIds,
        ])
      : [];

  const statuses = new Map<string, string>();
  if (containerIds.length > 0) {
    for (const line of dockerLines(env, [
      'ps',
      '--all',
      '--no-trunc',
      '--format',
      '{{.ID}}\t{{.Status}}',
    ])) {
      const separator = line.indexOf('\t');
      if (separator > 0) {
        statuses.set(line.slice(0, separator), line.slice(separator + 1));
      }
    }
  }

  const containers: ExpectedContainer[] = containerInspections.map(
    (inspect) => ({
      id: inspect.Id,
      name: inspect.Name.replace(/^\/+/, ''),
      status: statuses.get(inspect.Id) ?? '',
      inspect,
    }),
  );
  const networks: ExpectedNetwork[] = networkInspections.map((inspect) => ({
    id: inspect.Id,
    name: inspect.Name,
    inspect,
  }));

  const networksByName = new Map(
    networks.map((network) => [network.name, network]),
  );
  const seenEdges = new Set<string>();
  const edgePairs: ExpectedEdge[] = [];

  for (const container of containers) {
    const state = container.inspect.State?.Status?.toLowerCase() ?? '';
    if (!ELIGIBLE_STATES.has(state)) {
      continue;
    }
    for (const networkName of Object.keys(
      container.inspect.NetworkSettings?.Networks ?? {},
    )) {
      const network = networksByName.get(networkName);
      if (!network) {
        continue;
      }
      const key = `${container.id}->${network.id}`;
      if (seenEdges.has(key)) {
        continue;
      }
      seenEdges.add(key);
      edgePairs.push({ containerId: container.id, networkId: network.id });
    }
  }

  return {
    containers,
    networks,
    edgePairs,
    nodes: sortedNodes(containers, networks),
    edges: sortedEdges(edgePairs),
  };
}

export class DockerFixture {
  readonly env: E2EEnvironment;
  readonly caseLabel: string;
  private readonly caseTag: string;
  private readonly containers: FixtureContainer[] = [];
  private readonly networks: FixtureNetwork[] = [];
  private sequence = 0;

  constructor(env: E2EEnvironment, testTitle: string) {
    this.env = env;
    this.caseTag = `${slugify(testTitle)}-${randomBytes(3).toString('hex')}`;
    this.caseLabel = `ui-${this.caseTag}`;
  }

  private labelArgs(): string[] {
    return [
      '--label',
      `${this.env.runLabel}=${this.env.runId}`,
      '--label',
      `${CASE_LABEL}=${this.caseLabel}`,
    ];
  }

  private resourceName(stem: string): string {
    this.sequence += 1;
    return `e2e-ui-${this.caseTag}-${slugify(stem)}-${this.sequence}`;
  }

  async createNetwork(
    stem: string,
    options: FixtureNetworkOptions = {},
  ): Promise<FixtureNetwork> {
    const name = this.resourceName(stem);
    const args = [
      'network',
      'create',
      '--driver',
      options.driver ?? 'bridge',
      ...this.labelArgs(),
    ];
    if (options.internal) {
      args.push('--internal');
    }
    if (options.ipv6) {
      args.push('--ipv6');
    }
    for (const subnet of [
      ...(options.subnet ? [options.subnet] : []),
      ...(options.subnets ?? []),
    ]) {
      args.push('--subnet', subnet);
    }
    args.push(name);

    const id = innerDocker(this.env, args);
    const network = { id, name };
    this.networks.push(network);
    return network;
  }

  async createContainer(
    stem: string,
    options: FixtureContainerOptions,
  ): Promise<FixtureContainer> {
    if (options.networks.length === 0) {
      throw new Error(`container ${stem} must be attached to at least one network`);
    }

    const name = this.resourceName(stem);
    const args = [
      'create',
      '--name',
      name,
      ...this.labelArgs(),
      '--network',
      options.networks[0],
    ];
    if (options.publishPort) {
      args.push(
        '--publish',
        `127.0.0.1::${options.publishPort}/tcp`,
      );
    }
    for (const binding of options.bindings ?? []) {
      const hostIp = binding.hostIp
        ? binding.hostIp.includes(':')
          ? `[${binding.hostIp}]`
          : binding.hostIp
        : '';
      const hostPort =
        binding.hostPort === undefined ? '' : String(binding.hostPort);
      args.push(
        '--publish',
        `${hostIp}:${hostPort}:${binding.containerPort}/${binding.protocol ?? 'tcp'}`,
      );
    }
    args.push(options.image ?? E2E_IMAGE);
    args.push(
      ...(options.command ?? ['sh', '-c', 'sleep 3600']),
    );

    const id = innerDocker(this.env, args);
    const container = { id, name };
    this.containers.push(container);

    for (const network of options.networks.slice(1)) {
      innerDocker(this.env, ['network', 'connect', network, id]);
    }

    if (options.start !== false) {
      this.start(container);
      if (options.pause) {
        await this.waitForState(id, 'running');
        innerDocker(this.env, ['pause', id]);
        await this.waitForState(id, 'paused');
      } else {
        await this.waitForState(id, options.waitForState ?? 'running');
      }
    }

    return container;
  }

  start(container: FixtureContainer): void {
    innerDocker(this.env, ['start', container.id]);
  }

  stop(container: FixtureContainer): void {
    innerDocker(this.env, ['stop', '--time', '1', container.id]);
  }

  pause(container: FixtureContainer): void {
    innerDocker(this.env, ['pause', container.id]);
  }

  removeContainer(container: FixtureContainer): void {
    innerDocker(this.env, ['rm', '--force', container.id]);
    const index = this.containers.findIndex(
      (candidate) => candidate.id === container.id,
    );
    if (index >= 0) {
      this.containers.splice(index, 1);
    }
  }

  inspectContainer(id: string): ContainerInspect {
    return innerDockerJson<ContainerInspect[]>(this.env, ['inspect', id])[0];
  }

  inspectNetwork(id: string): NetworkInspect {
    return innerDockerJson<NetworkInspect[]>(this.env, [
      'network',
      'inspect',
      id,
    ])[0];
  }

  async waitForState(
    id: string,
    wanted: string,
    timeoutMs = 20_000,
  ): Promise<ContainerInspect> {
    const deadline = Date.now() + timeoutMs;
    let last = 'missing';
    while (Date.now() < deadline) {
      const inspect = this.inspectContainer(id);
      last = inspect.State?.Status ?? 'unknown';
      if (last === wanted) {
        return inspect;
      }
      await sleep(100);
    }
    throw new Error(
      `container ${id} did not reach state ${wanted}; last state was ${last}`,
    );
  }

  expectedTopology(): ExpectedTopology {
    return inspectDockerTopology(this.env);
  }

  async cleanup(): Promise<void> {
    const errors: string[] = [];

    for (const container of [...this.containers].reverse()) {
      try {
        innerDocker(this.env, ['rm', '--force', container.id]);
      } catch (error) {
        errors.push(
          error instanceof Error ? error.message : String(error),
        );
      }
    }
    for (const network of [...this.networks].reverse()) {
      try {
        innerDocker(this.env, ['network', 'rm', network.id]);
      } catch (error) {
        errors.push(
          error instanceof Error ? error.message : String(error),
        );
      }
    }

    const labelFilter = [
      '--filter',
      `label=${CASE_LABEL}=${this.caseLabel}`,
    ];
    let leftoverContainers: string[] = [];
    let leftoverNetworks: string[] = [];

    try {
      leftoverContainers = dockerLines(this.env, [
        'ps',
        '--all',
        '--quiet',
        '--no-trunc',
        ...labelFilter,
      ]);
      leftoverNetworks = dockerLines(this.env, [
        'network',
        'ls',
        '--quiet',
        ...labelFilter,
      ]);
    } catch (error) {
      throw new Error(
        `could not verify fixture cleanup for ${this.caseLabel}: ${
          error instanceof Error ? error.message : String(error)
        }; removal errors: ${errors.join(' | ') || 'none'}`,
      );
    }

    for (const id of leftoverContainers) {
      try {
        innerDocker(this.env, ['rm', '--force', id]);
      } catch (error) {
        errors.push(
          `leftover container ${id}: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
    for (const id of leftoverNetworks) {
      try {
        innerDocker(this.env, ['network', 'rm', id]);
      } catch (error) {
        errors.push(
          `leftover network ${id}: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }

    if (leftoverContainers.length > 0 || leftoverNetworks.length > 0) {
      leftoverContainers = dockerLines(this.env, [
        'ps',
        '--all',
        '--quiet',
        '--no-trunc',
        ...labelFilter,
      ]);
      leftoverNetworks = dockerLines(this.env, [
        'network',
        'ls',
        '--quiet',
        ...labelFilter,
      ]);
    }

    if (leftoverContainers.length > 0 || leftoverNetworks.length > 0) {
      throw new Error(
        `fixture cleanup left resources labeled ${CASE_LABEL}=${this.caseLabel}: ` +
          `containers=[${leftoverContainers.join(',')}] networks=[${leftoverNetworks.join(',')}]; ` +
          `errors: ${errors.join(' | ') || 'none'}`,
      );
    }
  }
}
