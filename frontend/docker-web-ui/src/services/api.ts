import { Container, Network } from '../types';

const API_BASE_URL = process.env.REACT_APP_API_URL || 'http://localhost:8080/api';

export const fetchContainerData = async (): Promise<Container[]> => {
  try {
    const response = await fetch(`${API_BASE_URL}/containers`);
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    
    const data = await response.json();
    return data;
  } catch (error) {
    console.error('Error fetching container data:', error);
    // Return mock data for development when backend is not available
    return getMockContainers();
  }
};

export const fetchNetworkData = async (): Promise<Network[]> => {
  try {
    const response = await fetch(`${API_BASE_URL}/networks`);
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    
    const data = await response.json();
    return data;
  } catch (error) {
    console.error('Error fetching network data:', error);
    // Return mock data for development when backend is not available
    return getMockNetworks();
  }
};

// Mock data for development
const getMockContainers = (): Container[] => [
  {
    id: 'container1',
    name: 'web-server',
    image: 'nginx:latest',
    status: 'running',
    created: Date.now() - 86400000, // 1 day ago
    ports: ['80:8080/tcp'],
    networks: ['bridge', 'web-network']
  },
  {
    id: 'container2',
    name: 'database',
    image: 'postgres:13',
    status: 'running',
    created: Date.now() - 172800000, // 2 days ago
    ports: ['5432:5432/tcp'],
    networks: ['bridge', 'db-network']
  },
  {
    id: 'container3',
    name: 'redis-cache',
    image: 'redis:alpine',
    status: 'running',
    created: Date.now() - 259200000, // 3 days ago
    ports: ['6379:6379/tcp'],
    networks: ['bridge']
  }
];

const getMockNetworks = (): Network[] => [
  {
    id: 'network1',
    name: 'bridge',
    driver: 'bridge',
    containers: ['container1', 'container2', 'container3']
  },
  {
    id: 'network2',
    name: 'web-network',
    driver: 'bridge',
    containers: ['container1']
  },
  {
    id: 'network3',
    name: 'db-network',
    driver: 'bridge',
    containers: ['container2']
  }
];