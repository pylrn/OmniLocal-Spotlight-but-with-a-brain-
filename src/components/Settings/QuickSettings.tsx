import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { checkAiStatus, getSetting, setSetting, testAiProvider } from '../../lib/tauri';
import type { ProviderStatus } from '../../lib/tauri';
import styles from './QuickSettings.module.css';

interface QuickSettingsProps {
  onClose: () => void;
  onOpenAdvanced: (section?: string) => void;
}

export default function QuickSettings({ onClose, onOpenAdvanced }: QuickSettingsProps) {
  const [aiProvider, setAiProvider] = useState('ollama');
  const [embedModel, setEmbedModel] = useState('nomic-embed-text');
  const [geminiApiKey, setGeminiApiKey] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [status, setStatus] = useState<ProviderStatus | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh() {
    const [provider, model, geminiKey] = await Promise.all([
      getSetting('ai_provider'),
      getSetting('embed_model'),
      getSetting('gemini_api_key'),
    ]);
    setAiProvider(provider || 'ollama');
    setEmbedModel(model || 'nomic-embed-text');
    setGeminiApiKey(geminiKey || '');
    
    try {
      setStatus(await checkAiStatus());
    } catch {
      setStatus(null);
    }
  }

  async function handleSave() {
    setIsSaving(true);
    try {
      await Promise.all([
        setSetting('ai_provider', aiProvider),
        setSetting('embed_model', embedModel),
        setSetting('gemini_api_key', geminiApiKey),
      ]);
      const nextStatus = await testAiProvider();
      setStatus(nextStatus);
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <motion.div 
      className={styles.overlay}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      onClick={onClose}
    >
      <motion.div 
        className={styles.popover}
        initial={{ scale: 0.9, opacity: 0, y: 10 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        exit={{ scale: 0.9, opacity: 0, y: 10 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.header}>
          <h3>Quick Settings</h3>
          <button className={styles.closeBtn} onClick={onClose}>×</button>
        </div>

        <div className={styles.form}>
          <div className={styles.field}>
            <label>AI Provider</label>
            <select value={aiProvider} onChange={(e) => setAiProvider(e.target.value)}>
              <option value="ollama">Ollama (Local)</option>
              <option value="lmstudio">LM Studio (Local)</option>
              <option value="gemini">Gemini (Cloud)</option>
            </select>
          </div>

          <div className={styles.field}>
            <label>Model</label>
            {aiProvider === 'gemini' ? (
              <select value={embedModel} onChange={(e) => setEmbedModel(e.target.value)}>
                <option value="gemini-embedding-2">gemini-embedding-2 (Latest 2026)</option>
                <option value="text-embedding-004">text-embedding-004 (Legacy)</option>
                <option value="gemini-embedding-001">gemini-embedding-001 (Legacy)</option>
              </select>
            ) : (
              <input 
                value={embedModel} 
                onChange={(e) => setEmbedModel(e.target.value)}
                placeholder={aiProvider === 'ollama' ? 'e.g. nomic-embed-text' : 'e.g. text-embedding'}
              />
            )}
          </div>

          {aiProvider === 'gemini' && (
            <div className={styles.field}>
              <label>Gemini API Key</label>
              <input 
                type="password"
                value={geminiApiKey} 
                onChange={(e) => setGeminiApiKey(e.target.value)}
                placeholder="AIza..."
              />
            </div>
          )}

          <div className={styles.status}>
            {status ? (
              <div className={status.connected ? styles.connected : styles.disconnected}>
                <span className={styles.dot} />
                {status.connected ? `${status.provider} Connected` : status.error || 'Connection Failed'}
              </div>
            ) : (
              <div className={styles.loading}>Checking status...</div>
            )}
          </div>

          <div className={styles.actions}>
            <button 
              className={styles.saveBtn} 
              onClick={handleSave}
              disabled={isSaving}
            >
              {isSaving ? 'Applying...' : 'Save & Apply'}
            </button>
            <button 
              className={styles.advancedBtn}
              onClick={() => onOpenAdvanced()}
            >
              Advanced Controls →
            </button>
          </div>
        </div>
      </motion.div>
    </motion.div>
  );
}
