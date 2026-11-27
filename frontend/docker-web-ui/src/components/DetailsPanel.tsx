import type { NodeSelection, Topology } from '../types';
import {
  classifyHostBinding,
  currentPublishedBindings,
  formatPort,
  formatPortBinding,
  formatUnixSeconds,
  getContainerNetworkAttachments,
  getNetworkIpamConfigs,
  getNetworkMembersWithAttachments,
  isCurrentBindingState,
  isInactiveState,
  textOrDash,
} from '../utils/topology';
import './DetailsPanel.css';

interface DetailsPanelProps {
  topology: Topology | null;
  selection: NodeSelection | null;
}

function DetailsPanel({ topology, selection }: DetailsPanelProps) {
  if (!topology || !selection) {
    return (
      <section
        className="details-panel"
        aria-labelledby="details-heading"
        data-testid="details-panel"
      >
        <h2 id="details-heading">Details</h2>
        <p className="details-empty" data-testid="details-empty">
          Select a container or network in the graph or list to view metadata.
        </p>
      </section>
    );
  }

  if (selection.kind === 'container') {
    const container = topology.containers.find(
      (candidate) => candidate.id === selection.id,
    );

    if (!container) {
      return (
        <section
          className="details-panel"
          aria-labelledby="details-heading"
          data-testid="details-panel"
        >
          <h2 id="details-heading">Details</h2>
          <p className="details-empty" data-testid="details-empty">
            The selected container is no longer present.
          </p>
        </section>
      );
    }

    const attachments = getContainerNetworkAttachments(topology, container);
    const currentBindings = currentPublishedBindings(container);
    const activeRuntime = isCurrentBindingState(container.state);
    const inactive = isInactiveState(container.state);
    const createdDate = new Date(container.created * 1000);
    const createdIso = Number.isNaN(createdDate.getTime())
      ? undefined
      : createdDate.toISOString();

    return (
      <section
        className="details-panel"
        aria-labelledby="details-heading"
        aria-live="polite"
        data-testid="details-panel"
      >
        <div className="details-heading-row">
          <div>
            <p className="details-kind">Container</p>
            <h2 id="details-heading">
              {textOrDash(container.name || container.id)}
            </h2>
          </div>
        </div>
        <dl className="details-grid" data-testid="details-container">
          <div>
            <dt>Full container ID</dt>
            <dd className="mono" data-testid="details-container-id">
              {textOrDash(container.id)}
            </dd>
          </div>
          <div>
            <dt>Image</dt>
            <dd className="mono" data-testid="details-container-image">
              {textOrDash(container.image)}
            </dd>
          </div>
          <div>
            <dt>Docker state</dt>
            <dd data-testid="details-container-state">
              {textOrDash(container.state)}
            </dd>
          </div>
          <div>
            <dt>Raw Docker status</dt>
            <dd data-testid="details-container-status">
              {textOrDash(container.status)}
            </dd>
          </div>
          <div>
            <dt>Created</dt>
            <dd data-testid="details-container-created">
              <time dateTime={createdIso} title={`${container.created} Unix seconds`}>
                {formatUnixSeconds(container.created)}
              </time>
            </dd>
          </div>
          <div className="details-grid-wide">
            <dt>Docker-reported port mappings</dt>
            <dd data-testid="details-container-ports">
              {container.ports.length === 0 ? (
                'None'
              ) : (
                <ul className="compact-list port-list">
                  {container.ports.map((port, index) => {
                    const host = classifyHostBinding(port.host_ip);
                    return (
                      <li
                        key={`${port.protocol}-${port.private}-${port.public}-${index}`}
                        data-testid={`details-container-port-${index}`}
                        data-binding-kind={
                          port.public === null ? 'not-published' : host.kind
                        }
                      >
                        <span className="mono port-mapping">
                          {formatPort(port)}
                        </span>
                        <span className="port-binding">
                          {formatPortBinding(port)}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
              <p
                className="details-note"
                data-testid="details-container-binding-count"
              >
                Current host port bindings: {currentBindings.length}{' '}
                (Docker-reported, not a listening or reachability check)
              </p>
              <p
                className="details-note"
                data-testid="details-container-binding-state"
              >
                {activeRuntime
                  ? 'Running or paused: Docker-reported host mappings are shown as current bindings.'
                  : 'Not running or paused: all port rows are retained Docker metadata only and are not counted as current bindings.'}
              </p>
            </dd>
          </div>
          <div className="details-grid-wide">
            <dt>Docker network attachments</dt>
            <dd data-testid="details-container-networks">
              {attachments.length === 0 ? (
                'None'
              ) : (
                <ul className="network-attachment-list">
                  {attachments.map((attachment) => {
                    const networkId =
                      attachment.endpoint.network_id ?? attachment.network?.id ?? null;
                    return (
                      <li
                        key={attachment.name}
                        data-testid={`details-container-network-${attachment.name}`}
                      >
                        <strong>{textOrDash(attachment.name)}</strong>
                        <span className="mono" data-testid={`details-container-network-id-${attachment.name}`}>
                          Network ID: {textOrDash(networkId)}
                        </span>
                        <span data-testid={`details-container-network-ipv4-${attachment.name}`}>
                          IPv4: {textOrDash(attachment.endpoint.ipv4_address)}
                        </span>
                        <span data-testid={`details-container-network-ipv6-${attachment.name}`}>
                          IPv6: {textOrDash(attachment.endpoint.ipv6_address)}
                        </span>
                        <span data-testid={`details-container-network-legacy-${attachment.name}`}>
                          Legacy IP: {textOrDash(attachment.endpoint.ip_address)}
                        </span>
                        <span data-testid={`details-container-network-mac-${attachment.name}`}>
                          MAC: {textOrDash(attachment.endpoint.mac_address)}
                        </span>
                        <span
                          className="attachment-membership"
                          data-testid={`details-container-network-membership-${attachment.name}`}
                        >
                          {attachment.isMember
                            ? 'Supplied topology edge: Docker network membership'
                            : inactive
                              ? 'Inactive container: retained attachment metadata only; no current graph membership edge'
                              : 'No supplied graph membership edge (the attachment may be stale or ambiguous)'}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
            </dd>
          </div>
          <div className="details-grid-wide">
            <p className="details-note" data-testid="details-container-membership-note">
              Graph edges mean Docker network membership only. They do not prove
              traffic, reachability, or container health.
            </p>
          </div>
        </dl>
      </section>
    );
  }

  const network = topology.networks.find(
    (candidate) => candidate.id === selection.id,
  );

  if (!network) {
    return (
      <section
        className="details-panel"
        aria-labelledby="details-heading"
        data-testid="details-panel"
      >
        <h2 id="details-heading">Details</h2>
        <p className="details-empty" data-testid="details-empty">
          The selected network is no longer present.
        </p>
      </section>
    );
  }

  const members = getNetworkMembersWithAttachments(topology, network.id);
  const ipamConfigs = getNetworkIpamConfigs(network);

  return (
    <section
      className="details-panel"
      aria-labelledby="details-heading"
      aria-live="polite"
      data-testid="details-panel"
    >
      <div className="details-heading-row">
        <div>
          <p className="details-kind">Network</p>
          <h2 id="details-heading">{textOrDash(network.name || network.id)}</h2>
        </div>
      </div>
      <dl className="details-grid" data-testid="details-network">
        <div>
          <dt>Full network ID</dt>
          <dd className="mono" data-testid="details-network-id">
            {textOrDash(network.id)}
          </dd>
        </div>
        <div>
          <dt>Driver</dt>
          <dd data-testid="details-network-driver">
            {textOrDash(network.driver)}
          </dd>
        </div>
        <div>
          <dt>Scope</dt>
          <dd data-testid="details-network-scope">
            {textOrDash(network.scope)}
          </dd>
        </div>
        <div>
          <dt>Legacy subnet</dt>
          <dd className="mono" data-testid="details-network-subnet">
            {textOrDash(network.subnet)}
          </dd>
        </div>
        <div>
          <dt>Internal</dt>
          <dd data-testid="details-network-internal">
            {network.internal ? 'Yes' : 'No'}
          </dd>
        </div>
        <div className="details-grid-wide">
          <dt>All IPAM CIDRs</dt>
          <dd data-testid="details-network-cidrs">
            {ipamConfigs.length === 0 ? (
              <span data-testid="details-network-cidr-empty">—</span>
            ) : (
              <ul className="compact-list">
                {ipamConfigs.map((config, index) => (
                  <li
                    key={`${config.subnet ?? 'unknown'}-${index}`}
                    data-testid={`details-network-cidr-${index}`}
                  >
                    <span className="mono">{textOrDash(config.subnet)}</span>
                    <span className="port-binding">
                      {' '}
                      Gateway: {textOrDash(config.gateway)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </dd>
        </div>
        <div className="details-grid-wide">
          <dt>Members from supplied topology edges</dt>
          <dd data-testid="details-network-members">
            {members.length === 0 ? (
              <span data-testid="details-network-member-empty">
                No member connections in the supplied topology.
              </span>
            ) : (
              <ul className="member-list">
                {members.map(({ container, endpoint }) => (
                  <li
                    key={container.id}
                    data-testid={`details-network-member-${container.id}`}
                  >
                    <strong>{textOrDash(container.name || container.id)}</strong>
                    <span className="mono">{textOrDash(container.id)}</span>
                    <span>
                      IPv4: {textOrDash(endpoint?.ipv4_address)}
                    </span>
                    <span>
                      IPv6: {textOrDash(endpoint?.ipv6_address)}
                    </span>
                    <span>
                      State: {textOrDash(container.state)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </dd>
        </div>
        <div className="details-grid-wide">
          <p className="details-note" data-testid="details-network-membership-note">
            Edges show Docker network membership only. Sharing a network does
            not prove traffic or reachability.
          </p>
        </div>
      </dl>
    </section>
  );
}

export default DetailsPanel;
