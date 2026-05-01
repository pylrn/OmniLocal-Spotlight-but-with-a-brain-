import { useEffect, useState } from 'react';
import type {
  EmbeddingProgressEvent,
  EmbeddingRuntimeStatus,
  IndexOverview,
} from '../../lib/tauri';
import {
  getEmbeddingRuntime,
  getIndexOverview,
  listenToEmbeddingProgress,
  listenToIndexProgress,
} from '../../lib/tauri';
import styles from './StatusBar.module.css';

interface StatusBarProps {
  onSettingsClick?: (section?: any) => void;
}

export default function StatusBar({ onSettingsClick }: StatusBarProps) {
  const [overview, setOverview] = useState<IndexOverview | null>(null);
  const [runtime, setRuntime] = useState<EmbeddingRuntimeStatus | null>(null);

  useEffect(() => {
    let embeddingUnlisten: (() => void) | undefined;
    let indexUnlisten: (() => void) | undefined;

    void refresh();
    void getEmbeddingRuntime().then(setRuntime).catch(() => undefined);

    void listenToEmbeddingProgress((payload: EmbeddingProgressEvent) => {
      setRuntime(payload.runtime);
      setOverview((current) =>
        current
          ? {
              ...current,
              total_chunks: payload.stats.total_chunks,
              embedded_chunks: payload.stats.embedded_chunks,
              pending_chunks: payload.stats.pending_chunks,
              failed_chunks: payload.stats.failed_chunks,
              total_documents: payload.stats.total_documents,
              total_collections: payload.stats.total_collections,
            }
          : current,
      );
    }).then((unlisten) => {
      embeddingUnlisten = unlisten;
    });

    void listenToIndexProgress(() => {
      void refresh();
    }).then((unlisten) => {
      indexUnlisten = unlisten;
    });

    const interval = setInterval(() => {
      void refresh();
    }, 12000);

    return () => {
      clearInterval(interval);
      embeddingUnlisten?.();
      indexUnlisten?.();
    };
  }, []);

  async function refresh() {
    try {
      const nextOverview = await getIndexOverview();
      setOverview(nextOverview);
    } catch {
      // App might still be starting up.
    }
  }

  const provider = overview?.provider_status;
  const runtimeText = runtime?.message ?? 'Waiting for indexing work';

  return (
    <div className={styles.statusBar}>
      <div className={styles.statusCluster}>
        {onSettingsClick && (
          <button className={styles.settingsButton} onClick={() => onSettingsClick()} title="Open settings">
            <span className={styles.settingsIcon}>⚙</span>
            <span>Control center</span>
          </button>
        )}

        <button className={styles.metricCard} onClick={() => onSettingsClick?.('sources')}>
          <span className={styles.metricLabel}>Indexed</span>
          <strong>{overview?.total_documents ?? '—'} files</strong>
        </button>
        <button className={styles.metricCard} onClick={() => onSettingsClick?.('library')}>
          <span className={styles.metricLabel}>Chunks</span>
          <strong>{overview?.total_chunks ?? '—'}</strong>
        </button>
        <button className={styles.metricCard} onClick={() => onSettingsClick?.('library')}>
          <span className={styles.metricLabel}>Embedded</span>
          <strong>{overview?.embedded_chunks ?? '—'}</strong>
        </button>
      </div>

      <div className={styles.statusCluster}>
        <div className={styles.statusPill}>
          <span className={`${styles.statusDot} ${getProviderDot(provider)}`} />
          <span>{provider ? `${provider.provider} · ${provider.model_name}` : 'Provider unavailable'}</span>
        </div>

        <div className={styles.statusPill}>
          <span className={`${styles.statusDot} ${getRuntimeDot(runtime?.phase)}`} />
          <span>{runtimeText}</span>
        </div>

        {overview && (
          <button className={styles.pendingBlock} onClick={() => onSettingsClick?.('library')}>
            <span>{overview.pending_chunks} pending</span>
            <span>{overview.failed_chunks} failed</span>
          </button>
        )}
      </div>
    </div>
  );
}

function getProviderDot(provider: IndexOverview['provider_status'] | undefined) {
  if (!provider) return styles.statusDotGray;
  if (provider.connected && provider.model_available) return styles.statusDotGreen;
  if (provider.connected) return styles.statusDotYellow;
  return styles.statusDotRed;
}

function getRuntimeDot(phase: string | undefined) {
  switch (phase) {
    case 'embedding':
      return styles.statusDotPulse;
    case 'paused':
      return styles.statusDotYellow;
    case 'error':
      return styles.statusDotRed;
    default:
      return styles.statusDotGreen;
  }
}
