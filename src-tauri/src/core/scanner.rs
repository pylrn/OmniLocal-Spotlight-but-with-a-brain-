// SmartSearch — File Scanner & Indexing Pipeline
// Scans directories, detects changes via content hashing,
// chunks documents, and stores them in the database.

use std::fs;
use std::path::Path;

use glob::Pattern;
use log::{debug, info, warn};
use rusqlite::{Connection, OptionalExtension};
use walkdir::WalkDir;

use crate::core::chunker::{self, extract_title, ChunkerConfig};
use crate::core::db;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanResult {
    pub collection_name: String,
    pub files_found: usize,
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub errors: Vec<String>,
}

pub fn scan_collection(
    conn: &Connection,
    collection_id: i64,
    collection_path: &str,
    glob_pattern: &str,
    collection_name: &str,
) -> Result<ScanResult, String> {
    let base_path = Path::new(collection_path);
    if !base_path.exists() {
        return Err(format!("Collection path does not exist: {}", collection_path));
    }

    let pattern = Pattern::new(glob_pattern)
        .map_err(|e| format!("Invalid glob pattern '{}': {}", glob_pattern, e))?;

    let mut result = ScanResult {
        collection_name: collection_name.to_string(),
        files_found: 0,
        files_indexed: 0,
        files_unchanged: 0,
        files_removed: 0,
        errors: Vec::new(),
    };

    let mut found_paths = Vec::new();

    for entry in WalkDir::new(base_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let abs_path = entry.path();
        let rel_path = match abs_path.strip_prefix(base_path) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if !pattern.matches(&rel_path) || should_ignore(&rel_path) {
            continue;
        }

        result.files_found += 1;
        found_paths.push(rel_path.clone());

        match index_file(conn, collection_id, base_path, &rel_path) {
            Ok(IndexStatus::Indexed) => result.files_indexed += 1,
            Ok(IndexStatus::Unchanged) => result.files_unchanged += 1,
            Err(error) => {
                let message = format!("Error indexing {}: {}", rel_path, error);
                warn!("{}", message);
                result.errors.push(message);
            }
        }
    }

    result.files_removed = mark_missing_documents(conn, collection_id, &found_paths)?;

    info!(
        "Scan complete for '{}': {} found, {} indexed, {} unchanged, {} removed",
        collection_name, result.files_found, result.files_indexed, result.files_unchanged, result.files_removed
    );

    Ok(result)
}

enum IndexStatus {
    Indexed,
    Unchanged,
}

fn index_file(
    conn: &Connection,
    collection_id: i64,
    base_path: &Path,
    rel_path: &str,
) -> Result<IndexStatus, String> {
    let abs_path = base_path.join(rel_path);
    let abs_path_str = abs_path.to_string_lossy().to_string();
    let file_type = abs_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt")
        .to_lowercase();

    let content = read_supported_content(&abs_path, &file_type)?;
    if content.trim().is_empty() {
        debug!("Skipping empty or unreadable file: {}", rel_path);
        return Ok(IndexStatus::Unchanged);
    }

    let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
    if let Some(existing_hash) =
        db::get_document_hash(conn, collection_id, rel_path).map_err(|e| format!("DB error: {}", e))?
    {
        if existing_hash == hash {
            return Ok(IndexStatus::Unchanged);
        }
    }

    let metadata = fs::metadata(&abs_path).map_err(|e| format!("Cannot read metadata: {}", e))?;
    let last_modified = metadata.modified().ok().map(|time| {
        let datetime: chrono::DateTime<chrono::Utc> = time.into();
        datetime.to_rfc3339()
    });

    let document_id = db::upsert_document(
        conn,
        collection_id,
        rel_path,
        &abs_path_str,
        extract_title(&content).as_deref(),
        &hash,
        metadata.len() as i64,
        &file_type,
        last_modified.as_deref(),
    )
    .map_err(|e| format!("DB upsert error: {}", e))?;

    db::delete_chunks_for_document(conn, document_id).map_err(|e| format!("DB chunk cleanup error: {}", e))?;

    let config = ChunkerConfig::default();
    let chunks = chunker::fixed_size_chunk(&content, &config);

    for chunk in chunks {
        db::insert_chunk(
            conn,
            document_id,
            chunk.chunk_index as i32,
            &chunk.content,
            Some(chunk.start_line as i32),
            Some(chunk.end_line as i32),
            Some(chunk.estimated_tokens as i32),
        )
        .map_err(|e| format!("DB chunk insert error: {}", e))?;
    }

    debug!("Indexed: {}", rel_path);
    Ok(IndexStatus::Indexed)
}

fn read_supported_content(path: &Path, file_type: &str) -> Result<String, String> {
    match file_type {
        "pdf" => Ok(pdf_extract::extract_text(path).unwrap_or_default()),
        "docx" => {
            let file = fs::File::open(path).map_err(|e| format!("Cannot read docx: {}", e))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Cannot open docx zip: {}", e))?;
            let mut text = String::new();
            if let Ok(mut doc) = archive.by_name("word/document.xml") {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut doc, &mut xml).unwrap_or_default();
                let mut in_tag = false;
                for ch in xml.chars() {
                    match ch {
                        '<' => in_tag = true,
                        '>' => {
                            in_tag = false;
                            text.push(' ');
                        }
                        _ if !in_tag => text.push(ch),
                        _ => {}
                    }
                }
            }
            Ok(text)
        }
        _ => Ok(fs::read_to_string(path).unwrap_or_default()),
    }
}

fn mark_missing_documents(conn: &Connection, collection_id: i64, found_paths: &[String]) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT id, path FROM documents WHERE collection_id = ?1 AND is_active = 1")
        .map_err(|e| format!("DB error: {}", e))?;

    let db_docs: Vec<(i64, String)> = stmt
        .query_map(rusqlite::params![collection_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("DB error: {}", e))?
        .filter_map(|row| row.ok())
        .collect();

    let mut removed = 0;
    for (document_id, path) in db_docs {
        if !found_paths.contains(&path) {
            conn.execute(
                "UPDATE documents SET is_active = 0 WHERE id = ?1",
                rusqlite::params![document_id],
            )
            .map_err(|e| format!("DB error: {}", e))?;
            db::delete_chunks_for_document(conn, document_id).map_err(|e| format!("DB error: {}", e))?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn should_ignore(rel_path: &str) -> bool {
    let ignore_patterns = [
        "node_modules/",
        ".git/",
        ".svn/",
        ".hg/",
        "__pycache__/",
        ".DS_Store",
        "target/",
        "dist/",
        "build/",
        ".next/",
        ".vscode/",
        ".idea/",
    ];

    if ignore_patterns
        .iter()
        .any(|pattern| rel_path.contains(pattern) || rel_path.starts_with(pattern))
    {
        return true;
    }

    rel_path
        .split('/')
        .any(|component| component.starts_with('.') && component != "." && component != "..")
}

pub fn index_single_file(conn: &Connection, abs_path: &Path) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT id, path, glob_pattern FROM collections")
        .map_err(|e| format!("DB error: {}", e))?;

    let collections: Vec<(i64, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "**/*".to_string()),
            ))
        })
        .map_err(|e| format!("DB error: {}", e))?
        .filter_map(|row| row.ok())
        .collect();

    for (collection_id, collection_path, glob_pattern) in collections {
        let base = Path::new(&collection_path);
        if let Ok(rel_path) = abs_path.strip_prefix(base) {
            let rel_str = rel_path.to_string_lossy().to_string();
            if let Ok(pattern) = Pattern::new(&glob_pattern) {
                if pattern.matches(&rel_str) && !should_ignore(&rel_str) {
                    return match index_file(conn, collection_id, base, &rel_str)? {
                        IndexStatus::Indexed => Ok(true),
                        IndexStatus::Unchanged => Ok(false),
                    };
                }
            }
        }
    }

    Ok(false)
}

pub fn remove_single_file(conn: &Connection, abs_path: &Path) -> Result<bool, String> {
    let path_str = abs_path.to_string_lossy().to_string();
    
    let doc_id: Option<i64> = conn.query_row(
        "SELECT id FROM documents WHERE abs_path = ?1 AND is_active = 1",
        rusqlite::params![path_str],
        |row| row.get(0)
    ).optional().map_err(|e| format!("DB error: {}", e))?;

    if let Some(id) = doc_id {
        conn.execute("UPDATE documents SET is_active = 0 WHERE id = ?1", rusqlite::params![id]).map_err(|e| format!("DB error: {}", e))?;
        db::delete_chunks_for_document(conn, id).map_err(|e| format!("DB error: {}", e))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn index_manual_file(conn: &Connection, abs_path: &Path, collection_id: i64) -> Result<bool, String> {
    if !abs_path.is_file() {
        return Err("Path is not a valid file".to_string());
    }

    let parent_dir = abs_path.parent().unwrap_or(Path::new("/"));
    let file_name = abs_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    match index_file(conn, collection_id, parent_dir, &file_name)? {
        IndexStatus::Indexed | IndexStatus::Unchanged => Ok(true),
    }
}
