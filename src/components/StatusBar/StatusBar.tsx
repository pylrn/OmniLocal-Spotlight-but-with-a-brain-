import { useEffect, useState } from 'react';
import type { IndexStats, ProviderStatus } from '../../lib/tauri';
import { getIndexStats, checkAiStatus } from '../../lib/tauri';
import styles from './StatusBar.module.css';

interface StatusBarProps {
  onSettingsClick?: () => void;
}

export default function StatusBar({ onSettingsClick }: StatusBarProps) {
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [aiStatus, setAiStatus] = useState<ProviderStatus | null>(null);

  useEffect(() => {
    // Initial fetch
    refreshStats();
    refreshAiStatus();

    // Periodic refresh
    const interval = setInterval(() => {
      refreshStats();
    }, 10000);

    return () => clearInterval(interval);
  }, []);

  async function refreshStats() {
    try {
      const s = await getIndexStats();
      setStats(s);
    } catch {
      // Silently handle — app may still be initializing
    }
  }

  async function refreshAiStatus() {
    try {
      const s = await checkAiStatus();
      setAiStatus(s);
    } catch {
      setAiStatus(null);
    }
  }

  const getAiStatusDot = () => {
    if (!aiStatus) return styles.statusDotGray;
    if (aiStatus.connected && aiStatus.model_available) return styles.statusDotGreen;
    if (aiStatus.connected) return styles.statusDotYellow;
    return styles.statusDotRed;
  };

  const getAiStatusText = () => {
    if (!aiStatus) return 'AI: checking...';
    if (aiStatus.connected && aiStatus.model_available) {
      return `${aiStatus.provider} · ${aiStatus.model_name}`;
    }
    if (aiStatus.connected) {
      return `${aiStatus.provider} · model missing`;
    }
    return `${aiStatus.provider} · offline`;
  };

  return (
    <div className={styles.statusBar}>
      <div className={styles.statusGroup}>
        {onSettingsClick && (
          <button 
            onClick={onSettingsClick} 
            title="Settings Dashboard"
            style={{ background: 'transparent', border: 'none', cursor: 'pointer', opacity: 0.8 }}
          >
            ⚙️
          </button>
        )}
        <div className={styles.statusItem}>
          <span>Indexed: {stats?.total_documents ?? '—'} files</span>
        </div>
        <div className={styles.statusItem}>
          <span>{stats?.total_chunks ?? '—'} chunks</span>
        </div>
        {stats && stats.embedded_chunks < stats.total_chunks && (
          <div className={styles.statusItem}>
            <span
              className={`${styles.statusDot} ${styles.statusDotYellow} ${styles.statusPending}`}
            />
            <span>
              {stats.total_chunks - stats.embedded_chunks} pending embeddings
            </span>
          </div>
        )}
      </div>

      <div className={styles.statusGroup}>
        <div className={styles.statusItem}>
          <span className={`${styles.statusDot} ${getAiStatusDot()}`} />
          <span>{getAiStatusText()}</span>
        </div>
        <div className={styles.statusItem}>
          <span>{stats?.total_collections ?? 0} collections</span>
        </div>
      </div>
    </div>
  );
}
