# Project Checklist for Docker Network Discovery, Visualization, and Monitoring Tool

## Phase 1: Project Initialization
- [ ] Create directories: `backend`, `frontend`, `config`, `tests`.
- [ ] Initialize a Rust project in the `backend` directory.
- [ ] Initialize a React project with TypeScript in the `frontend` directory.
- [ ] Add a `.gitignore` file to ignore unnecessary files.

---

## Phase 2: YAML Configuration
- [ ] Define a YAML schema with the following fields:
  - [ ] `discovery_interval` (integer, default: 30 seconds).
  - [ ] `docker_socket_path` (string, default: "/var/run/docker.sock").
  - [ ] `reverse_proxy_url` (string, optional).
  - [ ] `logging_level` (string, default: "info").
  - [ ] `log_file_path` (string, default: "/var/log/docker-network-tool.log").
  - [ ] `metrics_log_file_path` (string, default: "/var/log/docker-network-metrics.log").
  - [ ] `frontend_theme` (string, default: "light").
  - [ ] `auth` (object with `username` and `password_hash`).
- [ ] Implement configuration parsing and validation in the backend.

---

## Phase 3: Docker API Interaction
- [ ] Write a Rust module to interact with the Docker API.
- [ ] Implement a function to list all running containers and their details (e.g., name, ID, status).

---

## Phase 4: Backend API
- [ ] Set up a REST API in the Rust backend using Actix or Rocket.
- [ ] Add an endpoint `/api/containers` to return a list of running containers and their details in JSON format.

---

## Phase 5: Frontend Initialization
- [ ] Create a basic UI layout with placeholders for:
  - [ ] A graph visualization area.
  - [ ] A search bar.
  - [ ] A theme toggle button.
- [ ] Add a placeholder API call to fetch container data from the backend.

---

## Phase 6: Network Discovery
- [ ] Extend the Docker API module to retrieve network configurations and relationships.
- [ ] Implement periodic discovery logic in the backend with a configurable interval.

---

## Phase 7: Graph Visualization
- [ ] Use a graph visualization library (e.g., D3.js or Cytoscape) in the React frontend.
- [ ] Display container and network relationships as nodes and edges.
- [ ] Add hover or click functionality to show metadata for each node.

---

## Phase 8: Authentication
- [ ] Implement username and password-based authentication in the Rust backend.
- [ ] Use credentials from the YAML configuration file.
- [ ] Secure the frontend with a login page.

---

## Phase 9: Logging and Metrics
- [ ] Add logging to the backend for discovery activities and performance metrics.
- [ ] Write logs to both the console and files specified in the YAML configuration.
- [ ] Display visual indicators in the frontend for discovery status and connection issues.

---

## Phase 10: Deployment
- [ ] Create a Dockerfile to containerize the project.
- [ ] Expose port 3000 for the frontend.
- [ ] Add a health check endpoint in the backend to verify service availability.

---

## Phase 11: Testing and Validation
### Unit Tests
- [ ] Test configuration parsing and validation.
- [ ] Test Docker API interactions for retrieving container and network data.
- [ ] Test logging functionality.

### Integration Tests
- [ ] Verify end-to-end functionality of periodic discovery and data updates.
- [ ] Test frontend-backend communication for graph visualization.
- [ ] Simulate Docker environment issues and verify reconnection logic.

### Frontend Tests
- [ ] Test graph rendering with sample data.
- [ ] Verify search functionality for partial matches and advanced filters.
- [ ] Ensure theme toggle works and saves the preference correctly.

### Deployment Tests
- [ ] Verify the Docker container runs correctly with the required configuration.
- [ ] Test health check functionality for service availability.

---

## Future Features (Optional)
- [ ] Alerts and notifications for specific events (e.g., container down, network changes).
- [ ] System-level resource usage monitoring (e.g., CPU, memory, disk).
- [ ] Role-based access control for the frontend.
- [ ] Export functionality for network data.
- [ ] Safeguards for excessive resource usage (e.g., limiting discovery frequency).
- [ ] Localization for multiple languages.
- [ ] Historical data or timeline of network changes.
