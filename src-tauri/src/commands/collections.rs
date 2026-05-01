use tauri::State;
use crate::AppState;
use crate::core::db;
use crate::core::db::CollectionRow;

#[tauri::command]
pub fn add_collection(
    state: State<'_, AppState>,
    name: String,
    path: String,
    glob_pattern: Option<String>,
    context: Option<String>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let pattern = glob_pattern.unwrap_or_else(|| "**/*.md".to_string());
    db::insert_collection(&conn, &name, &path, &pattern, context.as_deref())
        .map_err(|e| format!("Failed to add collection: {}", e))
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_collections(&conn).map_err(|e| format!("Failed to list collections: {}", e))
}

#[tauri::command]
pub fn remove_collection(state: State<'_, AppState>, name: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::remove_collection(&conn, &name).map_err(|e| format!("Failed to remove collection: {}", e))
}
