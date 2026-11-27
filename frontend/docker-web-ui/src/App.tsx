import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import DetailsPanel from './components/DetailsPanel';
import GraphVisualization from './components/GraphVisualization';
import SearchBar from './components/SearchBar';
import SubnetOverlapPanel from './components/SubnetOverlapPanel';
import ThemeToggle from './components/ThemeToggle';
import TopologyFilters from './components/TopologyFilters';
import TopologyList from './components/TopologyList';
import { ApiError, fetchRuntimeConfig, fetchTopology } from './services/api';
import type {
  LastSuccess,
  NodeSelection,
  RuntimeConfig,
  ThemeName,
  Topology,
} from './types';
import { filterTopology, formatTimestamp } from './utils/topology';
import './App.css';

const THEME_STORAGE_KEY = 'docker-web-theme';
const FALLBACK_INTERVAL_SECONDS = 30;

function readStoredTheme(): ThemeName | null {
  try {
    const value = window.localStorage.getItem(THEME_STORAGE_KEY);
    return value === 'light' || value === 'dark' ? value : null;
  } catch {
    return null;
  }
}

function persistTheme(theme: ThemeName): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    return;
  }
}

function normalizeDiscoveryInterval(config: RuntimeConfig | null): number {
  const interval = config?.discovery_interval;
  return typeof interval === 'number' &&
    Number.isFinite(interval) &&
    interval > 0
    ? interval
    : FALLBACK_INTERVAL_SECONDS;
}

function describeError(error: unknown): string {
  if (error instanceof ApiError) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function App() {
  const storedThemeRef = useRef<ThemeName | null>(readStoredTheme());
  const [theme, setTheme] = useState<ThemeName>(
    storedThemeRef.current ?? 'light',
  );
  const [config, setConfig] = useState<RuntimeConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [topology, setTopology] = useState<Topology | null>(null);
  const [topologyError, setTopologyError] = useState<string | null>(null);
  const [lastSuccess, setLastSuccess] = useState<LastSuccess | null>(null);
  const [query, setQuery] = useState('');
  const [networkId, setNetworkId] = useState('');
  const [publishedOnly, setPublishedOnly] = useState(false);
  const [selection, setSelection] = useState<NodeSelection | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(true);
  const [isInitialLoad, setIsInitialLoad] = useState(true);

  const configRef = useRef<RuntimeConfig | null>(null);
  const intervalRef = useRef(FALLBACK_INTERVAL_SECONDS);
  const timerRef = useRef<number | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const refreshInFlightRef = useRef(false);
  const mountedRef = useRef(false);

  const runRefreshCycle = useCallback(async () => {
    if (refreshInFlightRef.current || !mountedRef.current) {
      return;
    }

    refreshInFlightRef.current = true;
    setIsRefreshing(true);

    const controller = new AbortController();
    abortRef.current = controller;
    let nextInterval = intervalRef.current;

    try {
      let nextConfig: RuntimeConfig | null = null;

      try {
        nextConfig = await fetchRuntimeConfig(controller.signal);
      } catch (error) {
        if (controller.signal.aborted) {
          return;
        }
        nextInterval = normalizeDiscoveryInterval(configRef.current);
        setConfigError(describeError(error));
      }

      if (controller.signal.aborted) {
        return;
      }

      if (nextConfig) {
        configRef.current = nextConfig;
        nextInterval = normalizeDiscoveryInterval(nextConfig);
        setConfig(nextConfig);
        setConfigError(null);
        if (!storedThemeRef.current) {
          setTheme(nextConfig.theme);
        }
      }

      try {
        const nextTopology = await fetchTopology(controller.signal);
        if (controller.signal.aborted) {
          return;
        }
        setTopology(nextTopology);
        setTopologyError(null);
        setLastSuccess({
          timestamp: nextTopology.timestamp,
          receivedAt: Date.now(),
        });
      } catch (error) {
        if (controller.signal.aborted) {
          return;
        }
        setTopologyError(describeError(error));
      }
    } finally {
      abortRef.current = null;
      refreshInFlightRef.current = false;

      if (mountedRef.current) {
        setIsInitialLoad(false);
        setIsRefreshing(false);
        intervalRef.current = nextInterval;

        if (timerRef.current !== null) {
          window.clearTimeout(timerRef.current);
        }
        timerRef.current = window.setTimeout(() => {
          timerRef.current = null;
          void runRefreshCycle();
        }, nextInterval * 1000);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void runRefreshCycle();

    return () => {
      mountedRef.current = false;
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      abortRef.current?.abort();
    };
  }, [runRefreshCycle]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
  }, [theme]);

  useEffect(() => {
    if (!networkId || !topology) {
      return;
    }
    if (!topology.networks.some((network) => network.id === networkId)) {
      setNetworkId('');
    }
  }, [networkId, topology]);

  const pinnedNetworkId =
    selection?.kind === 'network' ? selection.id : null;

  const display = useMemo(
    () =>
      topology
        ? filterTopology(topology, query, {
            networkId: networkId || null,
            publishedOnly,
            pinnedNetworkId,
          })
        : null,
    [topology, query, networkId, publishedOnly, pinnedNetworkId],
  );

  useEffect(() => {
    if (!selection) {
      return;
    }

    const exists = display
      ? selection.kind === 'container'
        ? display.topology.containers.some(
            (container) => container.id === selection.id,
          )
        : Boolean(
            topology?.networks.some((network) => network.id === selection.id),
          )
      : false;

    if (!exists) {
      setSelection(null);
    }
  }, [display, selection, topology]);

  const handleRefresh = useCallback(() => {
    if (refreshInFlightRef.current) {
      return;
    }
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    void runRefreshCycle();
  }, [runRefreshCycle]);

  const handleThemeToggle = useCallback(() => {
    const nextTheme: ThemeName = theme === 'dark' ? 'light' : 'dark';
    storedThemeRef.current = nextTheme;
    persistTheme(nextTheme);
    setTheme(nextTheme);
  }, [theme]);

  const handleSelect = useCallback(
    (nextSelection: NodeSelection | null) => {
      setSelection(nextSelection);
    },
    [],
  );

  const handleClearFilters = useCallback(() => {
    setQuery('');
    setNetworkId('');
    setPublishedOnly(false);
  }, []);

  const handleSelectOverlapNetwork = useCallback((overlapNetworkId: string) => {
    setQuery('');
    setNetworkId('');
    setPublishedOnly(false);
    setSelection({ kind: 'network', id: overlapNetworkId });
  }, []);

  const intervalSeconds = normalizeDiscoveryInterval(config);
  const totalContainers =
    display?.totalContainers ?? topology?.containers.length ?? 0;
  const totalNetworks = display?.totalNetworks ?? topology?.networks.length ?? 0;
  const visibleContainers = display?.visibleContainers ?? 0;
  const visibleNetworks = display?.visibleNetworks ?? 0;
  const filtersActive = display?.active ?? false;
  const noMatches = Boolean(filtersActive && display && !display.hasMatches);
  const hasAdvancedFilters = Boolean(networkId || publishedOnly);
  const selectedNetwork =
    topology?.networks.find((network) => network.id === networkId) ?? null;

  const emptyContainerMessage = display?.active
    ? hasAdvancedFilters
      ? 'No containers match the current filters.'
      : 'No containers match the current search.'
    : topology
      ? 'No containers returned by the backend.'
      : 'Waiting for topology data.';
  const emptyNetworkMessage = display?.active
    ? hasAdvancedFilters
      ? 'No networks match the current filters.'
      : 'No networks are connected to the matching containers.'
    : topology
      ? 'No networks returned by the backend.'
      : 'Waiting for topology data.';

  const graphEmptyState = noMatches
    ? {
        testId: 'search-no-results',
        title: 'No containers match',
        description: hasAdvancedFilters
          ? 'No container matches the selected network membership and published-binding filters.'
          : `No container name or raw status contains "${display?.query ?? ''}".`,
      }
    : topology
      ? {
          testId: 'topology-graph-empty',
          title: 'No containers or networks',
          description: 'The backend returned an empty topology response.',
        }
      : topologyError
        ? {
            testId: 'topology-graph-empty',
            title: 'No live topology available',
            description:
              'The last topology request failed. Automatic retry remains active.',
          }
        : {
            testId: 'topology-graph-loading',
            title: 'Loading live topology',
            description: 'Connecting to /api/config and /api/topology.',
          };

  const filterDescriptions: string[] = [];
  if (selectedNetwork) {
    filterDescriptions.push(
      `network ${selectedNetwork.name || selectedNetwork.id} (${selectedNetwork.id})`,
    );
  }
  if (publishedOnly) {
    filterDescriptions.push('published bindings');
  }
  if (display?.query) {
    filterDescriptions.push(`search "${display.query}"`);
  }

  const searchSummary = filtersActive
    ? hasAdvancedFilters
      ? `Showing ${visibleContainers} of ${totalContainers} containers and ${visibleNetworks} of ${totalNetworks} networks. Active filters: ${filterDescriptions.join(', ')}.`
      : `Showing ${visibleContainers} of ${totalContainers} containers and ${visibleNetworks} of ${totalNetworks} networks. Search: "${display?.query ?? ''}".`
    : `Showing all ${totalContainers} containers and ${totalNetworks} networks.`;

  return (
    <div
      className={`app ${theme === 'dark' ? 'dark-mode' : 'light-mode'}`}
      data-testid="app-root"
      data-theme={theme}
    >
      <header className="app-header">
        <div className="app-title-block">
          <h1>Docker Network Discovery</h1>
          <p className="app-subtitle">
            Live container and network topology from the backend API.
          </p>
        </div>
        <div className="header-controls">
          <SearchBar value={query} onChange={setQuery} />
          <ThemeToggle
            isDarkMode={theme === 'dark'}
            onToggle={handleThemeToggle}
          />
        </div>
      </header>

      <main className="app-main">
        <div className="notice-stack">
          {isInitialLoad && (
            <div
              className="notice notice-info"
              role="status"
              data-testid="loading-indicator"
            >
              Loading live topology...
            </div>
          )}

          {configError && (
            <div
              className="notice notice-warning config-warning"
              role="alert"
              data-testid="config-warning"
            >
              <div>
                <strong>Backend configuration unavailable.</strong>{' '}
                {config
                  ? 'Using the last known configuration and retrying automatically.'
                  : storedThemeRef.current
                    ? `Using your saved theme preference and a ${FALLBACK_INTERVAL_SECONDS}-second fallback interval.`
                    : `Using the light theme and a ${FALLBACK_INTERVAL_SECONDS}-second fallback interval.`}{' '}
                <span className="notice-detail">{configError}</span>
              </div>
              <button
                type="button"
                className="notice-action"
                onClick={handleRefresh}
                disabled={isRefreshing}
                data-testid="config-retry-button"
              >
                Retry configuration
              </button>
            </div>
          )}

          {config?.demo_mode && (
            <div
              className="notice notice-info"
              role="status"
              data-testid="demo-mode-notice"
            >
              Backend demo mode is enabled. This UI still renders only live API
              responses and never substitutes mock topology data.
            </div>
          )}

          {topologyError && (
            <div
              className="notice notice-error"
              role="alert"
              data-testid="topology-error"
            >
              <strong>Topology refresh failed.</strong>{' '}
              {topology
                ? 'The last successful topology remains visible.'
                : 'No topology data is available yet.'}{' '}
              <span className="notice-detail">{topologyError}</span>
            </div>
          )}
        </div>

        <div className="filter-toolbar" data-testid="filter-toolbar">
          <TopologyFilters
            networks={topology?.networks ?? []}
            networkId={networkId}
            publishedOnly={publishedOnly}
            onChangeNetwork={setNetworkId}
            onChangePublishedOnly={setPublishedOnly}
            onReset={handleClearFilters}
          />
        </div>

        <SubnetOverlapPanel
          topology={topology}
          selection={selection}
          stale={Boolean(topologyError && topology)}
          onSelectNetwork={handleSelectOverlapNetwork}
        />

        <div className="workspace">
          <div className="graph-panel">
            <GraphVisualization
              topology={display?.topology ?? null}
              theme={theme}
              selection={selection}
              onSelect={handleSelect}
              emptyState={graphEmptyState}
            />
          </div>

          <aside className="sidebar" aria-label="Topology browser">
            <TopologyList
              containers={display?.topology.containers ?? []}
              networks={display?.topology.networks ?? []}
              selection={selection}
              onSelect={handleSelect}
              emptyContainerMessage={emptyContainerMessage}
              emptyNetworkMessage={emptyNetworkMessage}
            />
            <DetailsPanel topology={topology} selection={selection} />
          </aside>
        </div>

        <footer className="status-bar">
          <div className="status-counts">
            <span data-testid="count-total-containers">
              Containers total: {totalContainers}
            </span>
            <span data-testid="count-visible-containers">
              Containers visible: {visibleContainers}
            </span>
            <span data-testid="count-total-networks">
              Networks total: {totalNetworks}
            </span>
            <span data-testid="count-visible-networks">
              Networks visible: {visibleNetworks}
            </span>
          </div>

          <div className="status-meta">
            <span data-testid="search-summary" aria-live="polite">
              {searchSummary}
            </span>
            <span data-testid="last-success-timestamp">
              Last successful update:{' '}
              {lastSuccess ? (
                <time
                  dateTime={lastSuccess.timestamp}
                  title={`Response received ${new Date(lastSuccess.receivedAt).toLocaleTimeString()}`}
                >
                  {formatTimestamp(lastSuccess.timestamp)}
                </time>
              ) : (
                'Never'
              )}
            </span>
            <span data-testid="refresh-interval">
              Automatic refresh every {intervalSeconds} seconds
            </span>
            {isRefreshing && !isInitialLoad && (
              <span role="status" data-testid="refreshing-indicator">
                Refreshing...
              </span>
            )}
          </div>

          <div className="status-actions">
            <button
              type="button"
              className="refresh-button"
              onClick={handleRefresh}
              disabled={isRefreshing}
              aria-busy={isRefreshing}
              data-testid="refresh-button"
            >
              {isRefreshing ? 'Refreshing...' : 'Refresh'}
            </button>
          </div>
        </footer>
      </main>
    </div>
  );
}

export default App;
