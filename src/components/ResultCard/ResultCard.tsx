import { motion } from 'framer-motion';
import ReactMarkdown from 'react-markdown';
import type { SearchResult } from '../../lib/tauri';
import {
  getFileTypeColor,
  getFileTypeIcon,
  formatScore,
  getScoreColor,
} from '../../lib/tauri';
import styles from './ResultCard.module.css';

interface ResultCardProps {
  result: SearchResult;
  index: number;
  onClick: (result: SearchResult) => void;
}

export default function ResultCard({ result, index, onClick }: ResultCardProps) {
  const scorePercent = Math.round(result.score * 100);
  const filetypeColor = getFileTypeColor(result.file_type);

  return (
    <motion.div
      className={styles.resultCard}
      style={{ '--filetype-color': filetypeColor } as React.CSSProperties}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.2, delay: index * 0.04 }}
      onClick={() => onClick(result)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => e.key === 'Enter' && onClick(result)}
    >
      {/* Header row */}
      <div className={styles.resultHeader}>
        <div className={styles.resultTitle}>
          <span className={styles.fileIcon}>
            {getFileTypeIcon(result.file_type)}
          </span>
          <span className={styles.fileName}>
            {result.title || result.path.split('/').pop() || result.path}
          </span>
        </div>

        <div className={styles.resultMeta}>
          {result.context_boosted && (
            <span className={styles.contextBoostChip}>
              ✨ Boost
            </span>
          )}
          <span
            className={styles.scoreChip}
            style={{ background: getScoreColor(result.score) }}
          >
            {formatScore(result.score)}
          </span>
          <span className={styles.fileTypeChip}>.{result.file_type}</span>
        </div>
      </div>

      {/* Collection path */}
      <div className={styles.collectionPath}>
        {result.collection_name}/{result.path}
      </div>

      {/* Snippet */}
      <div className={styles.snippet}>
        <ReactMarkdown>{result.snippet}</ReactMarkdown>
      </div>

      {/* Score bar */}
      <div
        className={styles.scoreBar}
        style={{
          width: `${scorePercent}%`,
          background: getScoreColor(result.score),
        }}
      />
    </motion.div>
  );
}
