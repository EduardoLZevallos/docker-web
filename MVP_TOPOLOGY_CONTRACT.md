# MVP topology JSON contract (block 2)

This contract is additive. Existing fields keep their names and meanings; new
nullable fields are always present and use JSON `null` when Docker does not
report a value. New nullable strings that Docker reports as empty or
whitespace-only are normalized to `null`. Legacy string fields are preserved
as-is, including legacy empty strings.

The application endpoints are unchanged:

- `GET /api/containers`
- `GET /api/networks`
- `GET /api/topology`
- `GET /api/config`
- `GET /api/health`

The container object below is used by both `/api/containers` and
`/api/topology.containers`; the network object is used by both `/api/networks`
and `/api/topology.networks`.

## Docker data source and packaging boundary

Discovery is list-only. No container or network inspect requests are needed for
the topology payload.

| Backend method | Docker API path | Used for |
| --- | --- | --- |
| `DockerClient::list_containers()` | `GET /containers/json?all=1` | all containers, including inactive ones |
| `DockerClient::list_running_containers()` | `GET /containers/json?all=0` | legacy running-only callers and tests |
| `DockerClient::list_networks()` | `GET /networks` | all networks and IPAM metadata |

The production topology path calls the all-state container list and the network
list concurrently. It does not call `/containers/{id}/json`,
`/networks/{id}`, stats, logs, or events. Packaging therefore only needs
read-only access to the Docker socket/API and the application HTTP port.

Container data comes from the list response:

- `State` -> `container.state`
- `Status` -> `container.status`
- `Ports[]` -> `container.ports[]`
- `NetworkSettings.Networks` -> `container.networks`
- `Id`, `Names`, `Image`, `Created` -> existing identity fields

Network data comes from the network list response:

- `Id`, `Name`, `Driver`, `Scope`, `Internal`, `Containers` -> existing fields
- `IPAM.Config[]` -> `network.ipam_configs[]`
- the first IPAM config's `Subnet` -> legacy `network.subnet`

`network.containers` may remain empty because the non-verbose list endpoint does
not populate network-side membership. Topology edges are derived from the
container-side attachments instead.

## Container object

```json
{
  "id": "f0d5c0e37a8f4c8d9b7a6e5d4c3b2a10987654321fedcba09876543210abcdef",
  "name": "demo-web-1",
  "image": "nginx:1.27-alpine",
  "status": "Up 3 minutes",
  "state": "running",
  "created": 1764200000,
  "ports": [
    {
      "private": 80,
      "public": 8080,
      "protocol": "tcp",
      "host_ip": "0.0.0.0"
    }
  ],
  "networks": {
    "demo-frontend": {
      "network_id": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809",
      "ipv4_address": "172.30.0.2",
      "ipv6_address": null,
      "ip_address": "172.30.0.2",
      "mac_address": "02:42:ac:1e:00:02"
    }
  }
}
```

### `state`

`state` is the raw Docker container state string from the list endpoint, for
example `running`, `paused`, `restarting`, `created`, `exited`, `dead`, or
`removing`. It is not rewritten or lowercased by the backend. A missing or blank
Docker state becomes `null`.

`status` remains the existing human-readable Docker status string. Clients must
use `state` for machine decisions and must not infer state from `status`.

| Docker `state` | Container node | Topology edges |
| --- | --- | --- |
| `running` | yes | yes, for each resolved network attachment |
| `paused` | yes | yes, for each resolved network attachment |
| `restarting` | yes | yes, for each resolved network attachment |
| `created` | yes | no |
| `exited` | yes | no |
| `dead` | yes | no |
| `removing` | yes | no |
| any other value, `null`, or blank | yes | no |

Inactive containers remain visible with their Docker-reported metadata but do
not claim current network participation through topology edges.

### `ports`

`ports` keeps one row per Docker-reported binding. Multiple bindings for the
same container port are preserved as multiple rows. Existing `private`,
`public`, and `protocol` fields are unchanged.

- `private`: container-side port number.
- `public`: host-side port number, or `null` when Docker reports no host
  binding.
- `protocol`: `tcp`, `udp`, or `sctp`.
- `host_ip`: host IP reported by Docker for this binding, or `null` when absent
  or blank.

Port snapshot UI semantics:

- For `running` and `paused` containers, a row with a non-null `public` host
  port is counted and shown as currently published. `host_ip` identifies the
  reported host binding address when present.
- For `restarting`, `created`, `exited`, `dead`, `removing`, unknown, or
  `null` state, port rows are Docker-reported snapshot metadata only. Do not
  label them as actively published, listening, reachable, or running; do not
  count them as active runtime exposure.

### `networks` attachments

`networks` is an object keyed by network name. Each value contains:

- `network_id`: full Docker network ID reported on the container attachment, or
  `null` when absent or blank.
- `ipv4_address`: Docker-reported IPv4 address, or `null` when absent or blank.
- `ipv6_address`: Docker-reported global IPv6 address, or `null` when absent or
  blank.
- `ip_address`: legacy address field, preserved from Docker's IPv4 attachment
  value. It may be an empty string when Docker reports one.
- `mac_address`: existing MAC address field, preserving Docker's value.

## Network object

```json
{
  "id": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809",
  "name": "demo-frontend",
  "driver": "bridge",
  "scope": "local",
  "subnet": "172.30.0.0/16",
  "ipam_configs": [
    {
      "subnet": "172.30.0.0/16",
      "gateway": "172.30.0.1"
    }
  ],
  "internal": false,
  "containers": []
}
```

`ipam_configs` is either `null` or an ordered array matching Docker's
`IPAM.Config` order. Each entry always contains `subnet` and `gateway`, using
`null` when Docker reports no value or only blank whitespace. An absent or empty
Docker config list becomes `null`.

`subnet` is the legacy field and remains the first IPAM config's subnet exactly
as previously exposed. `ipam_configs` is the complete structured view.

## Topology object

```json
{
  "containers": [
    {
      "id": "f0d5c0e37a8f4c8d9b7a6e5d4c3b2a10987654321fedcba09876543210abcdef",
      "name": "demo-web-1",
      "image": "nginx:1.27-alpine",
      "status": "Up 3 minutes",
      "state": "running",
      "created": 1764200000,
      "ports": [],
      "networks": {
        "demo-frontend": {
          "network_id": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809",
          "ipv4_address": "172.30.0.2",
          "ipv6_address": null,
          "ip_address": "172.30.0.2",
          "mac_address": "02:42:ac:1e:00:02"
        }
      }
    }
  ],
  "networks": [
    {
      "id": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809",
      "name": "demo-frontend",
      "driver": "bridge",
      "scope": "local",
      "subnet": "172.30.0.0/16",
      "ipam_configs": [
        {
          "subnet": "172.30.0.0/16",
          "gateway": "172.30.0.1"
        }
      ],
      "internal": false,
      "containers": []
    }
  ],
  "edges": [
    {
      "source": "f0d5c0e37a8f4c8d9b7a6e5d4c3b2a10987654321fedcba09876543210abcdef",
      "target": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f809"
    }
  ],
  "timestamp": "2026-11-27T12:34:56Z"
}
```

`timestamp` remains an RFC3339 string. Edges are unique and always reference a
container ID and network ID present in the same response.

### Edge ID resolution

For each edge-eligible container attachment:

1. If the attachment has a non-blank `network_id`, that full Docker network ID
   is preferred. The edge is emitted only if the same network ID is present in
   the response's network list.
2. If the attachment has no `network_id`, the network name is used as a fallback
   only when exactly one listed network has that name. The edge target is that
   network's full Docker ID.
3. A non-blank attachment ID that is absent from the current network list is
   treated as stale. It is not replaced by a same-name fallback, and no edge is
   emitted.
4. Ambiguous names, missing names, and stale IDs produce no edge rather than a
   dangling or guessed edge.

Container IDs, network IDs, and edge targets use the full Docker IDs reported by
the list endpoints. Short IDs are not synthesized.

## Backwards compatibility

Existing client fields remain available:

- container: `id`, `name`, `image`, `status`, `created`, `ports`, `networks`
- port: `private`, `public`, `protocol`
- attachment: `ip_address`, `mac_address`
- network: `id`, `name`, `driver`, `scope`, `subnet`, `internal`, `containers`

New fields are additive: `container.state`, `port.host_ip`,
attachment `network_id`, `ipv4_address`, `ipv6_address`, and
`network.ipam_configs`. Consumers that ignore unknown fields continue to work.
