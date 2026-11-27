import { useMemo, useState } from 'react';
import type { NodeSelection, Topology } from '../types';
import {
  analyzeSubnetOverlaps,
  formatCidr,
  type CidrCoverageStatus,
  type NetworkCidrCoverage,
  type OverlapRelation,
  type SubnetOverlapFinding,
} from '../utils/subnetOverlap';
import './SubnetOverlapPanel.css';

interface SubnetOverlapPanelProps {
  topology: Topology | null;
  selection: NodeSelection | null;
  stale: boolean;
  onSelectNetwork: (networkId: string) => void;
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

function relationLabel(relation: OverlapRelation): string {
  switch (relation) {
    case 'equal':
      return 'identical CIDR';
    case 'contains':
      return 'left CIDR contains right CIDR';
    case 'contained-by':
      return 'right CIDR contains left CIDR';
    case 'overlap':
      return 'overlapping CIDRs';
  }
}

function coverageStatusLabel(status: CidrCoverageStatus): string {
  switch (status) {
    case 'complete':
      return 'complete coverage';
    case 'partial':
      return 'partial coverage';
    case 'invalid':
      return 'invalid or missing CIDR data';
    case 'empty':
      return 'no Docker-reported CIDR';
  }
}

function coverageDetail(coverage: NetworkCidrCoverage): string {
  if (coverage.status === 'invalid') {
    return 'No valid Docker-reported CIDR is available, so this network was not compared.';
  }
  if (coverage.status === 'partial') {
    return 'Some Docker-reported CIDR data is invalid or missing, so this network was only partially compared.';
  }
  if (coverage.status === 'empty') {
    return 'Docker did not report an IPAM CIDR for this network. This is not treated as an overlap finding.';
  }
  return 'Every Docker-reported CIDR for this network was compared.';
}

function FindingNetworkButton({
  finding,
  side,
  selected,
  onSelectNetwork,
}: {
  finding: SubnetOverlapFinding;
  side: 'left' | 'right';
  selected: boolean;
  onSelectNetwork: (networkId: string) => void;
}) {
  const entry = finding[side];
  return (
    <button
      type="button"
      className={`overlap-network-button ${selected ? 'is-selected' : ''}`}
      onClick={() => onSelectNetwork(entry.networkId)}
      aria-pressed={selected}
      aria-label={`Select network ${entry.networkName || entry.networkId} involved in ${finding.family} CIDR overlap`}
      data-testid={`overlap-select-${side}-${finding.key}`}
      data-network-id={entry.networkId}
    >
      <span className="overlap-network-name">
        {entry.networkName || entry.networkId}
      </span>
      <span className="overlap-cidr mono">{formatCidr(entry.cidr)}</span>
      <span className="overlap-network-id mono" title={entry.networkId}>
        {entry.networkId}
      </span>
    </button>
  );
}

function CoverageGroup({
  title,
  testId,
  coverages,
  showCidrs,
}: {
  title: string;
  testId: string;
  coverages: NetworkCidrCoverage[];
  showCidrs: boolean;
}) {
  if (coverages.length === 0) {
    return null;
  }

  return (
    <div className="overlap-coverage-group" data-testid={testId}>
      <h3>{title}</h3>
      <ul className="overlap-coverage-list">
        {coverages.map((coverage) => (
          <li
            key={coverage.networkId}
            data-testid={`overlap-coverage-network-${coverage.networkId}`}
            data-coverage-status={coverage.status}
          >
            <div className="overlap-coverage-heading">
              <strong>{coverage.networkName || coverage.networkId}</strong>
              <span className="overlap-coverage-status">
                {coverageStatusLabel(coverage.status)}
              </span>
            </div>
            <span className="overlap-network-id mono" title={coverage.networkId}>
              {coverage.networkId}
            </span>
            <span className="overlap-coverage-detail">
              {coverageDetail(coverage)}
            </span>
            {showCidrs && coverage.cidrs.length > 0 && (
              <span className="overlap-coverage-cidrs mono">
                {coverage.cidrs.map(formatCidr).join(', ')}
              </span>
            )}
            {coverage.invalidSubnets.length > 0 && (
              <span className="overlap-coverage-invalid">
                Invalid or unparseable values:{' '}
                <span className="mono">
                  {coverage.invalidSubnets.join(', ')}
                </span>
              </span>
            )}
            {coverage.missingSubnets > 0 && (
              <span className="overlap-coverage-invalid">
                {pluralize(
                  coverage.missingSubnets,
                  'IPAM entry has',
                  'IPAM entries have',
                )}{' '}
                no subnet value.
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function SubnetOverlapPanel({
  topology,
  selection,
  stale,
  onSelectNetwork,
}: SubnetOverlapPanelProps) {
  const [expanded, setExpanded] = useState(false);
  const analysis = useMemo(
    () => (topology ? analyzeSubnetOverlaps(topology) : null),
    [topology],
  );

  if (!analysis) {
    return null;
  }

  const findingCount = analysis.findings.length;
  const hasFindings = findingCount > 0;
  const selectedNetworkId =
    selection?.kind === 'network' ? selection.id : null;
  const badgeLabel = hasFindings
    ? 'Overlap found'
    : analysis.hasIncompleteCoverage
      ? 'Coverage incomplete'
      : 'No overlaps';

  let summary: string;
  if (analysis.totalNetworks === 0) {
    summary = 'No networks are present in the current snapshot.';
  } else if (analysis.evaluatedNetworks === 0) {
    summary = `No valid Docker-reported CIDRs are available to compare across ${pluralize(
      analysis.totalNetworks,
      'network',
      'networks',
    )}.`;
  } else if (hasFindings) {
    summary = `${pluralize(
      findingCount,
      'overlapping CIDR pair',
      'overlapping CIDR pairs',
    )} across ${pluralize(
      analysis.evaluatedNetworks,
      'evaluated network',
      'evaluated networks',
    )}.`;
  } else {
    summary = `No overlapping Docker-reported CIDRs across ${pluralize(
      analysis.evaluatedNetworks,
      'evaluated network',
      'evaluated networks',
    )}.`;
  }

  if (analysis.hasIncompleteCoverage) {
    summary += ` ${pluralize(
      analysis.incompleteCoverages.length,
      'network has',
      'networks have',
    )} incomplete CIDR coverage.`;
  }

  return (
    <section
      className={`overlap-panel ${hasFindings ? 'has-overlaps' : ''}`}
      aria-labelledby="overlap-heading"
      data-testid="overlap-panel"
      data-stale={stale ? 'true' : 'false'}
      data-finding-count={findingCount}
    >
      <div className="overlap-header">
        <div className="overlap-heading-block">
          <p className="overlap-kind">Subnet overview</p>
          <div className="overlap-title-row">
            <h2 id="overlap-heading">Docker-reported CIDR overlaps</h2>
            <span
              className={`overlap-badge ${hasFindings ? 'is-warning' : analysis.hasIncompleteCoverage ? 'is-caution' : 'is-clear'}`}
              data-testid="overlap-badge"
            >
              {badgeLabel}
            </span>
          </div>
          <p className="overlap-summary" data-testid="overlap-summary">
            {summary}
          </p>
          {stale && (
            <p className="overlap-stale" data-testid="overlap-stale">
              Showing the last successful topology snapshot because the latest
              refresh failed. Overlap status is stale until a refresh succeeds.
            </p>
          )}
        </div>
        <button
          type="button"
          className="overlap-toggle"
          onClick={() => setExpanded((current) => !current)}
          aria-expanded={expanded}
          aria-controls="overlap-panel-body"
          data-testid="overlap-toggle"
        >
          {expanded ? 'Hide details' : 'Show details'}
        </button>
      </div>

      {expanded && (
        <div
          className="overlap-body"
          id="overlap-panel-body"
          data-testid="overlap-body"
        >
          <p className="overlap-note" data-testid="overlap-note">
            This panel compares every Docker-reported IPv4 and IPv6 CIDR from
            distinct network IDs in the full topology snapshot. Search and
            filters do not change these findings. Adjacent CIDRs are not
            overlaps. A CIDR overlap does not prove routing, reachability, or
            traffic.
          </p>

          {hasFindings ? (
            <ul className="overlap-findings" data-testid="overlap-findings">
              {analysis.findings.map((finding) => (
                <li
                  key={finding.key}
                  className="overlap-finding"
                  data-testid="overlap-finding"
                  data-finding-key={finding.key}
                  data-family={finding.family}
                  data-relation={finding.relation}
                >
                  <div className="overlap-finding-meta">
                    <span className="overlap-family">
                      {finding.family === 'ipv4' ? 'IPv4' : 'IPv6'}
                    </span>
                    <span className="overlap-relation">
                      {relationLabel(finding.relation)}
                    </span>
                  </div>
                  <div className="overlap-finding-pair">
                    <FindingNetworkButton
                      finding={finding}
                      side="left"
                      selected={selectedNetworkId === finding.left.networkId}
                      onSelectNetwork={onSelectNetwork}
                    />
                    <span className="overlap-connector" aria-hidden="true">
                      ↔
                    </span>
                    <FindingNetworkButton
                      finding={finding}
                      side="right"
                      selected={selectedNetworkId === finding.right.networkId}
                      onSelectNetwork={onSelectNetwork}
                    />
                  </div>
                  <p className="overlap-finding-hint">
                    Select either network to clear search and filters and show
                    it in the graph, list, and details.
                  </p>
                </li>
              ))}
            </ul>
          ) : (
            <p className="overlap-empty" data-testid="overlap-empty">
              No overlapping Docker-reported CIDRs were found in this snapshot.
            </p>
          )}

          <div className="overlap-coverage" data-testid="overlap-coverage">
            <h3>Coverage</h3>
            <p className="overlap-coverage-summary" data-testid="overlap-coverage-summary">
              Compared {pluralize(analysis.totalValidCidrs, 'valid CIDR', 'valid CIDRs')}{' '}
              from {pluralize(analysis.evaluatedNetworks, 'network', 'networks')}.
              {analysis.emptyCoverages.length > 0
                ? ` ${pluralize(
                    analysis.emptyCoverages.length,
                    'network has',
                    'networks have',
                  )} no Docker-reported CIDR.`
                : ''}
              {analysis.hasIncompleteCoverage
                ? ` ${pluralize(
                    analysis.incompleteCoverages.length,
                    'network has',
                    'networks have',
                  )} incomplete coverage.`
                : ' Every reported CIDR was parsed successfully.'}
            </p>
            <CoverageGroup
              title="Incomplete coverage"
              testId="overlap-incomplete-coverage"
              coverages={analysis.incompleteCoverages}
              showCidrs
            />
            <CoverageGroup
              title="No Docker-reported CIDR"
              testId="overlap-empty-coverage"
              coverages={analysis.emptyCoverages}
              showCidrs={false}
            />
            {!analysis.hasIncompleteCoverage && analysis.emptyCoverages.length === 0 && (
              <p className="overlap-coverage-clear" data-testid="overlap-coverage-clear">
                All networks in this snapshot reported parseable CIDRs.
              </p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

export default SubnetOverlapPanel;
