import { useEffect, useMemo, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { open } from '@tauri-apps/plugin-dialog';
import { openPath } from '@tauri-apps/plugin-opener';
import type {
  Collection,
  DocumentRow,
  DocumentStatusRow,
  EmbeddingProgressEvent,
  EmbeddingRuntimeStatus,
  IndexOverview,
  ProviderStatus,
  ScanResult,
} from '../../lib/tauri';
import {
  addCollection,
  checkAiStatus,
  formatRelativeDate,
  getEmbeddingRuntime,
  getIndexOverview,
  getSetting,
  listCollections,
  listDocumentStatuses,
  listIndexedFiles,
  listenToEmbeddingProgress,
  listenToIndexProgress,
  reembedAll,
  removeCollection,
  removeManualFile,
  retryDocumentEmbeddings,
  retryFailedEmbeddings,
  scanCollections,
  setSetting,
} from '../../lib/tauri';
import styles from './Settings.module.css';

export type SectionId = 'overview' | 'indexing' | 'library' | 'sources' | 'intelligence';

interface AdvancedSettingsProps {
  onClose: () => void;
  initialSection?: SectionId;
}

const SECTION_LABELS: { id: SectionId; label: string; blurb: string }[] = [
  { id: 'overview', label: 'Overview', blurb: 'System health and storage' },
  { id: 'indexing', label: 'Indexing', blurb: 'Background jobs and control' },
  { id: 'library', label: 'Library', blurb: 'Document-level visibility' },
  { id: 'sources', label: 'Sources', blurb: 'Collections and standalone files' },
  { id: 'intelligence', label: 'Intelligence', blurb: 'AI querying and insights' },
];

export default function AdvancedSettings({ onClose, initialSection = 'overview' }: AdvancedSettingsProps) {
  const [activeSection, setActiveSection] = useState<SectionId>(initialSection);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [manualFiles, setManualFiles] = useState<DocumentRow[]>([]);
  const [documentStatuses, setDocumentStatuses] = useState<DocumentStatusRow[]>([]);
  const [overview, setOverview] = useState<IndexOverview | null>(null);
  const [runtime, setRuntime] = useState<EmbeddingRuntimeStatus | null>(null);
  const [providerStatus, setProviderStatus] = useState<ProviderStatus | null>(null);
  const [scanResults, setScanResults] = useState<ScanResult[] | null>(null);
  const [isScanning, setIsScanning] = useState(false);

  const [newName, setNewName] = useState('');
  const [newPath, setNewPath] = useState('');
  const [newGlob, setNewGlob] = useState('**/*.md');

  const [watchEnabled, setWatchEnabled] = useState(true);
  const [autoEmbedEnabled, setAutoEmbedEnabled] = useState(true);
  const [embedBatchSize, setEmbedBatchSize] = useState('12');
  const [queryApiKey, setQueryApiKey] = useState('');
  const [queryModel, setQueryModel] = useState('gemini-2.0-flash-lite');

  useEffect(() => {
    let unlistenEmbedding: (() => void) | undefined;
    let unlistenIndex: (() => void) | undefined;

    void refreshAll();

    void listenToEmbeddingProgress((payload: EmbeddingProgressEvent) => {
      setRuntime(payload.runtime);
      setOverview((current) =>
        current
          ? {
              ...current,
              total_collections: payload.stats.total_collections,
              total_documents: payload.stats.total_documents,
              total_chunks: payload.stats.total_chunks,
              embedded_chunks: payload.stats.embedded_chunks,
              pending_chunks: payload.stats.pending_chunks,
              failed_chunks: payload.stats.failed_chunks,
            }
          : current,
      );
      void refreshDocuments();
    }).then((unlisten) => {
      unlistenEmbedding = unlisten;
    });

    void listenToIndexProgress(() => {
      void refreshAll();
    }).then((unlisten) => {
      unlistenIndex = unlisten;
    });

    return () => {
      unlistenEmbedding?.();
      unlistenIndex?.();
    };
  }, []);

  async function refreshAll() {
    await Promise.all([
      refreshCollections(),
      refreshSettings(),
      refreshOverview(),
      refreshDocuments(),
      refreshProviderStatus(),
    ]);
  }

  async function refreshCollections() {
    const cols = await listCollections();
    setCollections(cols.filter((collection) => collection.name !== 'Standalone Files'));

    const files = await listIndexedFiles();
    setManualFiles(files.filter((file) => file.collection_name === 'Standalone Files'));
  }

  async function refreshSettings() {
    const [
      watchValue,
      autoEmbedValue,
      batchSize,
      queryKey,
      qModel,
    ] = await Promise.all([
      getSetting('watch_enabled'),
      getSetting('auto_embed_enabled'),
      getSetting('embed_batch_size'),
      getSetting('query_api_key'),
      getSetting('query_model'),
    ]);

    setWatchEnabled((watchValue || 'true') === 'true');
    setAutoEmbedEnabled((autoEmbedValue || 'true') === 'true');
    setEmbedBatchSize(batchSize || '12');
    setQueryApiKey(queryKey || '');
    setQueryModel(qModel || 'gemini-2.0-flash-lite');
  }

  async function refreshOverview() {
    const [nextOverview, nextRuntime] = await Promise.all([
      getIndexOverview(),
      getEmbeddingRuntime(),
    ]);
    setOverview(nextOverview);
    setRuntime(nextRuntime);
  }

  async function refreshDocuments() {
    const documents = await listDocumentStatuses();
    setDocumentStatuses(documents);
  }

  async function refreshProviderStatus() {
    try {
      setProviderStatus(await checkAiStatus());
    } catch {
      setProviderStatus(null);
    }
  }

  async function updateIndexingSetting(key: string, value: string) {
    await setSetting(key, value);
    await refreshSettings();
  }

  async function handleScan() {
    setIsScanning(true);
    setScanResults(null);
    try {
      const results = await scanCollections();
      setScanResults(results);
      await refreshAll();
    } finally {
      setIsScanning(false);
    }
  }

  async function handleRetryFailed() {
    await retryFailedEmbeddings();
    await refreshOverview();
  }

  async function handleReembedAll() {
    await reembedAll();
    await refreshOverview();
  }

  async function handleAddCollection() {
    if (!newName.trim() || !newPath.trim()) return;
    await addCollection(newName.trim(), newPath.trim(), newGlob);
    setNewName('');
    setNewPath('');
    setNewGlob('**/*.md');
    await refreshCollections();
  }

  async function handleRemoveCollection(name: string) {
    if (!confirm(`Remove collection "${name}"? This will only remove it from SmartSearch.`)) return;
    await removeCollection(name);
    await refreshCollections();
  }

  async function handleRemoveFile(id: number, title: string) {
    if (!confirm(`Remove "${title}" from the SmartSearch index?`)) return;
    await removeManualFile(id);
    await Promise.all([refreshCollections(), refreshDocuments(), refreshOverview()]);
  }

  async function handleRetryDocument(id: number) {
    await retryDocumentEmbeddings(id);
    await Promise.all([refreshDocuments(), refreshOverview()]);
  }

  async function handleOpenPath(path: string) {
    try {
      await openPath(path);
    } catch (error) {
      console.error('Failed to open path:', error);
    }
  }

  async function handleBrowse() {
    const selectedPath = await open({ directory: true, multiple: false });
    if (selectedPath) {
      const normalized = selectedPath as string;
      setNewPath(normalized);
      if (!newName.trim()) {
        const parts = normalized.split(/[\\/]/);
        setNewName(parts[parts.length - 1]);
      }
    }
  }

  const pendingDocuments = useMemo(
    () => documentStatuses.filter((row) => row.pending_chunk_count > 0 || row.failed_chunk_count > 0),
    [documentStatuses],
  );

  return (
    <motion.div
      className={styles.overlay}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onClick={onClose}
    >
      <motion.div
        className={styles.panel}
        initial={{ y: 24, opacity: 0, scale: 0.98 }}
        animate={{ y: 0, opacity: 1, scale: 1 }}
        exit={{ y: 24, opacity: 0, scale: 0.98 }}
        transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
        onClick={(event) => event.stopPropagation()}
      >
        <div className={styles.sidebar}>
          <div className={styles.sidebarHeader}>
            <span className={styles.sidebarEyebrow}>SmartSearch</span>
            <h2 className={styles.sidebarTitle}>Advanced Controls</h2>
            <p className={styles.sidebarCopy}>
              Deep configuration for indexing jobs, library management, and sources.
            </p>
          </div>

          <nav className={styles.nav}>
            {SECTION_LABELS.map((section) => (
              <button
                key={section.id}
                className={`${styles.navItem} ${activeSection === section.id ? styles.navItemActive : ''}`}
                onClick={() => setActiveSection(section.id)}
              >
                <span>{section.label}</span>
                <small>{section.blurb}</small>
              </button>
            ))}
          </nav>

          <button className={styles.closeButton} onClick={onClose}>
            Back to Search
          </button>
        </div>

        <div className={styles.content}>
          <header className={styles.contentHeader}>
            <div>
              <span className={styles.contentEyebrow}>
                {SECTION_LABELS.find((section) => section.id === activeSection)?.blurb}
              </span>
              <h3 className={styles.contentTitle}>
                {SECTION_LABELS.find((section) => section.id === activeSection)?.label}
              </h3>
            </div>
          </header>

          <div className={styles.contentScroll}>
            {activeSection === 'overview' && overview && (
              <OverviewSection
                overview={overview}
                runtime={runtime}
                providerStatus={providerStatus}
                onOpenPath={handleOpenPath}
              />
            )}

            {activeSection === 'indexing' && (
              <section className={styles.sectionStack}>
                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>Background job controls</h4>
                      <p>Control watch mode, embedding automation, and recovery actions for failed chunks.</p>
                    </div>
                  </div>

                  <div className={styles.toggleGrid}>
                    <label className={styles.toggleCard}>
                      <div>
                        <strong>Watch for file changes</strong>
                        <span>Keep collections synced in the background.</span>
                      </div>
                      <input 
                        type="checkbox" 
                        checked={watchEnabled} 
                        onChange={() => updateIndexingSetting('watch_enabled', String(!watchEnabled))} 
                      />
                    </label>

                    <label className={styles.toggleCard}>
                      <div>
                        <strong>Auto-embed pending chunks</strong>
                        <span>Run embeddings automatically after scans and watcher updates.</span>
                      </div>
                      <input
                        type="checkbox"
                        checked={autoEmbedEnabled}
                        onChange={() => updateIndexingSetting('auto_embed_enabled', String(!autoEmbedEnabled))}
                      />
                    </label>
                  </div>

                  <div className={styles.fieldGrid}>
                    <label className={styles.field}>
                      <span>Embedding batch size</span>
                      <input 
                        value={embedBatchSize} 
                        onBlur={(e) => updateIndexingSetting('embed_batch_size', e.target.value)}
                        onChange={(e) => setEmbedBatchSize(e.target.value)} 
                      />
                    </label>
                  </div>

                  <div className={styles.actionRow}>
                    <button className={styles.primaryButton} onClick={handleScan} disabled={isScanning}>
                      {isScanning ? 'Scanning collections...' : 'Scan collections now'}
                    </button>
                    <button className={styles.secondaryButton} onClick={handleRetryFailed}>
                      Retry failed
                    </button>
                    <button className={styles.secondaryButton} onClick={handleReembedAll}>
                      Re-embed everything
                    </button>
                  </div>

                  {scanResults && (
                    <div className={styles.logBox}>
                      {scanResults.map((result) => (
                        <div key={result.collection_name} className={styles.logRow}>
                          <strong>{result.collection_name}</strong>
                          <span>
                            {result.files_indexed} indexed · {result.files_unchanged} unchanged · {result.errors.length} errors
                          </span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </section>
            )}

            {activeSection === 'library' && (
              <section className={styles.sectionStack}>
                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>Embedding backlog</h4>
                      <p>See which files are still embedding, which failed, and which have already landed in vector storage.</p>
                    </div>
                  </div>

                  <div className={styles.table}>
                    {pendingDocuments.length === 0 ? (
                      <div className={styles.emptyTable}>No pending or failed documents right now.</div>
                    ) : (
                      pendingDocuments.map((row) => (
                        <div key={row.document_id} className={styles.tableRow}>
                          <div className={styles.tableMain}>
                            <button
                              type="button"
                              className={styles.titleLink}
                              onClick={() => handleOpenPath(row.abs_path)}
                            >
                              {row.title || row.path.split('/').pop()}
                            </button>
                            <button
                              type="button"
                              className={styles.pathLink}
                              onClick={() => handleOpenPath(row.abs_path)}
                            >
                              {row.collection_name} · {row.path}
                            </button>
                          </div>
                          <div className={styles.tableStats}>
                            <span>{row.embedded_chunk_count}/{row.chunk_count} embedded</span>
                            <span>{row.pending_chunk_count} pending</span>
                            <span>{row.failed_chunk_count} failed</span>
                          </div>
                          <div className={styles.rowActions}>
                            <button
                              type="button"
                              className={styles.secondaryButton}
                              disabled={row.failed_chunk_count === 0}
                              onClick={() => handleRetryDocument(row.document_id)}
                            >
                              Retry
                            </button>
                            <button
                              type="button"
                              className={styles.ghostDangerButton}
                              onClick={() =>
                                handleRemoveFile(row.document_id, row.title || row.path.split('/').pop() || 'file')
                              }
                            >
                              Remove
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </div>

                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>Document library</h4>
                      <p>Every indexed document with chunk counts, embedding status, and the latest indexing error.</p>
                    </div>
                  </div>

                  <div className={styles.table}>
                    {documentStatuses.map((row) => (
                      <div key={row.document_id} className={styles.tableRow}>
                        <div className={styles.tableMain}>
                          <button
                            type="button"
                            className={styles.titleLink}
                            onClick={() => handleOpenPath(row.abs_path)}
                          >
                            {row.title || row.path.split('/').pop()}
                          </button>
                          <button
                            type="button"
                            className={styles.pathLink}
                            onClick={() => handleOpenPath(row.abs_path)}
                          >
                            {row.abs_path}
                          </button>
                        </div>
                        <div className={styles.tableStats}>
                          <span>{row.chunk_count} chunks</span>
                          <span>{row.embedded_chunk_count} embedded</span>
                          <span>{formatRelativeDate(row.last_indexed)}</span>
                        </div>
                        <div className={styles.rowActions}>
                          <button
                            type="button"
                            className={styles.secondaryButton}
                            disabled={row.failed_chunk_count === 0}
                            onClick={() => handleRetryDocument(row.document_id)}
                          >
                            Retry
                          </button>
                          <button
                            type="button"
                            className={styles.ghostDangerButton}
                            onClick={() =>
                              handleRemoveFile(row.document_id, row.title || row.path.split('/').pop() || 'file')
                            }
                          >
                            Remove
                          </button>
                        </div>
                        {row.last_error && <div className={styles.errorText}>{row.last_error}</div>}
                      </div>
                    ))}
                  </div>
                </div>
              </section>
            )}

            {activeSection === 'sources' && (
              <section className={styles.sectionStack}>
                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>Connected folders</h4>
                      <p>Collections are scanned, chunked, and watched for future updates when watch mode is enabled.</p>
                    </div>
                  </div>

                  <div className={styles.table}>
                    <AnimatePresence>
                      {collections.map((collection) => (
                        <motion.div
                          key={collection.id}
                          className={styles.tableRow}
                          initial={{ opacity: 0, y: 8 }}
                          animate={{ opacity: 1, y: 0 }}
                          exit={{ opacity: 0, y: -8 }}
                        >
                          <div className={styles.tableMain}>
                            <strong>{collection.name}</strong>
                            <button
                              type="button"
                              className={styles.pathLink}
                              onClick={() => handleOpenPath(collection.path)}
                            >
                              {collection.path}
                            </button>
                          </div>
                          <div className={styles.tableStats}>
                            <span>{collection.doc_count} documents</span>
                          </div>
                          <button className={styles.ghostDangerButton} onClick={() => handleRemoveCollection(collection.name)}>
                            Remove
                          </button>
                        </motion.div>
                      ))}
                    </AnimatePresence>
                  </div>

                  <div className={styles.addPanel}>
                    <div className={styles.fieldGrid}>
                      <label className={styles.field}>
                        <span>Collection name</span>
                        <input value={newName} onChange={(event) => setNewName(event.target.value)} />
                      </label>
                      <label className={styles.field}>
                        <span>Glob pattern</span>
                        <input value={newGlob} onChange={(event) => setNewGlob(event.target.value)} />
                      </label>
                      <label className={`${styles.field} ${styles.fullWidth}`}>
                        <span>Folder path</span>
                        <div className={styles.pathRow}>
                          <input value={newPath} onChange={(event) => setNewPath(event.target.value)} />
                          <button className={styles.secondaryButton} onClick={handleBrowse}>
                            Browse
                          </button>
                        </div>
                      </label>
                    </div>

                    <div className={styles.actionRow}>
                      <button className={styles.primaryButton} onClick={handleAddCollection}>
                        Add folder
                      </button>
                    </div>
                  </div>
                </div>

                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>Standalone files</h4>
                      <p>Files added directly from the main search bar live here.</p>
                    </div>
                  </div>

                  <div className={styles.table}>
                    {manualFiles.length === 0 ? (
                      <div className={styles.emptyTable}>No standalone files indexed yet.</div>
                    ) : (
                      manualFiles.map((doc) => (
                        <div key={doc.id} className={styles.tableRow}>
                          <div className={styles.tableMain}>
                            <button
                              type="button"
                              className={styles.titleLink}
                              onClick={() => handleOpenPath(doc.abs_path)}
                            >
                              {doc.title || doc.path.split('/').pop()}
                            </button>
                            <button
                              type="button"
                              className={styles.pathLink}
                              onClick={() => handleOpenPath(doc.abs_path)}
                            >
                              {doc.abs_path}
                            </button>
                          </div>
                          <button
                            type="button"
                            className={styles.ghostDangerButton}
                            onClick={() => handleRemoveFile(doc.id, doc.title || 'file')}
                          >
                            Remove
                          </button>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              </section>
            )}

            {activeSection === 'intelligence' && (
              <section className={styles.sectionStack}>
                <div className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div>
                      <h4>AI Query Model</h4>
                      <p>
                        Configures the generative model used to synthesize search results into
                        a natural-language insight. Top-5 ranked result snippets are sent as context.
                      </p>
                    </div>
                  </div>

                  <div className={styles.fieldGrid}>
                    <label className={styles.field}>
                      <span>Model name</span>
                      <div className={styles.inputStack}>
                        <select
                          value={queryModel}
                          onChange={(e) => {
                            setQueryModel(e.target.value);
                            updateIndexingSetting('query_model', e.target.value);
                          }}
                        >
                          <option value="gemini-3.1-pro">Gemini 3.1 Pro (Flagship 2026)</option>
                          <option value="gemini-3-flash">Gemini 3 Flash (Fast & Modern)</option>
                          <option value="gemini-3.1-flash-lite">Gemini 3.1 Flash-Lite (Cheapest)</option>
                          <option value="gemini-2.5-pro">Gemini 2.5 Pro (Stable High Quality)</option>
                          <option value="gemini-2.5-flash">Gemini 2.5 Flash (Stable Balanced)</option>
                          <option value="custom">-- Custom model id --</option>
                        </select>
                        
                        {(queryModel === 'custom' || !['gemini-3.1-pro', 'gemini-3-flash', 'gemini-3.1-flash-lite', 'gemini-2.5-pro', 'gemini-2.5-flash'].includes(queryModel)) && (
                          <input
                            value={queryModel === 'custom' ? '' : queryModel}
                            onChange={(e) => setQueryModel(e.target.value)}
                            onBlur={(e) => updateIndexingSetting('query_model', e.target.value)}
                            placeholder="Enter custom model id..."
                            style={{ marginTop: '0.5rem' }}
                          />
                        )}
                      </div>
                    </label>

                    <label className={`${styles.field} ${styles.fullWidth}`}>
                      <span>Query API Key</span>
                      <input
                        type="password"
                        value={queryApiKey}
                        onChange={(e) => setQueryApiKey(e.target.value)}
                        onBlur={(e) => updateIndexingSetting('query_api_key', e.target.value)}
                        placeholder="AIza..."
                      />
                    </label>
                  </div>

                  <div className={styles.logBox}>
                    <div className={styles.logRow}>
                      <span>Provider</span>
                      <strong>Gemini (Google AI)</strong>
                    </div>
                    <div className={styles.logRow}>
                      <span>Context sent</span>
                      <strong>Top 5 result snippets + your query</strong>
                    </div>
                    <div className={styles.logRow}>
                      <span>Suggested models</span>
                      <strong>gemini-2.0-flash-lite · gemini-1.5-flash · gemini-2.0-flash</strong>
                    </div>
                  </div>
                </div>
              </section>
            )}
          </div>
        </div>
      </motion.div>
    </motion.div>
  );
}

function OverviewSection({
  overview,
  runtime,
  providerStatus,
  onOpenPath,
}: {
  overview: IndexOverview;
  runtime: EmbeddingRuntimeStatus | null;
  providerStatus: ProviderStatus | null;
  onOpenPath: (path: string) => Promise<void>;
}) {
  return (
    <section className={styles.sectionStack}>
      <div className={styles.metricGrid}>
        <MetricCard label="Collections" value={overview.total_collections} />
        <MetricCard label="Documents" value={overview.total_documents} />
        <MetricCard label="Chunks" value={overview.total_chunks} />
        <MetricCard label="Embedded" value={overview.embedded_chunks} />
        <MetricCard label="Pending" value={overview.pending_chunks} accent="warning" />
        <MetricCard label="Failed" value={overview.failed_chunks} accent="error" />
      </div>

      <div className={styles.card}>
        <div className={styles.cardHeader}>
          <div>
            <h4>Current runtime</h4>
            <p>Live embedding status from the background worker.</p>
          </div>
        </div>

        <div className={styles.statusCard}>
          <strong>{runtime?.phase || 'idle'}</strong>
          <span>{runtime?.message || 'Waiting for indexing work'}</span>
          {runtime?.current_path && (
            <span>
              Current file:{' '}
              <button
                type="button"
                className={styles.pathLink}
                onClick={() => onOpenPath(runtime.current_path!)}
              >
                {runtime.current_path}
              </button>
            </span>
          )}
        </div>
      </div>

      <div className={styles.card}>
        <div className={styles.cardHeader}>
          <div>
            <h4>Storage details</h4>
            <p>Where vectors live and which provider is currently active.</p>
          </div>
        </div>

        <div className={styles.detailList}>
          <div><span>SQLite database</span><strong>{overview.db_path}</strong></div>
          <div><span>Watcher</span><strong>{overview.watcher_enabled ? 'Enabled' : 'Disabled'}</strong></div>
          <div><span>Provider</span><strong>{overview.provider_status.provider}</strong></div>
          <div><span>Model</span><strong>{overview.provider_status.model_name}</strong></div>
          <div><span>Dimensions</span><strong>{overview.provider_status.dimensions ?? 'Unknown'}</strong></div>
          <div><span>Last indexing activity</span><strong>{formatRelativeDate(overview.last_indexed_at)}</strong></div>
          {providerStatus?.error && <div><span>Provider error</span><strong>{providerStatus.error}</strong></div>}
        </div>
      </div>
    </section>
  );
}

function MetricCard({
  label,
  value,
  accent,
}: {
  label: string;
  value: number;
  accent?: 'warning' | 'error';
}) {
  return (
    <div className={`${styles.metricCard} ${accent === 'warning' ? styles.metricWarning : ''} ${accent === 'error' ? styles.metricError : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
