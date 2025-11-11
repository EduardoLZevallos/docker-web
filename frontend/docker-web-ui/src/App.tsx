import React, { useState, useEffect } from 'react';
import './App.css';
import GraphVisualization from './components/GraphVisualization';
import SearchBar from './components/SearchBar';
import ThemeToggle from './components/ThemeToggle';
import { fetchContainerData } from './services/api';
import { Container } from './types';

function App() {
  const [containers, setContainers] = useState<Container[]>([]);
  const [filteredContainers, setFilteredContainers] = useState<Container[]>([]);
  const [darkMode, setDarkMode] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadContainerData();
  }, []);

  useEffect(() => {
    // Filter containers based on search term
    const filtered = containers.filter(container =>
      container.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      container.status.toLowerCase().includes(searchTerm.toLowerCase())
    );
    setFilteredContainers(filtered);
  }, [containers, searchTerm]);

  const loadContainerData = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchContainerData();
      setContainers(data);
    } catch (err) {
      setError('Failed to fetch container data');
      console.error('Error fetching container data:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleThemeToggle = () => {
    setDarkMode(!darkMode);
  };

  const handleSearch = (term: string) => {
    setSearchTerm(term);
  };

  return (
    <div className={`app ${darkMode ? 'dark-mode' : 'light-mode'}`}>
      <header className="app-header">
        <h1>Docker Network Discovery Tool</h1>
        <div className="header-controls">
          <SearchBar onSearch={handleSearch} />
          <ThemeToggle isDarkMode={darkMode} onToggle={handleThemeToggle} />
        </div>
      </header>
      
      <main className="app-main">
        {loading && <div className="loading">Loading container data...</div>}
        {error && <div className="error">{error}</div>}
        
        <div className="graph-container">
          <GraphVisualization 
            containers={filteredContainers} 
            isDarkMode={darkMode}
          />
        </div>
        
        <div className="status-bar">
          <span>Total Containers: {containers.length}</span>
          {searchTerm && <span>Filtered: {filteredContainers.length}</span>}
          <button onClick={loadContainerData} disabled={loading}>
            Refresh Data
          </button>
        </div>
      </main>
    </div>
  );
}

export default App;
