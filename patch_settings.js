const fs = require('fs');
const file = 'src/components/Settings/Settings.tsx';
let txt = fs.readFileSync(file, 'utf8');

txt = txt.replace(
  "const [isScanning, setIsScanning] = useState(false);",
  `const [isScanning, setIsScanning] = useState(false);
  const [aiProvider, setAiProvider] = useState<'ollama' | 'gemini'>('ollama');
  const [geminiKey, setGeminiKey] = useState('');
  const [embedModel, setEmbedModel] = useState('');
  const [saveStatus, setSaveStatus] = useState('');`
);

txt = txt.replace(
  "loadCollections();\n  }, []);",
  `loadCollections();
    loadSettings();
  }, []);

  async function loadSettings() {
    try {
      const provider = await getSetting('ai_provider');
      if (provider) setAiProvider(provider as 'ollama' | 'gemini');
      
      const key = await getSetting('gemini_api_key');
      if (key) setGeminiKey(key);
      
      const model = await getSetting('embed_model');
      if (model) setEmbedModel(model);
    } catch(err) {
      console.error(err);
    }
  }

  async function handleSaveSettings() {
    setSaveStatus('Saving...');
    try {
      await setSetting('ai_provider', aiProvider);
      await setSetting('gemini_api_key', geminiKey);
      await setSetting('embed_model', embedModel || (aiProvider === 'gemini' ? 'models/text-embedding-004' : 'nomic-embed-text'));
      setSaveStatus('Saved! Restart app to apply.');
      setTimeout(() => setSaveStatus(''), 3000);
    } catch (e) {
      setSaveStatus(\`Error: \${e}\`);
    }
  }`
);

const renderForm = `
            <div className={styles.sectionHeader} style={{ marginTop: '24px' }}>
              <h3>AI configuration</h3>
              <p>Configure local Ollama or Gemini API for embeddings. (Gemini requires restart)</p>
            </div>
            <div className={styles.addForm}>
              <div className={styles.inputGroup} style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                <div style={{ display: 'flex', gap: '10px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px', color: 'var(--text-secondary)' }}>
                    <input type="radio" checked={aiProvider === 'ollama'} onChange={() => {setAiProvider('ollama'); setEmbedModel('nomic-embed-text');}} /> 
                    Ollama (Local)
                  </label>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px', color: 'var(--text-secondary)' }}>
                    <input type="radio" checked={aiProvider === 'gemini'} onChange={() => {setAiProvider('gemini'); setEmbedModel('models/text-embedding-004');}} /> 
                    Gemini API
                  </label>
                </div>
                
                {aiProvider === 'gemini' && (
                  <input 
                    type="password" 
                    placeholder="Enter Gemini API Key..." 
                    value={geminiKey} 
                    onChange={(e) => setGeminiKey(e.target.value)} 
                  />
                )}
                
                <input 
                  type="text" 
                  placeholder={aiProvider === 'gemini' ? 'models/text-embedding-004' : 'nomic-embed-text'} 
                  value={embedModel} 
                  onChange={(e) => setEmbedModel(e.target.value)} 
                  title="Embedding model to use"
                />
              </div>
              <div className={styles.actionRow} style={{ marginTop: '12px' }}>
                <button className={styles.primaryBtn} onClick={handleSaveSettings}>Save AI Settings</button>
                {saveStatus && <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>{saveStatus}</span>}
              </div>
            </div>

            <div className={styles.sectionHeader} style={{ marginTop: '24px' }}>
              <h3>Standalone Documents</h3>`;

txt = txt.replace(
  `<div className={styles.sectionHeader} style={{ marginTop: '24px' }}>
              <h3>Standalone Documents</h3>`,
  renderForm
);

fs.writeFileSync(file, txt);
