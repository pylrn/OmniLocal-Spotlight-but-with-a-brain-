// SmartSearch — AI Provider Abstraction
// Supports Ollama and LM Studio locally, plus Gemini as an opt-in cloud provider.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

    pub async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        self.embed(texts).await
    }

    pub async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        let mut result = self.embed(vec![query]).await?;
        let first = result.pop();
        first.ok_or_else(|| "Ollama returned no embedding for query".to_string())
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

    pub async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        self.embed(texts).await
    }

    pub async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        let mut result = self.embed(vec![query]).await?;
        let first = result.pop();
        first.ok_or_else(|| "LM Studio returned no embedding for query".to_string())
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiEmbedRequest {
    model: String,
    content: GeminiContent,
    task_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimensionality: Option<usize>,
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
    embedding: GeminiEmbedding,
}

#[derive(Deserialize)]
struct GeminiEmbedding {
    values: Vec<f32>,
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

    pub async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        let mut output = Vec::with_capacity(texts.len());
        for text in texts {
            output.push(self.embed_one(&text, "RETRIEVAL_DOCUMENT").await?);
        }
        Ok(output)
    }

    pub async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        self.embed_one(&query, "RETRIEVAL_QUERY").await
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

    pub async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        match self {
            AiProvider::Ollama(client) => client.embed_documents(texts).await,
            AiProvider::LMStudio(client) => client.embed_documents(texts).await,
            AiProvider::Gemini(client) => client.embed_documents(texts).await,
        }
    }

    pub async fn embed_query(&self, query: String) -> Result<Vec<f32>, String> {
        match self {
            AiProvider::Ollama(client) => client.embed_query(query).await,
            AiProvider::LMStudio(client) => client.embed_query(query).await,
            AiProvider::Gemini(client) => client.embed_query(query).await,
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

pub async fn gemini_generate_answer(
    api_key: &str,
    model: &str,
    query: &str,
    context: &[(String, String)], // (snippet, source_path)
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("Query model API key is not configured. Add it in Settings → Intelligence.".to_string());
    }

    // List of models to try in order. If the user-provided model is not in this list, 
    // it will be tried first as the primary choice.
    let mut models_to_try = Vec::new();
    let primary_model = if model.is_empty() { "gemini-3-flash" } else { model };
    models_to_try.push(primary_model);

    // Fallbacks if primary fails (order: newest flash -> lightest flash -> stable flash)
    // Updated for May 2026: prioritizing Gemini 3.1 series
    let fallbacks = ["gemini-3.1-flash-lite", "gemini-3-flash", "gemini-2.5-flash"];
    for f in fallbacks {
        if f != primary_model {
            models_to_try.push(f);
        }
    }

    let context_text: String = context
        .iter()
        .enumerate()
        .map(|(i, (snippet, path))| format!("[{}] {}\n{}", i + 1, path, snippet))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "You are a local file search assistant. The user searched their local files for: \"{}\"\n\nHere are the most relevant excerpts, ranked by relevance:\n\n{}\n\nProvide a concise insight (2-4 sentences) that:\n- Directly addresses what the user was looking for\n- Highlights the most relevant finding (mention which result number)\n- Notes any key patterns across results if applicable\n\nBe specific and direct. Do not repeat file paths or source numbers unless citing.",
        query, context_text
    );

    let client = build_client();
    let mut last_error = String::new();

    for m in models_to_try {
        let normalized_model = if m.starts_with("models/") {
            m.to_string()
        } else {
            format!("models/{}", m)
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/{}:generateContent?key={}",
            normalized_model, api_key
        );

        let body = GeminiGenerateRequest {
            contents: vec![GeminiGenerateContent {
                parts: vec![GeminiGeneratePart { text: prompt.clone() }],
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
                    // Continue loop to try next model
                }
            }
            Err(e) => {
                last_error = format!("Gemini ({}) request failed: {}", m, e);
                // Continue loop
            }
        }
    }

    Err(format!("All models failed. Last error: {}", last_error))
}
