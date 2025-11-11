import React, { useEffect, useRef } from 'react';
import cytoscape from 'cytoscape';
import { Container } from '../types';
import './GraphVisualization.css';

interface GraphVisualizationProps {
  containers: Container[];
  isDarkMode: boolean;
}

const GraphVisualization: React.FC<GraphVisualizationProps> = ({ 
  containers, 
  isDarkMode 
}) => {
  const cyRef = useRef<HTMLDivElement>(null);
  const cyInstance = useRef<cytoscape.Core | null>(null);

  useEffect(() => {
    if (!cyRef.current) return;

    // Initialize Cytoscape instance
    cyInstance.current = cytoscape({
      container: cyRef.current,
      elements: generateGraphElements(containers),
      style: getGraphStyle(isDarkMode),
      layout: {
        name: 'breadthfirst',
        directed: false,
        spacingFactor: 2,
        avoidOverlap: true,
        padding: 20
      },
      minZoom: 0.1,
      maxZoom: 3,
      wheelSensitivity: 0.1
    });

    // Add event listeners
    cyInstance.current.on('tap', 'node', (event) => {
      const node = event.target;
      const data = node.data();
      console.log('Node clicked:', data);
      
      // Highlight connected nodes
      const connectedEdges = node.connectedEdges();
      const connectedNodes = connectedEdges.connectedNodes();
      
      cyInstance.current?.elements().removeClass('highlighted');
      node.addClass('highlighted');
      connectedNodes.addClass('highlighted');
      connectedEdges.addClass('highlighted');
    });

    // Cleanup function
    return () => {
      if (cyInstance.current) {
        cyInstance.current.destroy();
      }
    };
  }, [containers, isDarkMode]);

  const generateGraphElements = (containers: Container[]) => {
    const elements: any[] = [];

    // Add container nodes
    containers.forEach(container => {
      elements.push({
        data: {
          id: container.id,
          label: container.name,
          type: 'container',
          status: container.status,
          image: container.image,
          ports: container.ports,
          networks: container.networks
        }
      });
    });

    // Create a set of unique networks
    const networks = new Set<string>();
    containers.forEach(container => {
      container.networks.forEach(network => {
        networks.add(network);
      });
    });

    // Add network nodes
    networks.forEach(network => {
      elements.push({
        data: {
          id: `network-${network}`,
          label: network,
          type: 'network'
        }
      });
    });

    // Add edges (connections between containers and networks)
    containers.forEach(container => {
      container.networks.forEach(network => {
        elements.push({
          data: {
            id: `${container.id}-${network}`,
            source: container.id,
            target: `network-${network}`,
            type: 'connection'
          }
        });
      });
    });

    return elements;
  };

  const getGraphStyle = (isDarkMode: boolean): any => {
    const backgroundColor = isDarkMode ? '#1a1a1a' : '#ffffff';
    const nodeColor = isDarkMode ? '#4a90e2' : '#2c5aa0';
    const networkColor = isDarkMode ? '#e74c3c' : '#c0392b';
    const textColor = isDarkMode ? '#ffffff' : '#333333';
    const edgeColor = isDarkMode ? '#555555' : '#cccccc';

    return [
      {
        selector: 'node[type="container"]',
        style: {
          'background-color': nodeColor,
          'label': 'data(label)',
          'text-halign': 'center',
          'text-valign': 'center',
          'color': textColor,
          'font-size': '12px',
          'font-family': 'Arial, sans-serif',
          'text-outline-width': 2,
          'text-outline-color': backgroundColor,
          'width': '60px',
          'height': '60px',
          'shape': 'round-rectangle'
        }
      },
      {
        selector: 'node[type="network"]',
        style: {
          'background-color': networkColor,
          'label': 'data(label)',
          'text-halign': 'center',
          'text-valign': 'center',
          'color': textColor,
          'font-size': '12px',
          'font-family': 'Arial, sans-serif',
          'text-outline-width': 2,
          'text-outline-color': backgroundColor,
          'width': '50px',
          'height': '50px',
          'shape': 'diamond'
        }
      },
      {
        selector: 'edge',
        style: {
          'width': 2,
          'line-color': edgeColor,
          'curve-style': 'bezier'
        }
      },
      {
        selector: '.highlighted',
        style: {
          'border-width': 3,
          'border-color': '#ffeb3b',
          'line-color': '#ffeb3b',
          'target-arrow-color': '#ffeb3b'
        }
      },
      {
        selector: 'node[status="running"]',
        style: {
          'border-width': 3,
          'border-color': '#4caf50'
        }
      },
      {
        selector: 'node[status="stopped"]',
        style: {
          'border-width': 3,
          'border-color': '#f44336'
        }
      }
    ];
  };

  return (
    <div className={`graph-visualization ${isDarkMode ? 'dark' : 'light'}`}>
      <div className="graph-header">
        <h2>Container Network Topology</h2>
        <div className="graph-legend">
          <div className="legend-item">
            <div className="legend-icon container-icon"></div>
            <span>Container</span>
          </div>
          <div className="legend-item">
            <div className="legend-icon network-icon"></div>
            <span>Network</span>
          </div>
        </div>
      </div>
      <div ref={cyRef} className="cytoscape-container" />
      {containers.length === 0 && (
        <div className="empty-state">
          <h3>No containers found</h3>
          <p>Connect to Docker daemon to view container topology</p>
        </div>
      )}
    </div>
  );
};

export default GraphVisualization;