use std::sync::{Arc, Mutex};
use std::path::Path;
use tokio::sync::mpsc;
use rusqlite::Connection;
use tauri::Emitter;
use crate::core::ai::{AiProvider, EmbeddingProvider};
use crate::core::db;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingRuntimeStatus {
    pub phase: String,
    pub current_title: Option<String>,
    pub current_path: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub message: String,
}

impl Default for EmbeddingRuntimeStatus {
    fn default() -> Self {
        Self {
            phase: "idle".to_string(),
            current_title: None,
            current_path: None,
            provider: None,
            model: None,
            message: "Waiting for indexing work".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EmbedCommand {
    Kick,
    RetryFailed,
    ReembedAll,
}

pub fn spawn_embedding_worker(
    db_path: String,
    ai_provider: Arc<Mutex<Option<AiProvider>>>,
    runtime: Arc<Mutex<EmbeddingRuntimeStatus>>,
    app: tauri::AppHandle,
    mut rx: mpsc::UnboundedReceiver<EmbedCommand>,
) {
    tauri::async_runtime::spawn(async move {
        let db = match db::init_db(Path::new(&db_path)) {
            Ok(conn) => Arc::new(Mutex::new(conn)),
            Err(error) => {
                set_runtime(
                    &runtime,
                    EmbeddingRuntimeStatus {
                        phase: "error".to_string(),
                        current_title: None,
                        current_path: None,
                        provider: None,
                        model: None,
                        message: format!("Embedding worker DB init failed: {}", error),
                    },
                );
                let _ = app.emit(
                    "embedding-progress",
                    serde_json::json!({
                        "runtime": runtime.lock().map(|guard| guard.clone()).unwrap_or_default(),
                        "stats": db::IndexStats {
                            total_collections: 0,
                            total_documents: 0,
                            total_chunks: 0,
                            embedded_chunks: 0,
                            pending_chunks: 0,
                            failed_chunks: 0,
                        },
                    }),
                );
                return;
            }
        };

        while let Some(command) = rx.recv().await {
            match command {
                EmbedCommand::RetryFailed | EmbedCommand::ReembedAll | EmbedCommand::Kick => {
                    if let Err(error) = process_pending_embeddings(&db, &ai_provider, &runtime, &app).await {
                        set_runtime(
                            &runtime,
                            EmbeddingRuntimeStatus {
                                phase: "error".to_string(),
                                current_title: None,
                                current_path: None,
                                provider: None,
                                model: None,
                                message: error.clone(),
                            },
                        );
                        emit_runtime(&app, &db, &runtime);
                    }
                }
            }
        }
    });
}

pub async fn process_pending_embeddings(
    db: &Arc<Mutex<Connection>>,
    ai_provider: &Arc<Mutex<Option<AiProvider>>>,
    runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    loop {
        let auto_embed_enabled = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::get_setting(&conn, "auto_embed_enabled")
                .map_err(|e| format!("DB error: {}", e))?
                .unwrap_or_else(|| "true".to_string())
                == "true"
        };

        if !auto_embed_enabled {
            set_runtime(
                runtime,
                EmbeddingRuntimeStatus {
                    phase: "paused".to_string(),
                    current_title: None,
                    current_path: None,
                    provider: None,
                    model: None,
                    message: "Auto-embedding is disabled".to_string(),
                },
            );
            emit_runtime(app, db, runtime);
            return Ok(());
        }

        let batch_size = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::get_setting(&conn, "embed_batch_size")
                .ok()
                .flatten()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(12)
        };

        let pending_chunks = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::list_pending_chunks(&conn, batch_size).map_err(|e| format!("Failed to list pending chunks: {}", e))?
        };

        if pending_chunks.is_empty() {
            set_runtime(runtime, EmbeddingRuntimeStatus::default());
            emit_runtime(app, db, runtime);
            return Ok(());
        }

        let provider = {
            let guard = ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
            guard.clone()
        }
        .ok_or_else(|| "AI provider is not configured".to_string())?;

        {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            for chunk in &pending_chunks {
                db::mark_chunk_embedding_in_progress(&conn, chunk.chunk_id)
                    .map_err(|e| format!("Failed to mark chunk processing: {}", e))?;
            }
        }

        let lead = &pending_chunks[0];
        set_runtime(
            runtime,
            EmbeddingRuntimeStatus {
                phase: "embedding".to_string(),
                current_title: lead.document_title.clone(),
                current_path: Some(lead.abs_path.clone()),
                provider: Some(provider.provider_name().to_string()),
                model: Some(provider.model_name().to_string()),
                message: format!("Embedding {} chunk(s)", pending_chunks.len()),
            },
        );
        emit_runtime(app, db, runtime);

        let results = embed_batch_with_fallback(&provider, &pending_chunks).await;
        {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            for (chunk, embedding_result) in pending_chunks.iter().zip(results.into_iter()) {
                match embedding_result {
                    Ok(vector) => db::mark_chunk_embedding_success(
                        &conn,
                        chunk.chunk_id,
                        provider.provider_name(),
                        provider.model_name(),
                        &vector,
                    )
                    .map_err(|e| format!("Failed to store embedding: {}", e))?,
                    Err(error) => db::mark_chunk_embedding_failed(&conn, chunk.chunk_id, &error)
                        .map_err(|e| format!("Failed to store embedding error: {}", e))?,
                }
            }
        }

        emit_runtime(app, db, runtime);
    }
}

async fn embed_batch_with_fallback(
    provider: &AiProvider,
    pending_chunks: &[db::PendingChunk],
) -> Vec<Result<Vec<f32>, String>> {
    let texts = pending_chunks.iter().map(|chunk| chunk.content.clone()).collect::<Vec<_>>();
    match provider.embed_documents(texts).await {
        Ok(embeddings) if embeddings.len() == pending_chunks.len() => embeddings.into_iter().map(Ok).collect(),
        Ok(_) | Err(_) => {
            let mut results = Vec::with_capacity(pending_chunks.len());
            for chunk in pending_chunks {
                match provider.embed_documents(vec![chunk.content.clone()]).await {
                    Ok(mut embeddings) => {
                        if let Some(vector) = embeddings.pop() {
                            results.push(Ok(vector));
                        } else {
                            results.push(Err(format!("No embedding returned for {}", chunk.path)));
                        }
                    }
                    Err(error) => results.push(Err(error)),
                }
            }
            results
        }
    }
}

pub fn set_runtime(runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>, next: EmbeddingRuntimeStatus) {
    if let Ok(mut guard) = runtime.lock() {
        *guard = next;
    }
}

pub fn emit_runtime(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Connection>>,
    runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>,
) {
    let runtime_snapshot = runtime.lock().map(|guard| guard.clone()).unwrap_or_default();
    let stats = db
        .lock()
        .ok()
        .and_then(|conn| db::get_index_stats(&conn).ok())
        .unwrap_or(db::IndexStats {
            total_collections: 0,
            total_documents: 0,
            total_chunks: 0,
            embedded_chunks: 0,
            pending_chunks: 0,
            failed_chunks: 0,
        });

    let _ = app.emit(
        "embedding-progress",
        serde_json::json!({
            "runtime": runtime_snapshot,
            "stats": stats,
        }),
    );
}

pub fn kick_embeddings(embedding_tx: &mpsc::UnboundedSender<EmbedCommand>) {
    let _ = embedding_tx.send(EmbedCommand::Kick);
}
