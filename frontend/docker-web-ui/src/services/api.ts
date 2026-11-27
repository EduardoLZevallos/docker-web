import type {
  Container,
  ContainerNetworkEndpoint,
  ContainerPort,
  IpamConfig,
  Network,
  RuntimeConfig,
  ThemeName,
  Topology,
  TopologyEdge,
} from '../types';

const REQUEST_TIMEOUT_MS = 10_000;

export class ApiError extends Error {
  readonly status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function expectRecord(value: unknown, context: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new ApiError(`Invalid ${context}: expected an object.`);
  }
  return value;
}

function expectArray(value: unknown, context: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new ApiError(`Invalid ${context}: expected an array.`);
  }
  return value;
}

function expectString(
  record: Record<string, unknown>,
  key: string,
  context: string,
): string {
  const value = record[key];
  if (typeof value !== 'string') {
    throw new ApiError(`Invalid ${context}.${key}: expected a string.`);
  }
  return value;
}

function expectNumber(
  record: Record<string, unknown>,
  key: string,
  context: string,
): number {
  const value = record[key];
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new ApiError(`Invalid ${context}.${key}: expected a finite number.`);
  }
  return value;
}

function expectBoolean(
  record: Record<string, unknown>,
  key: string,
  context: string,
): boolean {
  const value = record[key];
  if (typeof value !== 'boolean') {
    throw new ApiError(`Invalid ${context}.${key}: expected a boolean.`);
  }
  return value;
}

function expectNullableString(
  record: Record<string, unknown>,
  key: string,
  context: string,
): string | null {
  const value = record[key];
  if (value !== null && typeof value !== 'string') {
    throw new ApiError(
      `Invalid ${context}.${key}: expected a string or null.`,
    );
  }
  return value;
}

function expectOptionalNullableString(
  record: Record<string, unknown>,
  key: string,
  context: string,
): string | null {
  if (!(key in record)) {
    return null;
  }
  return expectNullableString(record, key, context);
}

async function fetchJson(path: string, callerSignal?: AbortSignal): Promise<unknown> {
  const controller = new AbortController();
  let timedOut = false;
  const timeoutId = window.setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, REQUEST_TIMEOUT_MS);

  const forwardCallerAbort = () => controller.abort();
  if (callerSignal) {
    if (callerSignal.aborted) {
      controller.abort();
    } else {
      callerSignal.addEventListener('abort', forwardCallerAbort, { once: true });
    }
  }

  try {
    const response = await fetch(path, {
      method: 'GET',
      headers: { Accept: 'application/json' },
      cache: 'no-store',
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new ApiError(
        `GET ${path} failed with HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ''}.`,
        response.status,
      );
    }

    try {
      return await response.json();
    } catch {
      throw new ApiError(`GET ${path} returned invalid JSON.`);
    }
  } catch (error) {
    if (timedOut) {
      throw new ApiError(
        `GET ${path} timed out after ${REQUEST_TIMEOUT_MS / 1000} seconds.`,
      );
    }
    if (error instanceof ApiError) {
      throw error;
    }
    if (callerSignal?.aborted) {
      throw error;
    }
    const message = error instanceof Error ? error.message : String(error);
    throw new ApiError(`GET ${path} failed: ${message}`);
  } finally {
    window.clearTimeout(timeoutId);
    callerSignal?.removeEventListener('abort', forwardCallerAbort);
  }
}

function parseContainerNetwork(
  value: unknown,
  context: string,
): ContainerNetworkEndpoint {
  const record = expectRecord(value, context);
  return {
    network_id: expectOptionalNullableString(record, 'network_id', context),
    ipv4_address: expectOptionalNullableString(record, 'ipv4_address', context),
    ipv6_address: expectOptionalNullableString(record, 'ipv6_address', context),
    ip_address: expectNullableString(record, 'ip_address', context),
    mac_address: expectNullableString(record, 'mac_address', context),
  };
}

function parseContainerPort(value: unknown, context: string): ContainerPort {
  const record = expectRecord(value, context);
  const privatePort = expectNumber(record, 'private', context);
  if (!Number.isInteger(privatePort) || privatePort < 0 || privatePort > 65_535) {
    throw new ApiError(
      `Invalid ${context}.private: expected a port number between 0 and 65535.`,
    );
  }

  const publicPort = record.public;
  if (publicPort !== null) {
    if (
      typeof publicPort !== 'number' ||
      !Number.isInteger(publicPort) ||
      publicPort < 0 ||
      publicPort > 65_535
    ) {
      throw new ApiError(
        `Invalid ${context}.public: expected a port number or null.`,
      );
    }
  }

  return {
    private: privatePort,
    public: publicPort,
    protocol: expectString(record, 'protocol', context),
    host_ip: expectOptionalNullableString(record, 'host_ip', context),
  };
}

function parseContainer(value: unknown, index: number): Container {
  const context = `topology.containers[${index}]`;
  const record = expectRecord(value, context);
  const networksRecord = expectRecord(record.networks, `${context}.networks`);
  const networks: Record<string, ContainerNetworkEndpoint> = {};

  for (const [name, endpoint] of Object.entries(networksRecord)) {
    networks[name] = parseContainerNetwork(
      endpoint,
      `${context}.networks["${name}"]`,
    );
  }

  const ports = expectArray(record.ports, `${context}.ports`).map(
    (port, portIndex) =>
      parseContainerPort(port, `${context}.ports[${portIndex}]`),
  );

  return {
    id: expectString(record, 'id', context),
    name: expectString(record, 'name', context),
    image: expectString(record, 'image', context),
    status: expectString(record, 'status', context),
    state: expectOptionalNullableString(record, 'state', context),
    created: expectNumber(record, 'created', context),
    ports,
    networks,
  };
}

function parseIpamConfigs(
  record: Record<string, unknown>,
  context: string,
): IpamConfig[] | null {
  if (!('ipam_configs' in record)) {
    return null;
  }
  const value = record.ipam_configs;
  if (value === null) {
    return null;
  }
  const configs = expectArray(value, `${context}.ipam_configs`);
  return configs.map((config, index) => {
    const configContext = `${context}.ipam_configs[${index}]`;
    const configRecord = expectRecord(config, configContext);
    return {
      subnet: expectOptionalNullableString(configRecord, 'subnet', configContext),
      gateway: expectOptionalNullableString(configRecord, 'gateway', configContext),
    };
  });
}

function parseNetwork(value: unknown, index: number): Network {
  const context = `topology.networks[${index}]`;
  const record = expectRecord(value, context);
  const containers = expectArray(
    record.containers,
    `${context}.containers`,
  ).map((containerId, containerIndex) => {
    if (typeof containerId !== 'string') {
      throw new ApiError(
        `Invalid ${context}.containers[${containerIndex}]: expected a string.`,
      );
    }
    return containerId;
  });

  return {
    id: expectString(record, 'id', context),
    name: expectString(record, 'name', context),
    driver: expectString(record, 'driver', context),
    scope: expectString(record, 'scope', context),
    subnet: expectNullableString(record, 'subnet', context),
    ipam_configs: parseIpamConfigs(record, context),
    internal: expectBoolean(record, 'internal', context),
    containers,
  };
}

function parseEdge(value: unknown, index: number): TopologyEdge {
  const context = `topology.edges[${index}]`;
  const record = expectRecord(value, context);
  return {
    source: expectString(record, 'source', context),
    target: expectString(record, 'target', context),
  };
}

function parseTopology(value: unknown): Topology {
  const record = expectRecord(value, 'topology response');
  return {
    containers: expectArray(record.containers, 'topology.containers').map(
      parseContainer,
    ),
    networks: expectArray(record.networks, 'topology.networks').map(
      parseNetwork,
    ),
    edges: expectArray(record.edges, 'topology.edges').map(parseEdge),
    timestamp: expectString(record, 'timestamp', 'topology'),
  };
}

function parseConfig(value: unknown): RuntimeConfig {
  const record = expectRecord(value, 'config response');
  const themeValue = expectString(record, 'theme', 'config');
  const theme: ThemeName = themeValue.toLowerCase() === 'dark' ? 'dark' : 'light';

  let discoveryInterval: number | undefined;
  const rawInterval = record.discovery_interval;
  if (rawInterval !== undefined && rawInterval !== null) {
    if (
      typeof rawInterval !== 'number' ||
      !Number.isFinite(rawInterval) ||
      rawInterval <= 0
    ) {
      throw new ApiError(
        'Invalid config.discovery_interval: expected a positive number.',
      );
    }
    discoveryInterval = rawInterval;
  }

  return {
    theme,
    demo_mode: expectBoolean(record, 'demo_mode', 'config'),
    discovery_interval: discoveryInterval,
  };
}

export async function fetchRuntimeConfig(
  signal?: AbortSignal,
): Promise<RuntimeConfig> {
  return parseConfig(await fetchJson('/api/config', signal));
}

export async function fetchTopology(signal?: AbortSignal): Promise<Topology> {
  return parseTopology(await fetchJson('/api/topology', signal));
}
