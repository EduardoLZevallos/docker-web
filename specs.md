# Docker Network Discovery, Visualization, and Monitoring Tool Specification

## Overview
This tool is designed to provide network discovery, visualization, and monitoring for Docker containers. It will automatically detect running Docker containers, their network configurations, and relationships, and present this data in a graph-based visualization. The tool will run inside a Docker container and include a React-based frontend for user interaction.

---

## Features

### Network Discovery
- Automatically detect all running Docker containers and their network configurations.
- Retrieve the following details:
  - **Container Information**: Names, IDs, statuses.
  - **Network Information**: Names, IP addresses, subnets, MAC addresses.
  - **Relationships**: Connections between containers on the same network.
  - **Ports**: Open ports for each container.
- Periodic discovery with a configurable interval (via YAML configuration file).
- Manual discovery trigger available in the frontend.
- Automatically reconnect to the Docker environment if the connection is lost.

### Network Visualization
- Graph-based visualization:
  - **Center Node**: Represents the network name.
  - **Edges**: Connect the network node to container nodes.
- Metadata display on hover or click:
  - IP address, MAC address, open ports, and status.
- Dynamic updates based on periodic discovery.
- Fixed design for graph appearance.
- Search functionality:
  - Partial matches (e.g., "web" matches "web-server").
  - Advanced filters (e.g., by status or open ports).
- Frontend theme options:
  - Light and dark modes with a toggle in the UI.
  - User preference saved in the configuration file.

### Configuration
- YAML configuration file with the following options:
  - **Discovery Interval**: Configurable periodic interval.
  - **Docker Socket Path**: Default `/var/run/docker.sock`.
  - **Reverse Proxy URL**: Prioritized over the Docker socket path if configured.
  - **Logging Level**: Options like `debug`, `info`, `error`.
  - **Log File Paths**:
    - Discovery and network data logs.
    - Performance metrics logs.
  - **Frontend Theme Preference**: Light or dark mode.
  - **Authentication Credentials**: Single username and hashed password.

### Authentication
- Username and password-based authentication for the frontend.
- Single set of credentials stored in the configuration file (password hashed for security).
- No role-based access control; all authenticated users have the same access level.

### Logging and Metrics
- Logs discovery and monitoring activities to both a file and the console.
- Separate log file for performance metrics:
  - Discovery duration.
  - Resource usage (e.g., memory, CPU).
- No log rotation (left to external tools).

### Error Handling
- Validates the configuration file on startup:
  - Ensures reverse proxy URL or Docker socket path is accessible.
  - Exits with a clear error message if validation fails.
- Detects and handles changes to the Docker environment (e.g., socket becoming unavailable).
- Displays visual indicators in the frontend for:
  - Active discovery process.
  - Connection issues with the Docker environment.
- Automatically attempts to reconnect to the Docker environment if disconnected.

### Deployment
- Runs inside a Docker container.
- Exposes port `3000` for the frontend.
- Requires users to provide their own YAML configuration file.
- Includes a health check for general service availability (e.g., checking if the frontend port is responsive).

---

## Architecture

### Backend
- Written in **Rust**.
- Interacts with the Docker API via the Docker socket or reverse proxy.
- Periodically performs network discovery and updates the data.
- Provides data to the frontend via an API.

### Frontend
- Written in **React**.
- Displays a graph-based visualization of the network.
- Includes a search bar for filtering containers and networks.
- Provides a toggle for light/dark mode.
- Allows manual discovery trigger via a button.
- Displays visual indicators for discovery status and connection issues.

---

## Data Handling

### Input
- Reads configuration from a YAML file.
- Interacts with the Docker API to retrieve container and network data.

### Output
- Logs discovery and monitoring activities to a file and the console.
- Logs performance metrics to a separate file.

---

## Error Handling Strategies
- **Configuration Validation**:
  - Checks for missing or invalid configuration on startup.
  - Exits with a clear error message if validation fails.
- **Docker Environment Issues**:
  - Detects if the Docker socket or reverse proxy becomes unavailable.
  - Automatically attempts to reconnect.
  - Displays a warning in the frontend if the connection is lost.
- **Frontend Errors**:
  - Displays error messages if the graph fails to load or if manual discovery encounters an issue.

---

## Testing Plan

### Unit Tests
- Validate configuration file parsing and error handling.
- Test Docker API interactions for retrieving container and network data.
- Ensure proper logging of discovery and performance metrics.

### Integration Tests
- Verify end-to-end functionality of periodic discovery and data updates.
- Test frontend-backend communication for graph visualization.
- Simulate Docker environment issues (e.g., socket becoming unavailable) and verify reconnection logic.

### Frontend Tests
- Test graph rendering with sample data.
- Verify search functionality for partial matches and advanced filters.
- Ensure theme toggle works and saves the preference correctly.

### Deployment Tests
- Verify the Docker container runs correctly with the required configuration.
- Test health check functionality for service availability.

---

## Future Features (Not Included in Initial Version)
- Alerts and notifications for specific events (e.g., container down, network changes).
- System-level resource usage monitoring (e.g., CPU, memory, disk).
- Role-based access control for the frontend.
- Export functionality for network data.
- Safeguards for excessive resource usage (e.g., limiting discovery frequency).
- Localization for multiple languages.
- Historical data or timeline of network changes.

---

## Default Configuration Example (YAML)
```yaml
discovery_interval: 30 # in seconds
docker_socket_path: "/var/run/docker.sock"
reverse_proxy_url: "" # Leave empty if not using a reverse proxy
logging_level: "info"
log_file_path: "/var/log/docker-network-tool.log"
metrics_log_file_path: "/var/log/docker-network-metrics.log"
frontend_theme: "light" # Options: "light", "dark"
auth:
  username: "admin"
  password_hash: "hashed_password_here"
```

---

## Notes
This specification is designed to be developer-ready, providing all necessary details for implementation. Future features can be added iteratively based on user feedback and requirements.