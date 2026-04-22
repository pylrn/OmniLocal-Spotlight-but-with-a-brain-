import { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { getSetting, setSetting, listCollections, addCollection, removeCollection, scanCollections, listIndexedFiles, removeManualFile } from '../../lib/tauri';
import { open } from '@tauri-apps/plugin-dialog';
import type { Collection, ScanResult, DocumentRow } from '../../lib/tauri';
import styles from './Settings.module.css';

interface SettingsProps {
  onClose: () => void;
}

export default function Settings({ onClose }: SettingsProps) {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [manualFiles, setManualFiles] = useState<DocumentRow[]>([]);
  const [newName, setNewName] = useState('');
  const [newPath, setNewPath] = useState('');
  const [newGlob, setNewGlob] = useState('**/*.md');
  const [scanResults, setScanResults] = useState<ScanResult[] | null>(null);
  const [isScanning, setIsScanning] = useState(false);

  useEffect(() => {
    loadCollections();
  }, []);

  async function loadCollections() {
    try {
      const cols = await listCollections();
      // Filter out Standalone Files from the folders view since it has its own section
      setCollections(cols.filter(c => c.name !== "Standalone Files"));
      
      const files = await listIndexedFiles();
      // Only track files uploaded to the generic standalone bucket
      setManualFiles(files.filter(f => f.collection_name === "Standalone Files"));
    } catch (err) {
      console.error('Failed to load assets:', err);
    }
  }

  async function handleAddCollection() {
    if (!newName.trim() || !newPath.trim()) return;

    try {
      await addCollection(newName.trim(), newPath.trim(), newGlob);
      setNewName('');
      setNewPath('');
      setNewGlob('**/*.md');
      await loadCollections();
    } catch (err) {
      console.error('Failed to add collection:', err);
      alert(`Error: ${err}`);
    }
  }

  async function handleRemoveCollection(name: string) {
    if (!confirm(`Remove collection "${name}"? This won't delete your files.`)) return;
    try {
      await removeCollection(name);
      await loadCollections();
    } catch (err) {
      console.error('Failed to remove collection:', err);
    }
  }

  async function handleRemoveFile(id: number, title: string) {
    if (!confirm(`Remove file "${title}" from the index?`)) return;
    try {
      await removeManualFile(id);
      await loadCollections();
    } catch (err) {
      console.error('Failed to remove file:', err);
    }
  }

  async function handleBrowse() {
    try {
      const selectedPath = await open({
        directory: true,
        multiple: false,
      });
      if (selectedPath) {
        setNewPath(selectedPath as string);
        if (!newName.trim()) {
          const parts = (selectedPath as string).split(/[\\/]/);
          setNewName(parts[parts.length - 1]);
        }
      }
    } catch (err) {
      console.error('Failed to open dialog:', err);
    }
  }

  async function handleScan() {
    setIsScanning(true);
    setScanResults(null);
    try {
      const results = await scanCollections();
      setScanResults(results);
      await loadCollections();
    } catch (err) {
      console.error('Scan failed:', err);
    } finally {
      setIsScanning(false);
    }
  }

  return (
    <motion.div
      className={styles.overlay}
      initial={{ opacity: 0, backdropFilter: 'blur(0px)' }}
      animate={{ opacity: 1, backdropFilter: 'blur(12px)' }}
      exit={{ opacity: 0, backdropFilter: 'blur(0px)' }}
      transition={{ duration: 0.4 }}
      onClick={onClose}
    >
      <motion.div
        className={styles.panel}
        initial={{ y: '20px', scale: 0.9, opacity: 0 }}
        animate={{ y: 0, scale: 1, opacity: 1 }}
        exit={{ y: '20px', scale: 0.9, opacity: 0 }}
        transition={{ type: 'spring', damping: 25, stiffness: 300 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.header}>
          <h2 className={styles.title}>System Configuration</h2>
          <button className={styles.closeBtn} onClick={onClose}>✕</button>
        </div>

        <div className={styles.content}>
          <section className={styles.section}>
            <div className={styles.sectionHeader}>
              <h3>Indexed Data Sources</h3>
              <p>Manage local paths where SmartSearch looks for context.</p>
            </div>

            <div className={styles.collectionList}>
              <AnimatePresence>
                {collections.map((col) => (
                  <motion.div 
                    key={col.id} 
                    className={styles.collectionItem}
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    exit={{ opacity: 0, height: 0, scale: 0.95 }}
                  >
                    <div className={styles.colInfo}>
                      <span className={styles.colName}>{col.name}</span>
                      <span className={styles.colPath}>{col.path}</span>
                    </div>
                    <div className={styles.colActions}>
                      <span className={styles.countBadge}>{col.doc_count || 0} chunks</span>
                      <button className={styles.dangerBtn} onClick={() => handleRemoveCollection(col.name)}>Disconnect</button>
                    </div>
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>

            <div className={styles.sectionHeader} style={{ marginTop: '24px' }}>
              <h3>Standalone Documents</h3>
              <p>Individual files tracked and embedded by SmartSearch.</p>
            </div>

            <div className={styles.collectionList}>
              <AnimatePresence>
                {manualFiles.length === 0 ? (
                  <div style={{ padding: '16px', color: 'var(--text-tertiary)', fontSize: '13px' }}>
                    No standalone files indexed. Use the + button to add files.
                  </div>
                ) : (
                  manualFiles.map((doc) => (
                    <motion.div 
                      key={doc.id} 
                      className={styles.collectionItem}
                      initial={{ opacity: 0, height: 0 }}
                      animate={{ opacity: 1, height: 'auto' }}
                      exit={{ opacity: 0, height: 0, scale: 0.95 }}
                    >
                      <div className={styles.colInfo}>
                        <span className={styles.colName}>{doc.title || doc.path.split('/').pop()}</span>
                        <span className={styles.colPath}>{doc.abs_path}</span>
                      </div>
                      <div className={styles.colActions}>
                        <button className={styles.dangerBtn} onClick={() => handleRemoveFile(doc.id, doc.title || 'file')}>🗑️</button>
                      </div>
                    </motion.div>
                  ))
                )}
              </AnimatePresence>
            </div>

            <div className={styles.addForm}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-md)' }}>
                <h4>Mount New Source</h4>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button type="button" className={styles.secondaryBtn} style={{ padding: '4px 8px', fontSize: '10px' }} onClick={() => { setNewPath('~/Downloads'); setNewGlob('**/*.{pdf,docx,md}'); setNewName('downloads'); }}>+ Downloads Preset</button>
                  <button type="button" className={styles.secondaryBtn} style={{ padding: '4px 8px', fontSize: '10px' }} onClick={() => { setNewPath('~/Documents'); setNewGlob('**/*.{pdf,docx,txt,md}'); setNewName('documents'); }}>+ Documents Preset</button>
                </div>
              </div>
              <div className={styles.inputGroup}>
                <input 
                  type="text" 
                  placeholder="name (e.g. obsidian-vault)" 
                  value={newName} 
                  onChange={(e) => setNewName(e.target.value)} 
                />
                <div className={styles.pathInputWrapper}>
                  <input 
                    type="text" 
                    placeholder="/Users/example/notes" 
                    value={newPath} 
                    onChange={(e) => setNewPath(e.target.value)} 
                  />
                  <button type="button" className={styles.browseBtn} onClick={handleBrowse}>Browse</button>
                </div>
                <input 
                  type="text" 
                  placeholder="**/*.md" 
                  value={newGlob} 
                  onChange={(e) => setNewGlob(e.target.value)} 
                />
              </div>
              <div className={styles.actionRow}>
                <button 
                  className={styles.primaryBtn} 
                  onClick={handleAddCollection}
                  disabled={!newName || !newPath}
                >
                  Mount Folder
                </button>
                <button 
                  className={styles.secondaryBtn} 
                  onClick={handleScan}
                  disabled={isScanning || collections.length === 0}
                >
                  {isScanning ? 'Syncing...' : 'Force Sync Vectors'}
                </button>
              </div>
            </div>

            {scanResults && (
              <motion.div 
                className={styles.logBox}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
              >
                {scanResults.map((sr, i) => (
                  <div key={i} className={styles.logEntry}>
                    <span className={styles.logHighlight}>[{sr.collection_name}]</span> Synced {sr.files_indexed} files. {sr.files_unchanged} skipped.
                    {sr.errors.length > 0 && <span className={styles.logError}> Errors: {sr.errors.length}</span>}
                  </div>
                ))}
              </motion.div>
            )}
          </section>
        </div>
      </motion.div>
    </motion.div>
  );
}
