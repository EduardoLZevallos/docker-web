import React from 'react';
import './ThemeToggle.css';

interface ThemeToggleProps {
  isDarkMode: boolean;
  onToggle: () => void;
}

const ThemeToggle: React.FC<ThemeToggleProps> = ({ isDarkMode, onToggle }) => {
  return (
    <div className="theme-toggle">
      <button
        onClick={onToggle}
        className={`toggle-button ${isDarkMode ? 'dark' : 'light'}`}
        title={`Switch to ${isDarkMode ? 'light' : 'dark'} mode`}
      >
        <span className="toggle-icon">
          {isDarkMode ? '☀️' : '🌙'}
        </span>
        <span className="toggle-text">
          {isDarkMode ? 'Light' : 'Dark'}
        </span>
      </button>
    </div>
  );
};

export default ThemeToggle;