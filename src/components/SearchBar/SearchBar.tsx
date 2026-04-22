import { useState, useCallback, useRef, useEffect } from 'react';
import styles from './SearchBar.module.css';

interface SearchBarProps {
  onSearch: (query: string) => void;
  onAddFilesClick: () => void;
  isSearching: boolean;
  resultCount: number;
}

export default function SearchBar({
  onSearch,
  onAddFilesClick,
  isSearching,
  resultCount,
}: SearchBarProps) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Auto-focus on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = e.target.value;
      setQuery(value);

      // Debounce search
      if (debounceTimer.current) {
        clearTimeout(debounceTimer.current);
      }

      if (value.trim().length >= 2) {
        debounceTimer.current = setTimeout(() => {
          onSearch(value.trim());
        }, 300);
      } else if (value.trim().length === 0) {
        onSearch('');
      }
    },
    [onSearch],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && query.trim()) {
        if (debounceTimer.current) {
          clearTimeout(debounceTimer.current);
        }
        onSearch(query.trim());
      }
      if (e.key === 'Escape') {
        setQuery('');
        onSearch('');
      }
    },
    [query, onSearch],
  );

  return (
    <div className={styles.searchBarContainer}>
      <div className={styles.searchInputWrapper}>
        <span className={styles.searchIcon}>🔍</span>
        <input
          ref={inputRef}
          id="search-input"
          className={styles.searchInput}
          type="text"
          placeholder="Search your knowledge..."
          value={query}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          autoComplete="off"
          spellCheck={false}
        />
        <div className={styles.searchActions}>
          {isSearching && <span className={styles.searchSpinner} />}
          <button
            id="add-files-button"
            className={styles.actionButton}
            onClick={onAddFilesClick}
            title="Add File(s) to Index"
          >
            +
          </button>
        </div>
      </div>
      {query.trim().length > 0 && (
        <div className={styles.searchMeta}>
          <span className={styles.searchMetaText}>
            {isSearching
              ? 'Searching...'
              : `${resultCount} result${resultCount !== 1 ? 's' : ''}`}
          </span>
        </div>
      )}
    </div>
  );
}
