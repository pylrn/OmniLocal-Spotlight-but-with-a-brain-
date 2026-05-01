import { useCallback, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { openPath } from '@tauri-apps/plugin-opener';
import SearchBar from './components/SearchBar/SearchBar';
import ResultCard from './components/ResultCard/ResultCard';
import InsightPanel from './components/InsightPanel/InsightPanel';
import QuickSettings from './components/Settings/QuickSettings';
import AdvancedSettings from './components/Settings/Settings';
import type { SectionId } from './components/Settings/Settings';
import StatusBar from './components/StatusBar/StatusBar';
import { addManualFiles, queryWithContext, searchKeyword } from './lib/tauri';
import type { SearchResult } from './lib/tauri';
import './App.css';

export default function App() {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [showQuickSettings, setShowQuickSettings] = useState(false);
  const [showAdvancedSettings, setShowAdvancedSettings] = useState<SectionId | null>(null);
  const [hasSearched, setHasSearched] = useState(false);
  const [isIndexing, setIsIndexing] = useState(false);
  const [currentQuery, setCurrentQuery] = useState('');
  const [insight, setInsight] = useState<string | null>(null);
  const [insightLoading, setInsightLoading] = useState(false);
  const [insightError, setInsightError] = useState<string | null>(null);
  const [insightDismissed, setInsightDismissed] = useState(false);

  const handleAddFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const paths = await open({
        multiple: true,
        title: 'Select files to index',
      });

      if (paths && Array.isArray(paths) && paths.length > 0) {
        setIsIndexing(true);
        await addManualFiles(paths as string[]);
      }
    } catch (error) {
      console.error('Failed to add manual files', error);
    } finally {
      setIsIndexing(false);
    }
  }, []);

  const handleSearch = useCallback(async (query: string) => {
    if (!query) {
      setResults([]);
      setHasSearched(false);
      setInsight(null);
      setInsightError(null);
      return;
    }

    setCurrentQuery(query);
    setIsSearching(true);
    setHasSearched(true);
    setInsight(null);
    setInsightError(null);
    setInsightDismissed(false);

    try {
      const nextResults = await searchKeyword(query, 15);
      setResults(nextResults);

      // Trigger AI insight from top 5 results
      if (nextResults.length > 0) {
        setInsightLoading(true);
        const snippets = nextResults.slice(0, 5).map((r) => ({
          snippet: r.snippet,
          path: r.abs_path || r.path,
        }));
        queryWithContext(query, snippets)
          .then((answer) => setInsight(answer))
          .catch((err: unknown) =>
            setInsightError(err instanceof Error ? err.message : String(err)),
          )
          .finally(() => setInsightLoading(false));
      }
    } catch (error) {
      console.error('Search failed:', error);
      setResults([]);
    } finally {
      setIsSearching(false);
    }
  }, []);

  const handleResultClick = useCallback(async (result: SearchResult) => {
    try {
      await openPath(result.abs_path);
    } catch (error) {
      console.error('Failed to open file:', error);
    }
  }, []);

  const handleOpenSettings = (section?: SectionId) => {
    if (section && typeof section === 'string') {
      setShowAdvancedSettings(section);
      setShowQuickSettings(false);
    } else {
      setShowQuickSettings(true);
    }
  };

  return (
    <div className="appShell">
      <AuroraBackdrop />
      <div className="appFrame">
        <header className="appChrome">
          <div className="brandGroup">
            <svg className="brandMark" viewBox="0 0 200 200" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
              <path d="M30 100C30 54.7 66.7 18 112 18C136.5 18 158.5 29.1 173.3 46.5L140.5 53.4C131.9 45.3 120.3 40.4 107.5 40.4C81.3 40.4 60 61.7 60 87.9C60 114.1 81.3 135.4 107.5 135.4C111.8 135.4 116 134.8 120 133.6L104.8 164.8C59.5 162.1 30 134.8 30 100Z" fill="url(#bw)"/>
              <path d="M170 100C170 145.3 133.3 182 88 182C63.5 182 41.5 170.9 26.7 153.5L59.5 146.6C68.1 154.7 79.7 159.6 92.5 159.6C118.7 159.6 140 138.3 140 112.1C140 85.9 118.7 64.6 92.5 64.6C88.2 64.6 84 65.2 80 66.4L95.2 35.2C140.5 37.9 170 65.2 170 100Z" fill="url(#bc)"/>
              <circle cx="121" cy="76" r="18" fill="#FFD166"/>
              <circle cx="79" cy="124" r="22" fill="#4BE1F6"/>
              <defs>
                <linearGradient id="bw" x1="49" y1="27" x2="170" y2="146" gradientUnits="userSpaceOnUse"><stop stopColor="#FFDB6E"/><stop offset="1" stopColor="#FF9B45"/></linearGradient>
                <linearGradient id="bc" x1="24" y1="150" x2="152" y2="44" gradientUnits="userSpaceOnUse"><stop stopColor="#41E2F7"/><stop offset="1" stopColor="#54A9FF"/></linearGradient>
              </defs>
            </svg>
            <div className="brandText">
              <span className="brandEyebrow">Local semantic desktop search</span>
              <span className="brandTitle">smart-search</span>
            </div>
          </div>
        </header>

        <main className="appCanvas">
          <SearchBar
            onSearch={handleSearch}
            onAddFilesClick={handleAddFiles}
            isSearching={isSearching || isIndexing}
            resultCount={results.length}
          />

          <div className="mainContent">
            {!hasSearched ? (
              <EmptyState 
                onOpenSettings={() => handleOpenSettings()} 
                onOpenLibrary={() => handleOpenSettings('library')}
              />
            ) : results.length === 0 && !isSearching ? (
              <NoResults />
            ) : (
              <div className="resultsList">
                <AnimatePresence>
                  {(insightLoading || insight || insightError) && !insightDismissed && (
                    <InsightPanel
                      key="insight"
                      query={currentQuery}
                      isLoading={insightLoading}
                      insight={insight}
                      error={insightError}
                      onDismiss={() => setInsightDismissed(true)}
                      onOpenSettings={() => handleOpenSettings('intelligence')}
                    />
                  )}
                </AnimatePresence>
                <AnimatePresence mode="popLayout">
                  {results.map((result, index) => (
                    <ResultCard
                      key={`${result.document_id}-${result.chunk_id}`}
                      result={result}
                      index={index}
                      onClick={handleResultClick}
                    />
                  ))}
                </AnimatePresence>
              </div>
            )}
          </div>
        </main>

        <StatusBar onSettingsClick={handleOpenSettings} />
      </div>

      <AnimatePresence>
        {showQuickSettings && (
          <QuickSettings 
            onClose={() => setShowQuickSettings(false)} 
            onOpenAdvanced={(section) => {
              setShowAdvancedSettings((section as SectionId) || 'overview');
              setShowQuickSettings(false);
            }}
          />
        )}
        {showAdvancedSettings && (
          <AdvancedSettings 
            initialSection={showAdvancedSettings}
            onClose={() => setShowAdvancedSettings(null)} 
          />
        )}
      </AnimatePresence>
    </div>
  );
}

function EmptyState({ onOpenSettings, onOpenLibrary }: { onOpenSettings: () => void, onOpenLibrary: () => void }) {
  return (
    <div className="emptyState">
      <motion.div
        className="heroCopy"
        initial={{ opacity: 0, y: 14 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
      >
        <span className="heroEyebrow">Hybrid Desktop Search</span>
        <h1 className="heroTitle">Search your files locally.</h1>
        <div className="heroActions">
          <button className="heroPrimary" onClick={onOpenSettings}>
            Configure providers
          </button>
          <button className="heroSecondary" onClick={onOpenLibrary}>
            Open library
          </button>
        </div>
      </motion.div>

      <div className="emptyHint">
        <kbd className="kbd">⌘</kbd>
        <kbd className="kbd">Shift</kbd>
        <kbd className="kbd">Space</kbd>
        <span>to toggle from anywhere</span>
      </div>
    </div>
  );
}

function NoResults() {
  return (
    <div className="emptyState compact">
      <div className="heroCopy compactCopy">
        <span className="heroEyebrow">No matching results</span>
        <h2 className="heroTitle compactTitle">Try different wording or add more files.</h2>
      </div>
    </div>
  );
}

function AuroraBackdrop() {
  return (
    <div className="auroraBackdrop" aria-hidden="true">
      <div className="aurora orbA" />
      <div className="aurora orbB" />
      <div className="aurora orbC" />
      <div className="gridVeil" />
    </div>
  );
}


