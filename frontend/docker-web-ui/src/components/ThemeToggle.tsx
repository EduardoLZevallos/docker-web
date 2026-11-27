import './ThemeToggle.css';

interface ThemeToggleProps {
  isDarkMode: boolean;
  onToggle: () => void;
}

function ThemeToggle({ isDarkMode, onToggle }: ThemeToggleProps) {
  return (
    <button
      type="button"
      className={`theme-toggle ${isDarkMode ? 'dark' : 'light'}`}
      onClick={onToggle}
      aria-pressed={isDarkMode}
      aria-label="Dark mode"
      title={isDarkMode ? 'Switch to light theme' : 'Switch to dark theme'}
      data-testid="theme-toggle"
    >
      <span className="toggle-track" aria-hidden="true">
        <span className="toggle-thumb" />
      </span>
      <span className="toggle-text">Dark mode</span>
    </button>
  );
}

export default ThemeToggle;
