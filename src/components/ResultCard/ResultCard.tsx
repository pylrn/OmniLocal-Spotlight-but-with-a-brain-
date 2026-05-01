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
  const fileLabel = getFileTypeIcon(result.file_type);

  return (
    <motion.div
      className={styles.resultCard}
      style={{ '--filetype-color': filetypeColor } as React.CSSProperties}
      initial={{ opacity: 0, y: 14, scale: 0.98 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.28, delay: index * 0.04 }}
      onClick={() => onClick(result)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => e.key === 'Enter' && onClick(result)}
    >
      <div className={styles.resultHeader}>
        <div className={styles.resultTitle}>
          <span className={styles.fileIcon}>{fileLabel.slice(0, 2).toUpperCase()}</span>
          <span className={styles.fileName}>
            {result.title || result.path.split('/').pop() || result.path}
          </span>
        </div>

        <div className={styles.resultMeta}>
          {result.context_boosted && (
            <span className={styles.contextBoostChip}>Context boost</span>
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

      <div className={styles.collectionPath}>
        {result.collection_name} · {result.path}
      </div>

      <div className={styles.snippet}>
        <ReactMarkdown>{result.snippet}</ReactMarkdown>
      </div>

      <div className={styles.footerRow}>
        <span className={styles.footerMeta}>
          Lines {result.start_line ?? '—'}–{result.end_line ?? '—'}
        </span>
        <div className={styles.scoreTrack}>
          <div
            className={styles.scoreBar}
            style={{
              width: `${scorePercent}%`,
              background: getScoreColor(result.score),
            }}
          />
        </div>
        <button
          className={styles.openBtn}
          title={`Open ${result.abs_path}`}
          aria-label="Open file"
          onClick={(e) => {
            e.stopPropagation();
            onClick(result);
          }}
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
            <path d="M2 10L10 2M10 2H5M10 2V7" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
          Open
        </button>
      </div>
    </motion.div>
  );
}
