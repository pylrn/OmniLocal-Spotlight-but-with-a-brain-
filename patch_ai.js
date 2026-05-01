const fs = require('fs');
let txt = fs.readFileSync('src-tauri/src/ai.rs', 'utf8');

// Insert ProviderKind::Gemini and fix Enum
txt = txt.replace(/pub enum ProviderKind \{\n    Ollama,\n    LMStudio,\n\}/, 
\`pub enum ProviderKind {
    Ollama,
    LMStudio,
    Gemini,
}\`);

// Insert GeminiClient struct and impl before AiProvider enum
const geminiClientStr = \`
// ═══════════════════════════════════════════════════════════════════════
// Gemini Client
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct GeminiEmbedRequest {
    requests: Vec<GeminiEmbedRequestItem>,
}

#[derive(Serialize)]
struct GeminiEmbedRequestItem {
    model: String,
    content: GeminiContent,
}

#[derive(Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Deserialize)]
struct GeminiEmbedResponse {
    embeddings: Vec<GeminiEmbedding>,
}

#[derive(Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

impl GeminiClient {
    pub fn new(api_key: &str, model: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: api_key.to_string(),
            model: if model.starts_with("models/") {
                model.to_string()
            } else {
                format!("models/{}", model)
            },
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        if self.api_key.is_empty() {
            return ProviderStatus {
                connected: false,
                provider: ProviderKind::Gemini,
                model_available: false,
                model_name: self.model.clone(),
                error: Some("Gemini API key is not configured".to_string()),
            };
        }
        
        let url = format!("https://generativelanguage.googleapis.com/v1beta/{}:embedContent?key={}", self.model, self.api_key);
        
        let request = GeminiEmbedRequestItem {
            model: self.model.clone(),
            content: GeminiContent {
                parts: vec![GeminiPart { text: "test".to_string() }],
            },
        };

        match self.client.post(&url).json(&request).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    ProviderStatus {
                        connected: true,
                        provider: ProviderKind::Gemini,
                        model_available: true,
                        model_name: self.model.clone(),
                        error: None,
                    }
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    ProviderStatus {
                        connected: false,
                        provider: ProviderKind::Gemini,
                        model_available: false,
                        model_name: self.model.clone(),
                        error: Some(format!("Gemini API returned {}: {}", status, body)),
                    }
                }
            }
            Err(e) => ProviderStatus {
                connected: false,
                provider: ProviderKind::Gemini,
                model_available: false,
                model_name: self.model.clone(),
                error: Some(format!("Cannot connect to Gemini API: {}", e)),
            },
        }
    }

    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        if self.api_key.is_empty() {
            return Err("Gemini API key is not configured".to_string());
        }

        let url = format!("https://generativelanguage.googleapis.com/v1beta/{}:batchEmbedContents?key={}", self.model, self.api_key);

        let requests: Vec<GeminiEmbedRequestItem> = texts.into_iter().map(|text| {
            GeminiEmbedRequestItem {
                model: self.model.clone(),
                content: GeminiContent {
                    parts: vec![GeminiPart { text }],
                },
            }
        }).collect();

        let max_batch = 100;
        let mut all_embeddings = Vec::new();

        for chunk_req in requests.chunks(max_batch) {
            let request = GeminiEmbedRequest { requests: chunk_req.to_vec() };

            let resp = self.client
                .post(&url)
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("Gemini embedding request failed: {}", e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Gemini returned {}: {}", status, body));
            }

            let result: GeminiEmbedResponse = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;
            
            all_embeddings.extend(result.embeddings.into_iter().map(|d| d.values));
        }

        Ok(all_embeddings)
    }
}
\`;

txt = txt.replace("// ═══════════════════════════════════════════════════════════════════════\n// Unified Provider enum", geminiClientStr + "\\n// ═══════════════════════════════════════════════════════════════════════\n// Unified Provider enum");

// Update AiProvider Enum
txt = txt.replace(/pub enum AiProvider \{\n    Ollama\(OllamaClient\),\n    LMStudio\(LMStudioClient\),\n\}/, 
\`pub enum AiProvider {
    Ollama(OllamaClient),
    LMStudio(LMStudioClient),
    Gemini(GeminiClient),
}\`);

// Update from_settings
txt = txt.replace(/pub fn from_settings\(provider: &str, ollama_url: &str, lmstudio_url: &str, model: &str\) -> Self \{\n        match provider \{\n            "lmstudio" => AiProvider::LMStudio\(LMStudioClient::new\(lmstudio_url, model\)\),\n            _ => AiProvider::Ollama\(OllamaClient::new\(ollama_url, model\)\),\n        \}\n    \}/, 
\`pub fn from_settings(provider: &str, ollama_url: &str, lmstudio_url: &str, api_key: &str, model: &str) -> Self {
        match provider {
            "lmstudio" => AiProvider::LMStudio(LMStudioClient::new(lmstudio_url, model)),
            "gemini" => AiProvider::Gemini(GeminiClient::new(api_key, model)),
            _ => AiProvider::Ollama(OllamaClient::new(ollama_url, model)),
        }
    }\`);

// Update check_status
txt = txt.replace(/match self \{\n            AiProvider::Ollama\(c\) => c\.check_status\(\)\.await,\n            AiProvider::LMStudio\(c\) => c\.check_status\(\)\.await,\n        \}/, 
\`match self {
            AiProvider::Ollama(c) => c.check_status().await,
            AiProvider::LMStudio(c) => c.check_status().await,
            AiProvider::Gemini(c) => c.check_status().await,
        }\`);

// Update embed
txt = txt.replace(/match self \{\n            AiProvider::Ollama\(c\) => c\.embed\(texts\)\.await,\n            AiProvider::LMStudio\(c\) => c\.embed\(texts\)\.await,\n        \}/, 
\`match self {
            AiProvider::Ollama(c) => c.embed(texts).await,
            AiProvider::LMStudio(c) => c.embed(texts).await,
            AiProvider::Gemini(c) => c.embed(texts).await,
        }\`);

fs.writeFileSync('src-tauri/src/ai.rs', txt);
