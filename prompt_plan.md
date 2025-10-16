# Prompt Plan for Docker Network Discovery, Visualization, and Monitoring Tool

## Step-by-Step Blueprint

### Phase 1: Project Initialization
1. **Set up the project structure**:
   - Create directories for `backend`, `frontend`, `config`, and `tests`.
   - Initialize a Rust project for the backend and a React project for the frontend.
   - Add a `.gitignore` file for ignoring unnecessary files.

2. **Define the YAML configuration schema**:
   - Create a YAML schema for the configuration file.
   - Implement configuration parsing and validation in the backend.

3. **Set up Docker API interaction**:
   - Write a Rust module to interact with the Docker API.
   - Implement basic functionality to list running containers.

4. **Create a basic API for the backend**:
   - Set up a REST API using a Rust web framework (e.g., Actix or Rocket).
   - Add an endpoint to fetch container and network data.

5. **Set up the frontend project**:
   - Initialize a React project with TypeScript.
   - Create a basic UI layout with placeholders for the graph and controls.

---

### Phase 2: Core Features Development
1. **Implement network discovery**:
   - Extend the Docker API module to retrieve network configurations and relationships.
   - Add periodic discovery logic with a configurable interval.

2. **Build the graph-based visualization**:
   - Use a graph visualization library (e.g., D3.js or Cytoscape) in the frontend.
   - Display container and network relationships as nodes and edges.

3. **Add metadata display**:
   - Show container and network details on hover or click in the graph.

4. **Implement search functionality**:
   - Add a search bar to filter containers and networks by name or status.

5. **Add theme toggle**:
   - Implement light and dark modes in the frontend.
   - Save the user’s preference in the configuration file.

---

### Phase 3: Advanced Features
1. **Add authentication**:
   - Implement username and password-based authentication in the backend.
   - Secure the frontend with a login page.

2. **Implement logging and metrics**:
   - Log discovery activities and performance metrics in the backend.
   - Display visual indicators for discovery status in the frontend.

3. **Handle errors and reconnections**:
   - Add error handling for Docker environment issues.
   - Implement automatic reconnection logic.

4. **Add deployment support**:
   - Create a Dockerfile for the project.
   - Add a health check for service availability.

---

### Phase 4: Testing and Validation
1. **Write unit tests**:
   - Test configuration parsing, Docker API interactions, and logging.
   - Test frontend components and graph rendering.

2. **Write integration tests**:
   - Test end-to-end functionality of discovery and visualization.
   - Simulate Docker environment issues and verify reconnection logic.

3. **Deployment tests**:
   - Verify the Docker container runs correctly with the required configuration.
   - Test the health check endpoint.

---

## Iterative Chunks and Prompts

### Chunk 1: Project Initialization
**Prompt**:
```
Create a directory structure for the project with the following layout:
- `backend`: Rust backend code.
- `frontend`: React frontend code.
- `config`: YAML configuration files.
- `tests`: Unit and integration tests.

Initialize a Rust project in the `backend` directory and a React project in the `frontend` directory. Add a `.gitignore` file to ignore unnecessary files.
```

---

### Chunk 2: YAML Configuration Parsing
**Prompt**:
```
Define a YAML schema for the configuration file with the following fields:
- `discovery_interval` (integer, default: 30 seconds).
- `docker_socket_path` (string, default: "/var/run/docker.sock").
- `reverse_proxy_url` (string, optional).
- `logging_level` (string, default: "info").
- `log_file_path` (string, default: "/var/log/docker-network-tool.log").
- `metrics_log_file_path` (string, default: "/var/log/docker-network-metrics.log").
- `frontend_theme` (string, default: "light").
- `auth` (object with `username` and `password_hash`).

Implement a Rust module to parse and validate this configuration file.
```

---

### Chunk 3: Docker API Interaction
**Prompt**:
```
Write a Rust module to interact with the Docker API. Implement a function to list all running containers and their basic details (e.g., name, ID, status). Use the Docker socket path from the configuration file.
```

---

### Chunk 4: Backend API
**Prompt**:
```
Set up a REST API in the Rust backend using Actix or Rocket. Add an endpoint `/api/containers` that returns a list of running containers and their details in JSON format.
```

---

### Chunk 5: Frontend Initialization
**Prompt**:
```
Initialize a React project with TypeScript in the `frontend` directory. Create a basic UI layout with placeholders for:
- A graph visualization area.
- A search bar.
- A theme toggle button.

Add a placeholder API call to fetch container data from the backend.
```

---

### Chunk 6: Network Discovery
**Prompt**:
```
Extend the Docker API module to retrieve network configurations and relationships. Implement periodic discovery logic in the backend with a configurable interval from the YAML configuration file.
```

---

### Chunk 7: Graph Visualization
**Prompt**:
```
Use a graph visualization library (e.g., D3.js or Cytoscape) in the React frontend. Display container and network relationships as nodes and edges. Add hover or click functionality to show metadata for each node.
```

---

### Chunk 8: Authentication
**Prompt**:
```
Implement username and password-based authentication in the Rust backend. Use the credentials from the YAML configuration file. Secure the frontend with a login page that authenticates users before accessing the graph visualization.
```

---

### Chunk 9: Logging and Metrics
**Prompt**:
```
Add logging to the backend for discovery activities and performance metrics. Write logs to both the console and files specified in the YAML configuration. Display visual indicators in the frontend for discovery status and connection issues.
```

---

### Chunk 10: Deployment
**Prompt**:
```
Create a Dockerfile to containerize the project. Expose port 3000 for the frontend. Add a health check endpoint in the backend to verify service availability.
```

---

## Verification
After completing each chunk:
1. Run linting tools to ensure code quality.
2. Execute unit and integration tests to verify functionality.
3. Deploy the project locally using Docker and test its behavior.
