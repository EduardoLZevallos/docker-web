import type { SubnetOverlapAnalysis } from '../../src/utils/subnetOverlap.ts';
import type { Network, Topology } from '../../src/types.ts';

export function fixtureNetwork(
  id: string,
  name: string,
  subnets: Array<string | null> | null,
  legacySubnet: string | null = null,
): Network {
  const ipamConfigs =
    subnets === null
      ? null
      : subnets.map((subnet) => ({ subnet, gateway: null }));

  return {
    id,
    name,
    driver: 'bridge',
    scope: 'local',
    subnet:
      legacySubnet ??
      (subnets === null
        ? null
        : (subnets.find((subnet) => subnet !== null) ?? null)),
    ipam_configs: ipamConfigs,
    internal: false,
    containers: [],
  };
}

export function fixtureTopology(networks: Network[]): Topology {
  return {
    containers: [],
    networks,
    edges: [],
    timestamp: '2026-11-27T12:00:00Z',
  };
}

export function findingSignature(
  analysis: SubnetOverlapAnalysis,
): unknown {
  return {
    findings: analysis.findings.map((finding) => ({
      key: finding.key,
      family: finding.family,
      relation: finding.relation,
      left: finding.left.networkId,
      right: finding.right.networkId,
    })),
    coverages: analysis.coverages.map((coverage) => ({
      networkId: coverage.networkId,
      status: coverage.status,
      cidrs: coverage.cidrs.map((cidr) => `${cidr.canonical}/${cidr.prefixLength}`),
      invalidSubnets: coverage.invalidSubnets,
      missingSubnets: coverage.missingSubnets,
    })),
  };
}
