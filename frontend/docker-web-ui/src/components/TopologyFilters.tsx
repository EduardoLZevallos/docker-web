import type { Network } from '../types';
import { sortNetworks, textOrDash } from '../utils/topology';
import './TopologyFilters.css';

interface TopologyFiltersProps {
  networks: Network[];
  networkId: string;
  publishedOnly: boolean;
  onChangeNetwork: (networkId: string) => void;
  onChangePublishedOnly: (publishedOnly: boolean) => void;
  onReset: () => void;
}

function TopologyFilters({
  networks,
  networkId,
  publishedOnly,
  onChangeNetwork,
  onChangePublishedOnly,
  onReset,
}: TopologyFiltersProps) {
  const sortedNetworks = sortNetworks(networks);
  const hasActiveFilters = Boolean(networkId || publishedOnly);

  return (
    <div
      className="topology-filters"
      role="group"
      aria-label="Topology filters"
      data-testid="topology-filters"
    >
      <label className="filter-field" htmlFor="network-filter">
        <span className="filter-label">Network membership</span>
        <span className="filter-control">
          <select
            id="network-filter"
            className="filter-select"
            value={networkId}
            onChange={(event) => onChangeNetwork(event.target.value)}
            data-testid="network-filter"
            aria-describedby="network-filter-hint"
          >
            <option value="">All networks</option>
            {sortedNetworks.map((network) => (
              <option key={network.id} value={network.id}>
                {textOrDash(network.name || network.id)} — {network.id}
              </option>
            ))}
          </select>
          {networkId && (
            <span
              className="filter-selected-id mono"
              data-testid="network-filter-id"
              title={networkId}
            >
              Stable network ID: {networkId}
            </span>
          )}
        </span>
      </label>

      <label className="published-filter">
        <input
          type="checkbox"
          checked={publishedOnly}
          onChange={(event) => onChangePublishedOnly(event.target.checked)}
          data-testid="published-filter"
          aria-describedby="published-filter-hint"
        />
        <span>Published bindings</span>
      </label>

      {hasActiveFilters && (
        <button
          type="button"
          className="filter-reset"
          onClick={onReset}
          data-testid="filter-reset"
        >
          Clear filters
        </button>
      )}

      <p className="filter-hint" id="network-filter-hint">
        Filters containers by supplied membership edges. Network IDs are stable
        Docker IDs.
      </p>
      <p className="filter-hint" id="published-filter-hint">
        “Published bindings” means Docker-reported host port mappings on
        running or paused containers. It is not proof of a listening service or
        reachability.
      </p>
    </div>
  );
}

export default TopologyFilters;
