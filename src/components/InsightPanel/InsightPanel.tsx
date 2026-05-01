import { motion, AnimatePresence } from 'framer-motion';
import ReactMarkdown from 'react-markdown';
import styles from './InsightPanel.module.css';

interface InsightPanelProps {
  query: string;
  isLoading: boolean;
  insight: string | null;
  error: string | null;
  onDismiss: () => void;
  onOpenSettings: () => void;
}

export default function InsightPanel({
  query,
  isLoading,
  insight,
  error,
  onDismiss,
  onOpenSettings,
}: InsightPanelProps) {
  const isUnconfigured = error?.includes('not configured');

  return (
    <motion.div
      className={styles.panel}
      initial={{ opacity: 0, y: -10, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, y: -8, scale: 0.97 }}
      transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
    >
      <div className={styles.panelGlow} aria-hidden="true" />

      <div className={styles.header}>
        <div className={styles.label}>
          <SparkleIcon />
          <span className={styles.eyebrow}>AI Insight</span>
          {!isLoading && insight && (
            <span className={styles.queryPill}>"{query}"</span>
          )}
        </div>
        <button className={styles.dismissBtn} onClick={onDismiss} aria-label="Dismiss insight">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M2 2l10 10M12 2L2 12" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
          </svg>
        </button>
      </div>

      <div className={styles.body}>
        <AnimatePresence mode="wait">
          {isLoading && (
            <motion.div
              key="loading"
              className={styles.skeleton}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2 }}
            >
              <div className={styles.skeletonLine} style={{ width: '88%' }} />
              <div className={styles.skeletonLine} style={{ width: '72%' }} />
              <div className={styles.skeletonLine} style={{ width: '60%' }} />
            </motion.div>
          )}

          {!isLoading && error && (
            <motion.div
              key="error"
              className={styles.errorState}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              {isUnconfigured ? (
                <>
                  <span>Configure a query model to get AI insights on your results.</span>
                  <button className={styles.configureBtn} onClick={onOpenSettings}>
                    Open Intelligence Settings →
                  </button>
                </>
              ) : (
                <span>{error}</span>
              )}
            </motion.div>
          )}

          {!isLoading && insight && (
            <motion.div
              key="insight"
              className={styles.insightText}
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.25 }}
            >
              <ReactMarkdown>{insight}</ReactMarkdown>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div className={styles.footer}>
        <span className={styles.poweredBy}>Synthesized from top results via Gemini</span>
      </div>
    </motion.div>
  );
}

function SparkleIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true" className={styles.sparkleIcon}>
      <path d="M8 1v3M8 12v3M1 8h3M12 8h3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
      <path d="M3.76 3.76l2.12 2.12M10.12 10.12l2.12 2.12M10.12 5.88l2.12-2.12M3.76 12.24l2.12-2.12" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
      <circle cx="8" cy="8" r="2" fill="currentColor"/>
    </svg>
  );
}
