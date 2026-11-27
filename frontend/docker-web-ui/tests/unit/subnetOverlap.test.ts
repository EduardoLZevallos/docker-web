import assert from 'node:assert/strict';
import test from 'node:test';
import {
  analyzeSubnetOverlaps,
  compareCidrs,
  formatCidr,
  parseCidr,
} from '../../src/utils/subnetOverlap.ts';
import {
  findingSignature,
  fixtureNetwork,
  fixtureTopology,
} from './subnetOverlapFixtures.ts';

test('compareCidrs_classifies_ipv4_equal_nested_boundary_and_adjacent_ranges', () => {
  assert.equal(compareCidrs('10.0.0.0/24', '10.0.0.0/24'), 'equal');
  assert.equal(compareCidrs('10.0.0.0/16', '10.0.5.0/24'), 'contains');
  assert.equal(compareCidrs('10.0.5.0/24', '10.0.0.0/16'), 'contained-by');
  assert.equal(compareCidrs('0.0.0.0/0', '255.255.255.255/32'), 'contains');
  assert.equal(compareCidrs('10.0.0.0/24', '10.0.0.0/25'), 'contains');
  assert.equal(compareCidrs('10.0.0.0/25', '10.0.0.128/25'), 'adjacent');
  assert.equal(compareCidrs('10.0.0.255/32', '10.0.1.0/32'), 'adjacent');
  assert.equal(compareCidrs('10.0.0.0/25', '10.0.2.0/24'), 'disjoint');
  assert.equal(compareCidrs('not-a-cidr', '10.0.0.0/24'), null);
});

test('compareCidrs_supports_compressed_and_embedded_ipv6_without_mixing_families', () => {
  assert.equal(
    compareCidrs('2001:0db8:0000:0000::/32', '2001:db8::/32'),
    'equal',
  );
  assert.equal(
    compareCidrs('::ffff:192.0.2.0/120', '::ffff:c000:200/120'),
    'equal',
  );
  assert.equal(
    compareCidrs('fd00::/48', 'fd00:0:0:1::/64'),
    'contains',
  );
  assert.equal(
    compareCidrs('2001:db8::/33', '2001:db8:8000::/33'),
    'adjacent',
  );
  assert.equal(
    compareCidrs('::ffff:192.0.2.0/120', '192.0.2.0/24'),
    'different-family',
  );

  const mapped = parseCidr('::ffff:c000:200/120');
  assert.ok(mapped);
  assert.equal(mapped.family, 'ipv6');
  assert.equal(formatCidr(mapped), '::ffff:192.0.2.0/120');

  const compressed = parseCidr('2001:0db8:0000:0000:0000:0000:0000:0001/64');
  assert.ok(compressed);
  assert.equal(formatCidr(compressed), '2001:db8::/64');
});

test('parseCidr_normalizes_brackets_zones_and_host_bits_and_rejects_invalid_values', () => {
  const bracketed = parseCidr('[fd00::]/64');
  assert.ok(bracketed);
  assert.equal(formatCidr(bracketed), 'fd00::/64');

  const zoned = parseCidr('fe80::1%eth0/64');
  assert.ok(zoned);
  assert.equal(formatCidr(zoned), 'fe80::/64');

  const hostBits = parseCidr('10.0.0.99/24');
  assert.ok(hostBits);
  assert.equal(formatCidr(hostBits), '10.0.0.0/24');

  for (const invalid of [
    '',
    '10.0.0.0',
    '10.0.0.0/',
    '10.0.0.0/-1',
    '10.0.0.0/33',
    '10.0.0.0/24/25',
    '10.0.0.256/24',
    '2001:db8::/129',
    '2001:db8:::1/64',
    '2001:db8::gg/64',
    '::ffff:300.0.2.0/120',
  ]) {
    assert.equal(parseCidr(invalid), null, invalid);
  }
});

test('analyzeSubnetOverlaps_compares_later_ipam_configs_and_uses_legacy_fallback', () => {
  const first = fixtureNetwork('net-a', 'alpha', [
    '10.10.0.0/24',
    '10.20.0.0/24',
  ]);
  const later = fixtureNetwork('net-b', 'beta', ['10.20.0.0/24']);
  const legacy = fixtureNetwork('net-c', 'gamma', null, '10.30.0.0/24');
  const legacyPeer = fixtureNetwork('net-d', 'delta', ['10.30.0.0/24']);

  const analysis = analyzeSubnetOverlaps(
    fixtureTopology([first, later, legacy, legacyPeer]),
  );

  assert.equal(analysis.findings.length, 2);
  assert.deepEqual(
    analysis.findings.map((finding) => formatCidr(finding.left.cidr)),
    ['10.20.0.0/24', '10.30.0.0/24'],
  );
  assert.deepEqual(
    analysis.coverages.map((coverage) => coverage.status),
    ['complete', 'complete', 'complete', 'complete'],
  );
});

test('analyzeSubnetOverlaps_dedupes_repeated_cidrs_and_is_order_independent', () => {
  const alpha = fixtureNetwork('net-a', 'alpha', [
    '10.0.0.0/24',
    '10.0.0.0/24',
    '10.0.5.0/24',
  ]);
  const beta = fixtureNetwork('net-b', 'beta', ['10.0.5.0/24']);
  const gamma = fixtureNetwork('net-c', 'gamma', ['10.0.5.0/24']);
  const delta = fixtureNetwork('net-d', 'delta', ['10.9.0.0/24']);

  const forward = analyzeSubnetOverlaps(
    fixtureTopology([alpha, beta, gamma, delta]),
  );
  const reversed = analyzeSubnetOverlaps(
    fixtureTopology([delta, gamma, beta, alpha]),
  );

  assert.equal(forward.findings.length, 3);
  assert.equal(forward.totalValidCidrs, 5);
  assert.deepEqual(findingSignature(forward), findingSignature(reversed));
  assert.equal(new Set(forward.findings.map((finding) => finding.key)).size, 3);
});

test('analyzeSubnetOverlaps_ignores_adjacent_ranges_and_keeps_findings_same_family', () => {
  const left = fixtureNetwork('net-a', 'alpha', [
    '10.0.0.0/24',
    'fd00::/64',
  ]);
  const right = fixtureNetwork('net-b', 'beta', [
    '10.0.1.0/24',
    'fd00:0:0:1::/64',
  ]);

  const analysis = analyzeSubnetOverlaps(fixtureTopology([left, right]));
  assert.equal(analysis.findings.length, 0);
  assert.equal(analysis.evaluatedNetworks, 2);
  assert.equal(analysis.totalValidCidrs, 4);
});

test('analyzeSubnetOverlaps_distinguishes_missing_or_invalid_coverage_from_empty_ipam', () => {
  const valid = fixtureNetwork('net-a', 'alpha', ['10.0.0.0/24']);
  const invalid = fixtureNetwork('net-b', 'beta', [null]);
  const partial = fixtureNetwork('net-c', 'gamma', [
    '10.1.0.0/24',
    'not-a-cidr',
    '   ',
  ]);
  const empty = fixtureNetwork('net-d', 'delta', null);
  const explicitlyEmpty = fixtureNetwork('net-e', 'epsilon', []);

  const analysis = analyzeSubnetOverlaps(
    fixtureTopology([valid, invalid, partial, empty, explicitlyEmpty]),
  );

  const byId = new Map(
    analysis.coverages.map((coverage) => [coverage.networkId, coverage]),
  );
  assert.equal(byId.get('net-b')?.status, 'invalid');
  assert.equal(byId.get('net-b')?.missingSubnets, 1);
  assert.equal(byId.get('net-c')?.status, 'partial');
  assert.deepEqual(byId.get('net-c')?.invalidSubnets, ['not-a-cidr']);
  assert.equal(byId.get('net-c')?.missingSubnets, 1);
  assert.equal(byId.get('net-d')?.status, 'empty');
  assert.equal(byId.get('net-e')?.status, 'empty');
  assert.equal(analysis.incompleteCoverages.length, 2);
  assert.equal(analysis.emptyCoverages.length, 2);
  assert.equal(analysis.hasIncompleteCoverage, true);
  assert.equal(analysis.findings.length, 0);
});

test('analyzeSubnetOverlaps_does_not_compare_cidrs_from_the_same_network_id', () => {
  const duplicated = fixtureNetwork('net-a', 'alpha', [
    '10.0.0.0/16',
    '10.0.1.0/24',
  ]);
  const analysis = analyzeSubnetOverlaps(
    fixtureTopology([duplicated, { ...duplicated, name: 'duplicate-alpha' }]),
  );

  assert.equal(analysis.totalNetworks, 1);
  assert.equal(analysis.findings.length, 0);
});

test('analyzeSubnetOverlaps_reports_ipv6_and_ipv4_findings_with_stable_keys', () => {
  const v4 = fixtureNetwork('net-a', 'alpha', ['10.0.0.0/24']);
  const v4Peer = fixtureNetwork('net-b', 'beta', ['10.0.0.128/25']);
  const v6 = fixtureNetwork('net-d', 'gamma', ['fd00::/64']);
  const v6Peer = fixtureNetwork('net-c', 'delta', ['fd00::/80']);

  const analysis = analyzeSubnetOverlaps(
    fixtureTopology([v6Peer, v4Peer, v6, v4]),
  );

  assert.equal(analysis.findings.length, 2);
  assert.deepEqual(
    analysis.findings.map((finding) => finding.family),
    ['ipv4', 'ipv6'],
  );
  assert.deepEqual(
    analysis.findings.map((finding) => finding.relation),
    ['contains', 'contained-by'],
  );
  assert.ok(
    analysis.findings.every((finding) =>
      finding.key.includes(finding.left.networkId) &&
      finding.key.includes(finding.right.networkId),
    ),
  );
});
