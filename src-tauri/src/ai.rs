// SmartSearch — AI Provider Abstraction
// Supports Ollama and LM Studio via their localhost REST APIs

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use log::{info, warn};

// ═══════════════════════════════════════════════════════════════════════
// Provider trait
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    pub embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderKind {
    Ollama,
    LMStudio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub connected: bool,
    pub provider: ProviderKind,
    pub model_available: bool,
    pub model_name: String,
    pub error: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// Ollama Client
// ═══════════════════════════════════════════════════════════════════════

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
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Check if Ollama is running and the model is available
    pub async fn check_status(&self) -> ProviderStatus {
        let url = format!("{}/api/tags", self.base_url);

        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<OllamaTagsResponse>().await {
                        Ok(tags) => {
                            let model_available = tags.models.iter().any(|m| {
                                m.name == self.model || m.name.starts_with(&format!("{}:", self.model))
                            });

                            ProviderStatus {
                                connected: true,
                                provider: ProviderKind::Ollama,
                                model_available,
                                model_name: self.model.clone(),
                                error: if model_available {
                                    None
                                } else {
                                    Some(format!(
                                        "Model '{}' not found. Run: ollama pull {}",
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
                            error: Some(format!("Failed to parse response: {}", e)),
                        },
                    }
                } else {
                    ProviderStatus {
                        connected: false,
                        provider: ProviderKind::Ollama,
                        model_available: false,
                        model_name: self.model.clone(),
                        error: Some(format!("Ollama returned status {}", resp.status())),
                    }
                }
            }
            Err(e) => ProviderStatus {
                connected: false,
                provider: ProviderKind::Ollama,
                model_available: false,
                model_name: self.model.clone(),
                error: Some(format!("Cannot connect to Ollama at {}: {}", self.base_url, e)),
            },
        }
    }

    /// Generate embeddings for a batch of texts
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/api/embed", self.base_url);

        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            input: texts,
        };

        let resp = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Embedding request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, body));
        }

        let result: OllamaEmbedResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse embedding response: {}", e))?;

        Ok(result.embeddings)
    }

    /// Get the embedding dimensions by sending a test string
    pub async fn get_dimensions(&self) -> Result<usize, String> {
        let result = self.embed(vec!["test".to_string()]).await?;
        if let Some(first) = result.first() {
            Ok(first.len())
        } else {
            Err("No embeddings returned for test input".to_string())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LM Studio Client
// ═══════════════════════════════════════════════════════════════════════

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
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Check if LM Studio is running
    pub async fn check_status(&self) -> ProviderStatus {
        let url = format!("{}/v1/models", self.base_url);

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                ProviderStatus {
                    connected: true,
                    provider: ProviderKind::LMStudio,
                    model_available: true,
                    model_name: self.model.clone(),
                    error: None,
                }
            }
            Ok(resp) => ProviderStatus {
                connected: false,
                provider: ProviderKind::LMStudio,
                model_available: false,
                model_name: self.model.clone(),
                error: Some(format!("LM Studio returned status {}", resp.status())),
            },
            Err(e) => ProviderStatus {
                connected: false,
                provider: ProviderKind::LMStudio,
                model_available: false,
                model_name: self.model.clone(),
                error: Some(format!("Cannot connect to LM Studio: {}", e)),
            },
        }
    }

    /// Generate embeddings
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/v1/embeddings", self.base_url);

        let request = LMStudioEmbedRequest {
            model: self.model.clone(),
            input: texts,
        };

        let resp = self.client
            .post(&url)
            .json(&request)
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

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Unified Provider enum
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub enum AiProvider {
    Ollama(OllamaClient),
    LMStudio(LMStudioClient),
}

impl AiProvider {
    pub fn from_settings(provider: &str, ollama_url: &str, lmstudio_url: &str, model: &str) -> Self {
        match provider {
            "lmstudio" => AiProvider::LMStudio(LMStudioClient::new(lmstudio_url, model)),
            _ => AiProvider::Ollama(OllamaClient::new(ollama_url, model)),
        }
    }

    pub async fn check_status(&self) -> ProviderStatus {
        match self {
            AiProvider::Ollama(c) => c.check_status().await,
            AiProvider::LMStudio(c) => c.check_status().await,
        }
    }

    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        match self {
            AiProvider::Ollama(c) => c.embed(texts).await,
            AiProvider::LMStudio(c) => c.embed(texts).await,
        }
    }
}
