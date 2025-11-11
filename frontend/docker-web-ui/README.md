# Docker Network Discovery Tool - Frontend

A React TypeScript frontend for visualizing Docker container networks and relationships.

## Features

- **Graph Visualization**: Interactive network topology display using Cytoscape.js
- **Search Functionality**: Filter containers by name or status
- **Theme Toggle**: Switch between light and dark modes
- **Real-time Data**: Fetches container data from the Rust backend API
- **Responsive Design**: Works on desktop and mobile devices

## Getting Started

### Prerequisites

- Node.js 16+ 
- npm or yarn
- Running Docker Network Discovery backend API

### Installation

```bash
# Install dependencies
npm install

# Start development server
npm start
```

The application will be available at `http://localhost:3000`.

### Environment Variables

Create a `.env` file in the project root:

```bash
REACT_APP_API_URL=http://localhost:8080/api
REACT_APP_APP_NAME=Docker Network Discovery Tool
REACT_APP_VERSION=0.1.0
```

### Building for Production

```bash
npm run build
```

## API Integration

The frontend connects to the backend API endpoints:
- `GET /api/containers` - Fetch container list
- `GET /api/networks` - Fetch network list  
- `GET /api/health` - Health check

## Components

- **App.tsx** - Main application component with state management
- **GraphVisualization** - Cytoscape.js graph rendering
- **SearchBar** - Filter containers by search term
- **ThemeToggle** - Light/dark mode switcher

## Development

### Mock Data

When the backend API is unavailable, the app falls back to mock data to enable frontend development.

### Testing

```bash
npm test
```

### Available Scripts

- `npm start` - Start development server
- `npm run build` - Build for production
- `npm test` - Run tests
- `npm run eject` - Eject from Create React App