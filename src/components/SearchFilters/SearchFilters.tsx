import { motion } from 'framer-motion';
import styles from './SearchFilters.module.css';

interface SearchFiltersProps {
  fileTypes: string[];
  collections: string[];
  selectedFileType: string | null;
  selectedCollection: string | null;
  totalResults: number;
  filteredResults: number;
  onFileTypeChange: (type: string | null) => void;
  onCollectionChange: (collection: string | null) => void;
  onClearFilters: () => void;
}

export default function SearchFilters({
  fileTypes,
  collections,
  selectedFileType,
  selectedCollection,
  totalResults,
  filteredResults,
  onFileTypeChange,
  onCollectionChange,
  onClearFilters,
}: SearchFiltersProps) {
  const hasActiveFilters = selectedFileType !== null || selectedCollection !== null;

  return (
    <motion.div
      className={styles.filterContainer}
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
    >
      <div className={styles.filterGroup}>
        <div className={styles.filterControl}>
          <label className={styles.filterLabel}>File Type</label>
          <select
            className={styles.filterSelect}
            value={selectedFileType || ''}
            onChange={(e) => onFileTypeChange(e.target.value || null)}
          >
            <option value="">All file types</option>
            {fileTypes.map((type) => (
              <option key={type} value={type}>
                .{type}
              </option>
            ))}
          </select>
        </div>

        <div className={styles.filterControl}>
          <label className={styles.filterLabel}>Collection</label>
          <select
            className={styles.filterSelect}
            value={selectedCollection || ''}
            onChange={(e) => onCollectionChange(e.target.value || null)}
          >
            <option value="">All collections</option>
            {collections.map((collection) => (
              <option key={collection} value={collection}>
                {collection}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className={styles.filterStats}>
        <span className={styles.resultCount}>
          {filteredResults} of {totalResults} result{totalResults !== 1 ? 's' : ''}
        </span>
        {hasActiveFilters && (
          <button
            type="button"
            className={styles.clearButton}
            onClick={onClearFilters}
          >
            Clear filters
          </button>
        )}
      </div>
    </motion.div>
  );
}
