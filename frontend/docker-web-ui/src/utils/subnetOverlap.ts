import type { Network, Topology } from '../types';

export type IpFamily = 'ipv4' | 'ipv6';

export type CidrRelation =
  | 'equal'
  | 'contains'
  | 'contained-by'
  | 'overlap'
  | 'adjacent'
  | 'disjoint'
  | 'different-family';

export type CidrCoverageStatus = 'complete' | 'partial' | 'invalid' | 'empty';

export interface ParsedCidr {
  family: IpFamily;
  prefixLength: number;
  start: bigint;
  end: bigint;
  canonical: string;
}

export interface NetworkCidrCoverage {
  networkId: string;
  networkName: string;
  status: CidrCoverageStatus;
  cidrs: ParsedCidr[];
  invalidSubnets: string[];
  missingSubnets: number;
  reportedConfigs: number;
}

export interface OverlapFindingSide {
  networkId: string;
  networkName: string;
  cidr: ParsedCidr;
}

export type OverlapRelation = Extract<
  CidrRelation,
  'equal' | 'contains' | 'contained-by' | 'overlap'
>;

export interface SubnetOverlapFinding {
  key: string;
  family: IpFamily;
  relation: OverlapRelation;
  left: OverlapFindingSide;
  right: OverlapFindingSide;
}

export interface SubnetOverlapAnalysis {
  findings: SubnetOverlapFinding[];
  coverages: NetworkCidrCoverage[];
  incompleteCoverages: NetworkCidrCoverage[];
  emptyCoverages: NetworkCidrCoverage[];
  totalNetworks: number;
  evaluatedNetworks: number;
  totalValidCidrs: number;
  hasIncompleteCoverage: boolean;
}


const IPV4_BITS = 32;
const IPV6_BITS = 128;
const IPV4_MAX = (1n << 32n) - 1n;
const IPV6_MAX = (1n << 128n) - 1n;

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function compareBigInt(left: bigint, right: bigint): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function stripAddressBrackets(value: string): string {
  return value.startsWith('[') && value.endsWith(']')
    ? value.slice(1, -1)
    : value;
}

function stripZone(value: string): string {
  const zoneIndex = value.indexOf('%');
  return zoneIndex >= 0 ? value.slice(0, zoneIndex) : value;
}

function parseIpv4Number(value: string): bigint | null {
  const match = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(value);
  if (!match) {
    return null;
  }

  let address = 0n;
  for (const octetText of match.slice(1)) {
    const octet = Number(octetText);
    if (octet > 255) {
      return null;
    }
    address = (address << 8n) | BigInt(octet);
  }
  return address;
}

function parseIpv6Number(input: string): bigint | null {
  let address = input.trim().toLowerCase();
  if (!address) {
    return null;
  }

  if (address.includes('.')) {
    const lastColon = address.lastIndexOf(':');
    if (lastColon < 0) {
      return null;
    }
    const embedded = parseIpv4Number(address.slice(lastColon + 1));
    if (embedded === null) {
      return null;
    }
    const high = (embedded >> 16n).toString(16);
    const low = (embedded & 0xffffn).toString(16);
    address = `${address.slice(0, lastColon + 1)}${high}:${low}`;
  }

  const compressionIndex = address.indexOf('::');
  if (
    compressionIndex >= 0 &&
    address.indexOf('::', compressionIndex + 2) >= 0
  ) {
    return null;
  }

  let groups: string[];
  if (compressionIndex >= 0) {
    const left = address.slice(0, compressionIndex);
    const right = address.slice(compressionIndex + 2);
    const leftGroups = left ? left.split(':') : [];
    const rightGroups = right ? right.split(':') : [];
    if (
      leftGroups.some((group) => group.length === 0) ||
      rightGroups.some((group) => group.length === 0)
    ) {
      return null;
    }
    const missingGroups = 8 - leftGroups.length - rightGroups.length;
    if (missingGroups < 1) {
      return null;
    }
    groups = [
      ...leftGroups,
      ...Array.from({ length: missingGroups }, () => '0'),
      ...rightGroups,
    ];
  } else {
    groups = address.split(':');
    if (groups.length !== 8) {
      return null;
    }
  }

  if (groups.length !== 8) {
    return null;
  }

  let value = 0n;
  for (const group of groups) {
    if (!/^[0-9a-f]{1,4}$/.test(group)) {
      return null;
    }
    value = (value << 16n) | BigInt(Number.parseInt(group, 16));
  }
  return value;
}

function formatIpv4Number(value: bigint): string {
  return [24n, 16n, 8n, 0n]
    .map((shift) => Number((value >> shift) & 0xffn))
    .join('.');
}

function formatIpv6Number(value: bigint): string {
  if (value >> 32n === 0xffffn) {
    return `::ffff:${formatIpv4Number(value & 0xffffffffn)}`;
  }

  const groups = Array.from({ length: 8 }, (_, index) => {
    const shift = BigInt((7 - index) * 16);
    return Number((value >> shift) & 0xffffn);
  });

  let bestStart = -1;
  let bestLength = 0;
  let runStart = -1;
  for (let index = 0; index <= groups.length; index += 1) {
    if (index < groups.length && groups[index] === 0) {
      if (runStart < 0) {
        runStart = index;
      }
      continue;
    }
    if (runStart >= 0) {
      const runLength = index - runStart;
      if (runLength > bestLength) {
        bestStart = runStart;
        bestLength = runLength;
      }
      runStart = -1;
    }
  }

  const hexGroups = groups.map((group) => group.toString(16));
  if (bestLength < 2) {
    return hexGroups.join(':');
  }

  const left = hexGroups.slice(0, bestStart).join(':');
  const right = hexGroups.slice(bestStart + bestLength).join(':');
  if (!left && !right) {
    return '::';
  }
  if (!left) {
    return `::${right}`;
  }
  if (!right) {
    return `${left}::`;
  }
  return `${left}::${right}`;
}

function cidrMask(bits: number, prefixLength: number): bigint {
  if (prefixLength === 0) {
    return 0n;
  }
  const hostBits = bits - prefixLength;
  const max = bits === IPV4_BITS ? IPV4_MAX : IPV6_MAX;
  return (max >> BigInt(hostBits)) << BigInt(hostBits);
}

export function parseCidr(
  rawValue: string | null | undefined,
): ParsedCidr | null {
  const raw = rawValue?.trim() ?? '';
  if (!raw) {
    return null;
  }

  const slashIndex = raw.indexOf('/');
  if (slashIndex <= 0 || slashIndex !== raw.lastIndexOf('/')) {
    return null;
  }

  const addressText = stripZone(
    stripAddressBrackets(raw.slice(0, slashIndex).trim()),
  );
  const prefixText = raw.slice(slashIndex + 1).trim();
  if (!/^\d{1,3}$/.test(prefixText)) {
    return null;
  }

  const prefixLength = Number(prefixText);
  const isIpv6 = addressText.includes(':');
  const family: IpFamily = isIpv6 ? 'ipv6' : 'ipv4';
  const bits = isIpv6 ? IPV6_BITS : IPV4_BITS;
  const addressValue = isIpv6
    ? parseIpv6Number(addressText)
    : parseIpv4Number(addressText);

  if (
    addressValue === null ||
    prefixLength < 0 ||
    prefixLength > bits
  ) {
    return null;
  }

  const start = addressValue & cidrMask(bits, prefixLength);
  const hostMask =
    bits === IPV4_BITS
      ? IPV4_MAX >> BigInt(prefixLength)
      : IPV6_MAX >> BigInt(prefixLength);
  const end = start | hostMask;
  const canonical =
    family === 'ipv4'
      ? formatIpv4Number(start)
      : formatIpv6Number(start);

  return {
    family,
    prefixLength,
    start,
    end,
    canonical,
  };
}

export function formatCidr(cidr: ParsedCidr): string {
  return `${cidr.canonical}/${cidr.prefixLength}`;
}

export function classifyCidrRelation(
  left: ParsedCidr,
  right: ParsedCidr,
): CidrRelation {
  if (left.family !== right.family) {
    return 'different-family';
  }
  if (left.start === right.start && left.end === right.end) {
    return 'equal';
  }
  if (left.start <= right.start && left.end >= right.end) {
    return 'contains';
  }
  if (right.start <= left.start && right.end >= left.end) {
    return 'contained-by';
  }
  if (left.end < right.start) {
    return left.end + 1n === right.start ? 'adjacent' : 'disjoint';
  }
  if (right.end < left.start) {
    return right.end + 1n === left.start ? 'adjacent' : 'disjoint';
  }
  return 'overlap';
}

export function compareCidrs(
  left: string | null | undefined,
  right: string | null | undefined,
): CidrRelation | null {
  const leftCidr = parseCidr(left);
  const rightCidr = parseCidr(right);
  if (!leftCidr || !rightCidr) {
    return null;
  }
  return classifyCidrRelation(leftCidr, rightCidr);
}

function compareCidrsForSort(left: ParsedCidr, right: ParsedCidr): number {
  return (
    (left.family === right.family
      ? 0
      : left.family === 'ipv4'
        ? -1
        : 1) ||
    compareBigInt(left.start, right.start) ||
    left.prefixLength - right.prefixLength ||
    compareText(left.canonical, right.canonical)
  );
}

function rawSubnetEntries(network: Network): string[] {
  if (Array.isArray(network.ipam_configs)) {
    return network.ipam_configs.map((config) => config.subnet ?? '');
  }

  const legacySubnet = network.subnet?.trim() ?? '';
  return legacySubnet ? [legacySubnet] : [];
}

function buildCoverage(network: Network): NetworkCidrCoverage {
  const rawEntries = rawSubnetEntries(network);
  const cidrs: ParsedCidr[] = [];
  const invalid = new Set<string>();
  const seen = new Set<string>();
  let missingSubnets = 0;

  for (const entry of rawEntries) {
    const raw = entry.trim();
    if (!raw) {
      missingSubnets += 1;
      continue;
    }

    const cidr = parseCidr(raw);
    if (!cidr) {
      invalid.add(raw);
      continue;
    }

    const key = `${cidr.family}:${cidr.prefixLength}:${cidr.start.toString(16)}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    cidrs.push(cidr);
  }

  cidrs.sort(compareCidrsForSort);

  let status: CidrCoverageStatus;
  if (rawEntries.length === 0) {
    status = 'empty';
  } else if (cidrs.length === 0) {
    status = 'invalid';
  } else if (invalid.size > 0 || missingSubnets > 0) {
    status = 'partial';
  } else {
    status = 'complete';
  }

  return {
    networkId: network.id,
    networkName: network.name,
    status,
    cidrs,
    invalidSubnets: [...invalid].sort(compareText),
    missingSubnets,
    reportedConfigs: rawEntries.length,
  };
}

function findingKey(
  left: OverlapFindingSide,
  right: OverlapFindingSide,
): string {
  return `${left.networkId}|${formatCidr(left.cidr)}|${right.networkId}|${formatCidr(right.cidr)}`;
}

function compareFindings(
  left: SubnetOverlapFinding,
  right: SubnetOverlapFinding,
): number {
  return (
    (left.family === right.family ? 0 : left.family === 'ipv4' ? -1 : 1) ||
    compareText(left.left.networkId, right.left.networkId) ||
    compareText(left.right.networkId, right.right.networkId) ||
    compareCidrsForSort(left.left.cidr, right.left.cidr) ||
    compareCidrsForSort(left.right.cidr, right.right.cidr) ||
    compareText(left.key, right.key)
  );
}

export function analyzeSubnetOverlaps(
  topology: Pick<Topology, 'networks'>,
): SubnetOverlapAnalysis {
  const sortedNetworks = [...topology.networks].sort(
    (left, right) =>
      compareText(left.id, right.id) || compareText(left.name, right.name),
  );
  const uniqueNetworks: Network[] = [];
  const seenNetworkIds = new Set<string>();

  for (const network of sortedNetworks) {
    if (seenNetworkIds.has(network.id)) {
      continue;
    }
    seenNetworkIds.add(network.id);
    uniqueNetworks.push(network);
  }

  const coverages = uniqueNetworks.map(buildCoverage);
  const findings: SubnetOverlapFinding[] = [];

  for (let leftIndex = 0; leftIndex < coverages.length; leftIndex += 1) {
    const leftCoverage = coverages[leftIndex];
    for (
      let rightIndex = leftIndex + 1;
      rightIndex < coverages.length;
      rightIndex += 1
    ) {
      const rightCoverage = coverages[rightIndex];
      for (const leftCidr of leftCoverage.cidrs) {
        for (const rightCidr of rightCoverage.cidrs) {
          const relation = classifyCidrRelation(leftCidr, rightCidr);
          if (
            relation !== 'equal' &&
            relation !== 'contains' &&
            relation !== 'contained-by' &&
            relation !== 'overlap'
          ) {
            continue;
          }

          const left: OverlapFindingSide = {
            networkId: leftCoverage.networkId,
            networkName: leftCoverage.networkName,
            cidr: leftCidr,
          };
          const right: OverlapFindingSide = {
            networkId: rightCoverage.networkId,
            networkName: rightCoverage.networkName,
            cidr: rightCidr,
          };
          findings.push({
            key: findingKey(left, right),
            family: leftCidr.family,
            relation,
            left,
            right,
          });
        }
      }
    }
  }

  findings.sort(compareFindings);

  const incompleteCoverages = coverages.filter(
    (coverage) =>
      coverage.status === 'partial' || coverage.status === 'invalid',
  );
  const emptyCoverages = coverages.filter(
    (coverage) => coverage.status === 'empty',
  );
  const evaluatedCoverages = coverages.filter(
    (coverage) => coverage.cidrs.length > 0,
  );

  return {
    findings,
    coverages,
    incompleteCoverages,
    emptyCoverages,
    totalNetworks: coverages.length,
    evaluatedNetworks: evaluatedCoverages.length,
    totalValidCidrs: evaluatedCoverages.reduce(
      (total, coverage) => total + coverage.cidrs.length,
      0,
    ),
    hasIncompleteCoverage: incompleteCoverages.length > 0,
  };
}
