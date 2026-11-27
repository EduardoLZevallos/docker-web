import type { Container, Network, NodeSelection } from '../types';
import { sortContainers, sortNetworks, textOrDash } from '../utils/topology';
import './TopologyList.css';

interface TopologyListProps {
  containers: Container[];
  networks: Network[];
  selection: NodeSelection | null;
  onSelect: (selection: NodeSelection) => void;
  emptyContainerMessage: string;
  emptyNetworkMessage: string;
}

function TopologyList({
  containers,
  networks,
  selection,
  onSelect,
  emptyContainerMessage,
  emptyNetworkMessage,
}: TopologyListProps) {
  const sortedContainers = sortContainers(containers);
  const sortedNetworks = sortNetworks(networks);

  return (
    <section
      className="topology-list"
      aria-labelledby="topology-list-heading"
      data-testid="topology-list"
    >
      <h2 id="topology-list-heading">Topology nodes</h2>

      <div
        className="topology-list-group"
        data-testid="topology-list-containers"
      >
        <h3>
          Containers <span className="count-badge">{containers.length}</span>
        </h3>
        {sortedContainers.length === 0 ? (
          <p className="list-empty" data-testid="container-list-empty">
            {emptyContainerMessage}
          </p>
        ) : (
          <ul className="node-list">
            {sortedContainers.map((container) => {
              const selected =
                selection?.kind === 'container' &&
                selection.id === container.id;
              const name = textOrDash(container.name || container.id);

              return (
                <li key={container.id}>
                  <button
                    type="button"
                    className={`node-list-button ${selected ? 'is-selected' : ''}`}
                    onClick={() =>
                      onSelect({ kind: 'container', id: container.id })
                    }
                    aria-pressed={selected}
                    aria-label={`Select container ${name}`}
                    data-testid={`node-select-container-${container.id}`}
                    data-node-kind="container"
                    data-node-id={container.id}
                  >
                    <span className="node-list-name">{name}</span>
                    <span className="node-list-meta">
                      {textOrDash(container.state ?? container.status)}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="topology-list-group" data-testid="topology-list-networks">
        <h3>
          Networks <span className="count-badge">{networks.length}</span>
        </h3>
        {sortedNetworks.length === 0 ? (
          <p className="list-empty" data-testid="network-list-empty">
            {emptyNetworkMessage}
          </p>
        ) : (
          <ul className="node-list">
            {sortedNetworks.map((network) => {
              const selected =
                selection?.kind === 'network' && selection.id === network.id;
              const name = textOrDash(network.name || network.id);

              return (
                <li key={network.id}>
                  <button
                    type="button"
                    className={`node-list-button ${selected ? 'is-selected' : ''}`}
                    onClick={() =>
                      onSelect({ kind: 'network', id: network.id })
                    }
                    aria-pressed={selected}
                    aria-label={`Select network ${name}`}
                    data-testid={`node-select-network-${network.id}`}
                    data-node-kind="network"
                    data-node-id={network.id}
                  >
                    <span className="node-list-name">{name}</span>
                    <span className="node-list-meta">
                      {textOrDash(network.driver)}
                      {network.subnet ? ` - ${network.subnet}` : ''}
                    </span>
                    <span
                      className="node-list-id mono"
                      title={`Stable network ID: ${network.id}`}
                      data-testid={`network-id-${network.id}`}
                    >
                      {network.id}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}

export default TopologyList;
