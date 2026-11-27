export type ThemeName = 'light' | 'dark';

export type NodeKind = 'container' | 'network';

export interface ContainerNetworkEndpoint {
  network_id: string | null;
  ipv4_address: string | null;
  ipv6_address: string | null;
  ip_address: string | null;
  mac_address: string | null;
}

export interface ContainerPort {
  private: number;
  public: number | null;
  protocol: string;
  host_ip: string | null;
}

export interface Container {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string | null;
  created: number;
  ports: ContainerPort[];
  networks: Record<string, ContainerNetworkEndpoint>;
}

export interface IpamConfig {
  subnet: string | null;
  gateway: string | null;
}

export interface Network {
  id: string;
  name: string;
  driver: string;
  scope: string;
  subnet: string | null;
  ipam_configs: IpamConfig[] | null;
  internal: boolean;
  containers: string[];
}

export interface TopologyEdge {
  source: string;
  target: string;
}

export interface Topology {
  containers: Container[];
  networks: Network[];
  edges: TopologyEdge[];
  timestamp: string;
}

export interface RuntimeConfig {
  theme: ThemeName;
  demo_mode: boolean;
  discovery_interval?: number;
}

export interface NodeSelection {
  kind: NodeKind;
  id: string;
}

export interface TopologyFilters {
  query: string;
  networkId: string | null;
  publishedOnly: boolean;
  pinnedNetworkId: string | null;
}

export interface DisplayTopology {
  topology: Topology;
  active: boolean;
  query: string;
  networkId: string | null;
  publishedOnly: boolean;
  hasMatches: boolean;
  totalContainers: number;
  totalNetworks: number;
  visibleContainers: number;
  visibleNetworks: number;
}

export interface LastSuccess {
  timestamp: string;
  receivedAt: number;
}
