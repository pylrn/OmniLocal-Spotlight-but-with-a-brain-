use tauri::State;
use crate::AppState;
use crate::core::db;
use crate::core::db::{IndexOverview, IndexStats, DocumentStatusRow, ProviderHealth};
use crate::core::ai::ProviderStatus;
use crate::workers::embedding::{self, EmbeddingRuntimeStatus, EmbedCommand};

#[tauri::command]
pub fn get_index_stats(state: State<'_, AppState>) -> Result<IndexStats, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_index_stats(&conn).map_err(|e| format!("Failed to get stats: {}", e))
}

#[tauri::command]
pub async fn get_index_overview(state: State<'_, AppState>) -> Result<IndexOverview, String> {
    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    let provider_health = match provider {
        Some(provider) => provider_health_from_status(provider.check_status().await),
        None => ProviderHealth {
            provider: "Unavailable".to_string(),
            model_name: String::new(),
            connected: false,
            model_available: false,
            dimensions: None,
            error: Some("AI provider is not configured".to_string()),
        },
    };

    let watcher_enabled = *state
        .watcher_enabled
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    db::get_index_overview(&conn, &state.db_path, provider_health, watcher_enabled)
        .map_err(|e| format!("Failed to get overview: {}", e))
}

fn provider_health_from_status(status: ProviderStatus) -> ProviderHealth {
    ProviderHealth {
        provider: status.provider.as_str().to_string(),
        model_name: status.model_name,
        connected: status.connected,
        model_available: status.model_available,
        dimensions: status.dimensions,
        error: status.error,
    }
}

#[tauri::command]
pub fn list_document_statuses(state: State<'_, AppState>) -> Result<Vec<DocumentStatusRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_document_statuses(&conn).map_err(|e| format!("Failed to list document statuses: {}", e))
}

#[tauri::command]
pub fn get_embedding_runtime(state: State<'_, AppState>) -> Result<EmbeddingRuntimeStatus, String> {
    let runtime = state
        .embedding_runtime
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .clone();
    Ok(runtime)
}

#[tauri::command]
pub async fn check_ai_status(state: State<'_, AppState>) -> Result<ProviderStatus, String> {
    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    match provider {
        Some(provider) => Ok(provider.check_status().await),
        None => Err("AI provider not configured".to_string()),
    }
}

#[tauri::command]
pub async fn test_ai_provider(state: State<'_, AppState>) -> Result<ProviderStatus, String> {
    check_ai_status(state).await
}

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_setting(&conn, &key).map_err(|e| format!("Failed to get setting: {}", e))
}

#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::set_setting(&conn, &key, &value).map_err(|e| format!("Failed to set setting: {}", e))?;
    }

    if key == "watch_enabled" {
        let mut watcher_enabled = state
            .watcher_enabled
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        *watcher_enabled = value == "true";
    }

    if matches!(
        key.as_str(),
        "ai_provider" | "ollama_base_url" | "lmstudio_base_url" | "gemini_api_key" | "embed_model"
    ) {
        crate::reload_ai_provider(&state)?;
        embedding::kick_embeddings(&state.embedding_tx);
    }

    if key == "auto_embed_enabled" && value == "true" {
        embedding::kick_embeddings(&state.embedding_tx);
    }

    Ok(())
}

#[tauri::command]
pub fn retry_failed_embeddings(state: State<'_, AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::retry_failed_chunks(&conn).map_err(|e| format!("Failed to retry failed embeddings: {}", e))?;
    }

    state
        .embedding_tx
        .send(EmbedCommand::RetryFailed)
        .map_err(|e| format!("Failed to queue retry: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn retry_document_embeddings(state: State<'_, AppState>, document_id: i64) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::retry_failed_chunks_for_document(&conn, document_id)
            .map_err(|e| format!("Failed to retry document embeddings: {}", e))?;
    }

    state
        .embedding_tx
        .send(EmbedCommand::Kick)
        .map_err(|e| format!("Failed to queue document retry: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn reembed_all(state: State<'_, AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::mark_all_chunks_pending(&conn).map_err(|e| format!("Failed to mark chunks pending: {}", e))?;
    }

    state
        .embedding_tx
        .send(EmbedCommand::ReembedAll)
        .map_err(|e| format!("Failed to queue re-embed all: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn bulk_retry_documents(state: State<'_, AppState>, document_ids: Vec<i64>) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        for doc_id in document_ids {
            db::retry_failed_chunks_for_document(&conn, doc_id)
                .map_err(|e| format!("Failed to retry document {}: {}", doc_id, e))?;
        }
    }

    state
        .embedding_tx
        .send(EmbedCommand::Kick)
        .map_err(|e| format!("Failed to queue bulk retry: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn bulk_remove_documents(state: State<'_, AppState>, document_ids: Vec<i64>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    for doc_id in document_ids {
        db::remove_document(&conn, doc_id)
            .map_err(|e| format!("Failed to remove document {}: {}", doc_id, e))?;
    }
    Ok(())
}
