import type { ChangeEvent } from 'react';
import './SearchBar.css';

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
}

function SearchBar({ value, onChange }: SearchBarProps) {
  const handleInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    onChange(event.target.value);
  };

  const handleClear = () => {
    onChange('');
  };

  return (
    <div className="search-bar">
      <label className="sr-only" htmlFor="topology-search">
        Search containers by name or status
      </label>
      <div className="search-input-container">
        <input
          id="topology-search"
          type="search"
          value={value}
          onChange={handleInputChange}
          placeholder="Search containers by name or status"
          className="search-input"
          autoComplete="off"
          spellCheck={false}
          data-testid="search-input"
        />
        {value.length > 0 && (
          <button
            type="button"
            onClick={handleClear}
            className="clear-button"
            aria-label="Clear search"
            data-testid="search-clear"
          >
            ×
          </button>
        )}
      </div>
    </div>
  );
}

export default SearchBar;
