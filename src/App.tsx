import { useState, useCallback } from 'react';
import { AnimatePresence } from 'framer-motion';
import SearchBar from './components/SearchBar/SearchBar';
import ResultCard from './components/ResultCard/ResultCard';
import StatusBar from './components/StatusBar/StatusBar';
import Settings from './components/Settings/Settings';
import { searchKeyword } from './lib/tauri';
import type { SearchResult } from './lib/tauri';
import './App.css';

export default function App() {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [isIndexing, setIsIndexing] = useState(false);

  const handleAddFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const { addManualFiles } = await import('./lib/tauri');
      
      const paths = await open({
        multiple: true,
        title: "Select Files to Index"
      });
      
      if (paths && Array.isArray(paths) && paths.length > 0) {
        setIsIndexing(true);
        await addManualFiles(paths as string[]);
        setIsIndexing(false);
      }
    } catch (e) {
      console.error(e);
      setIsIndexing(false);
    }
  }, []);

  const handleSearch = useCallback(async (query: string) => {
    if (!query) {
      setResults([]);
      setHasSearched(false);
      return;
    }

    setIsSearching(true);
    setHasSearched(true);

    try {
      const res = await searchKeyword(query, 15);
      setResults(res);
    } catch (err) {
      console.error('Search failed:', err);
      setResults([]);
    } finally {
      setIsSearching(false);
    }
  }, []);

  const handleResultClick = useCallback((result: SearchResult) => {
    // Open file in default editor via the opener plugin
    // For now, log the path
    console.log('Open file:', result.abs_path, 'line:', result.start_line);
  }, []);

  return (
    <div className="app">
      <SearchBar
        onSearch={handleSearch}
        onAddFilesClick={handleAddFiles}
        isSearching={isSearching || isIndexing}
        resultCount={results.length}
      />

      <div className="mainContent">
        {!hasSearched ? (
          <EmptyState />
        ) : results.length === 0 && !isSearching ? (
          <NoResults />
        ) : (
          <div className="resultsList">
            <AnimatePresence mode="popLayout">
              {results.map((result, i) => (
                <ResultCard
                  key={`${result.document_id}-${result.chunk_id}`}
                  result={result}
                  index={i}
                  onClick={handleResultClick}
                />
              ))}
            </AnimatePresence>
          </div>
        )}
      </div>

      <StatusBar onSettingsClick={() => setShowSettings(true)} />

      <AnimatePresence>
        {showSettings && (
          <Settings onClose={() => setShowSettings(false)} />
        )}
      </AnimatePresence>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="emptyState">
      <div className="emptyIcon">🔮</div>
      <div className="emptyTitle">SmartSearch</div>
      <div className="emptySubtitle">
        Search your documents, notes, and code with AI-powered semantic understanding.
        Start typing to search across all your collections.
      </div>
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
    <div className="emptyState">
      <div className="emptyIcon">🔍</div>
      <div className="emptyTitle">No results found</div>
      <div className="emptySubtitle">
        Try different keywords or add a collection in settings.
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════
// UI Subcomponents
// ═══════════════════════════════════════════════════════════════
