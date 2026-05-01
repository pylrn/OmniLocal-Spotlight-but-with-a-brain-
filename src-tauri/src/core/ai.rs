use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String>;
    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String>;
}

#[async_trait]
pub trait GenerationProvider: Send + Sync {
    async fn generate_answer(&self, prompt: &str, system_prompt: &str) -> Result<String, String>;
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderKind {
    Ollama,
    LMStudio,
    Gemini,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Ollama => "Ollama",
            ProviderKind::LMStudio => "LM Studio",
            ProviderKind::Gemini => "Gemini",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub connected: bool,
    pub provider: ProviderKind,
    pub model_available: bool,
    pub model_name: String,
    pub dimensions: Option<usize>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[async_trait]
impl EmbeddingProvider for OllamaClient {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        self.embed(texts).await
    }

    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        let mut result = self.embed(vec![query]).await?;
        let first = result.pop();
        first.ok_or_else(|| "Ollama returned no embedding for query".to_string())
    }
}

impl OllamaClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: build_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        let url = format!("{}/api/tags", self.base_url);

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<OllamaTagsResponse>().await {
                Ok(tags) => {
                    let model_available = tags.models.iter().any(|m| {
                        m.name == self.model || m.name.starts_with(&format!("{}:", self.model))
                    });

                    ProviderStatus {
                        connected: true,
                        provider: ProviderKind::Ollama,
                        model_available,
                        model_name: self.model.clone(),
                        dimensions: None,
                        error: if model_available {
                            None
                        } else {
                            Some(format!(
                                "Model '{}' not found. Run `ollama pull {}`.",
                                self.model, self.model
                            ))
                        },
                    }
                }
                Err(e) => ProviderStatus {
                    connected: true,
                    provider: ProviderKind::Ollama,
                    model_available: false,
                    model_name: self.model.clone(),
                    dimensions: None,
                    error: Some(format!("Failed to parse Ollama model list: {}", e)),
                },
            },
            Ok(resp) => ProviderStatus {
                connected: false,
                provider: ProviderKind::Ollama,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some(format!("Ollama returned status {}", resp.status())),
            },
            Err(e) => ProviderStatus {
                connected: false,
                provider: ProviderKind::Ollama,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some(format!("Cannot connect to Ollama at {}: {}", self.base_url, e)),
            },
        }
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/api/embed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&OllamaEmbedRequest {
                model: self.model.clone(),
                input: texts,
            })
            .send()
            .await
            .map_err(|e| format!("Ollama embedding request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, body));
        }

        let result: OllamaEmbedResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama embedding response: {}", e))?;

        Ok(result.embeddings)
    }
}

#[derive(Clone)]
pub struct LMStudioClient {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct LMStudioEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct LMStudioEmbedResponse {
    data: Vec<LMStudioEmbeddingData>,
}

#[derive(Deserialize)]
struct LMStudioEmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for LMStudioClient {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        self.embed(texts).await
    }

    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        let mut result = self.embed(vec![query]).await?;
        let first = result.pop();
        first.ok_or_else(|| "LM Studio returned no embedding for query".to_string())
    }
}

impl LMStudioClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: build_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        let url = format!("{}/v1/models", self.base_url);

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => ProviderStatus {
                connected: true,
                provider: ProviderKind::LMStudio,
                model_available: true,
                model_name: self.model.clone(),
                dimensions: None,
                error: None,
            },
            Ok(resp) => ProviderStatus {
                connected: false,
                provider: ProviderKind::LMStudio,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some(format!("LM Studio returned status {}", resp.status())),
            },
            Err(e) => ProviderStatus {
                connected: false,
                provider: ProviderKind::LMStudio,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some(format!("Cannot connect to LM Studio at {}: {}", self.base_url, e)),
            },
        }
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/v1/embeddings", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&LMStudioEmbedRequest {
                model: self.model.clone(),
                input: texts,
            })
            .send()
            .await
            .map_err(|e| format!("LM Studio embedding request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("LM Studio returned {}: {}", status, body));
        }

        let result: LMStudioEmbedResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse LM Studio response: {}", e))?;

        Ok(result.data.into_iter().map(|entry| entry.embedding).collect())
    }
}

#[derive(Clone)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbedRequest {
    model: String,
    content: GeminiContent,
    task_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimensionality: Option<usize>,
}

#[derive(Serialize, Clone)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Clone)]
struct GeminiPart {
    text: String,
}

#[derive(Deserialize)]
struct GeminiEmbedResponse {
    embedding: GeminiEmbedding,
}

#[derive(Serialize)]
struct GeminiBatchEmbedRequest {
    requests: Vec<GeminiEmbedRequest>,
}

#[derive(Deserialize)]
struct GeminiBatchEmbedResponse {
    embeddings: Vec<GeminiEmbedding>,
}

#[derive(Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for GeminiClient {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        
        if self.api_key.is_empty() {
            return Err("Gemini API key is not configured".to_string());
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/{}:batchEmbedContents?key={}",
            self.model, self.api_key
        );

        let requests: Vec<GeminiEmbedRequest> = texts
            .into_iter()
            .map(|text| GeminiEmbedRequest {
                model: self.model.clone(),
                content: GeminiContent {
                    parts: vec![GeminiPart { text }],
                },
                task_type: "RETRIEVAL_DOCUMENT",
                output_dimensionality: None,
            })
            .collect();

        // Optional: Chunk into batches of 100 if texts is very large
        let max_batch = 100;
        let mut all_embeddings = Vec::with_capacity(requests.len());

        for chunk in requests.chunks(max_batch) {
            let request = GeminiBatchEmbedRequest {
                requests: chunk.to_vec(),
            };

            let resp = self
                .client
                .post(&url)
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("Gemini batch embedding request failed: {}", e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Gemini returned {}: {}", status, body));
            }

            let result: GeminiBatchEmbedResponse = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse Gemini batch response: {}", e))?;

            for embedding in result.embeddings {
                all_embeddings.push(embedding.values);
            }
        }

        Ok(all_embeddings)
    }

    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        self.embed_one(&query, "RETRIEVAL_QUERY").await
    }
}

#[async_trait]
impl GenerationProvider for GeminiClient {
    async fn generate_answer(&self, prompt: &str, system_prompt: &str) -> Result<String, String> {
        // May 2026: The system prompt is now integrated into the generate_answer logic.
        // We use the existing gemini_generate_answer logic but refactored into this trait.
        
        let client = build_client();
        let mut last_error = String::new();
        
        // List of models to try in order.
        let fallbacks = ["gemini-3.1-flash-lite", "gemini-3-flash", "gemini-2.5-flash"];
        let mut models_to_try = vec![self.model.as_str()];
        for f in fallbacks {
            if f != self.model {
                models_to_try.push(f);
            }
        }

        for m in models_to_try {
            let normalized_model = if m.starts_with("models/") {
                m.to_string()
            } else {
                format!("models/{}", m)
            };

            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/{}:generateContent?key={}",
                normalized_model, self.api_key
            );

            let combined_prompt = format!("{}\n\n{}", system_prompt, prompt);

            let body = GeminiGenerateRequest {
                contents: vec![GeminiGenerateContent {
                    parts: vec![GeminiGeneratePart { text: combined_prompt }],
                }],
                generation_config: GeminiGenerationConfig {
                    max_output_tokens: 512,
                    temperature: 0.3,
                },
            };

            let resp_result = client
                .post(&url)
                .json(&body)
                .send()
                .await;

            match resp_result {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let result: GeminiGenerateResponse = resp
                            .json()
                            .await
                            .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

                        let text = result
                            .candidates
                            .and_then(|cs| cs.into_iter().next())
                            .and_then(|c| c.content)
                            .and_then(|c| c.parts.into_iter().next())
                            .and_then(|p| p.text)
                            .unwrap_or_else(|| "No insight generated.".to_string());

                        return Ok(text);
                    } else {
                        let status = resp.status();
                        let err_body = resp.text().await.unwrap_or_default();
                        last_error = format!("Gemini ({}) returned {}: {}", m, status, err_body);
                    }
                }
                Err(e) => {
                    last_error = format!("Gemini ({}) request failed: {}", m, e);
                }
            }
        }

        Err(format!("All models failed. Last error: {}", last_error))
    }
}

impl GeminiClient {
    pub fn new(api_key: &str, model: &str) -> Self {
        let normalized = if model.is_empty() {
            "models/gemini-embedding-001".to_string()
        } else if model.starts_with("models/") {
            model.to_string()
        } else {
            format!("models/{}", model)
        };

        Self {
            client: build_client(),
            api_key: api_key.to_string(),
            model: normalized,
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        if self.api_key.is_empty() {
            return ProviderStatus {
                connected: false,
                provider: ProviderKind::Gemini,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some("Gemini API key is not configured".to_string()),
            };
        }

        match self.embed_one("SmartSearch connectivity test", "RETRIEVAL_DOCUMENT").await {
            Ok(vector) => ProviderStatus {
                connected: true,
                provider: ProviderKind::Gemini,
                model_available: true,
                model_name: self.model.clone(),
                dimensions: Some(vector.len()),
                error: None,
            },
            Err(error) => ProviderStatus {
                connected: false,
                provider: ProviderKind::Gemini,
                model_available: false,
                model_name: self.model.clone(),
                dimensions: None,
                error: Some(error),
            },
        }
    }

    async fn embed_one(&self, text: &str, task_type: &'static str) -> Result<Vec<f32>, String> {
        if self.api_key.is_empty() {
            return Err("Gemini API key is not configured".to_string());
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/{}:embedContent?key={}",
            self.model, self.api_key
        );

        let resp = self
            .client
            .post(&url)
            .json(&GeminiEmbedRequest {
                model: self.model.clone(),
                content: GeminiContent {
                    parts: vec![GeminiPart {
                        text: text.to_string(),
                    }],
                },
                task_type,
                output_dimensionality: None,
            })
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
            .map_err(|e| format!("Failed to parse Gemini embedding response: {}", e))?;

        Ok(result.embedding.values)
    }
}

#[derive(Clone)]
pub enum AiProvider {
    Ollama(OllamaClient),
    LMStudio(LMStudioClient),
    Gemini(GeminiClient),
}

#[async_trait]
impl EmbeddingProvider for AiProvider {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        match self {
            AiProvider::Ollama(client) => client.embed_documents(texts).await,
            AiProvider::LMStudio(client) => client.embed_documents(texts).await,
            AiProvider::Gemini(client) => client.embed_documents(texts).await,
        }
    }

    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        match self {
            AiProvider::Ollama(client) => client.embed_query(query).await,
            AiProvider::LMStudio(client) => client.embed_query(query).await,
            AiProvider::Gemini(client) => client.embed_query(query).await,
        }
    }
}

#[async_trait]
impl GenerationProvider for AiProvider {
    async fn generate_answer(&self, prompt: &str, system_prompt: &str) -> Result<String, String> {
        match self {
            AiProvider::Gemini(client) => client.generate_answer(prompt, system_prompt).await,
            _ => Err(format!("Generation not supported for {} provider", self.provider_name())),
        }
    }
}

impl AiProvider {
    pub fn from_settings(
        provider: &str,
        ollama_url: &str,
        lmstudio_url: &str,
        api_key: &str,
        model: &str,
    ) -> Self {
        match provider {
            "lmstudio" => AiProvider::LMStudio(LMStudioClient::new(lmstudio_url, model)),
            "gemini" => AiProvider::Gemini(GeminiClient::new(api_key, model)),
            _ => AiProvider::Ollama(OllamaClient::new(ollama_url, model)),
        }
    }

    pub fn provider_name(&self) -> &'static str {
        match self {
            AiProvider::Ollama(_) => "Ollama",
            AiProvider::LMStudio(_) => "LM Studio",
            AiProvider::Gemini(_) => "Gemini",
        }
    }

    pub fn model_name(&self) -> &str {
        match self {
            AiProvider::Ollama(client) => &client.model,
            AiProvider::LMStudio(client) => &client.model,
            AiProvider::Gemini(client) => &client.model,
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        match self {
            AiProvider::Ollama(client) => client.check_status().await,
            AiProvider::LMStudio(client) => client.check_status().await,
            AiProvider::Gemini(client) => client.check_status().await,
        }
    }
}

fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("Failed to create HTTP client")
}

// ── Gemini generate-content (RAG querying) ────────────────────────────────────

#[derive(Serialize)]
struct GeminiGenerateRequest {
    contents: Vec<GeminiGenerateContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
struct GeminiGenerateContent {
    parts: Vec<GeminiGeneratePart>,
}

#[derive(Serialize)]
struct GeminiGeneratePart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    max_output_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct GeminiGenerateResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiCandidateContent>,
}

#[derive(Deserialize)]
struct GeminiCandidateContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

