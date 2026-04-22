// SmartSearch — File Scanner & Indexing Pipeline
// Scans directories, detects changes via content hashing,
// chunks documents, and stores them in the database.

use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use glob::Pattern;
use log::{info, warn, debug};
use rusqlite::Connection;

use crate::db;
use crate::chunker::{self, ChunkerConfig, TextChunk, extract_title};

/// Result of a scan operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanResult {
    pub collection_name: String,
    pub files_found: usize,
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub errors: Vec<String>,
}

/// Result of an embedding operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbedResult {
    pub chunks_embedded: usize,
    pub chunks_skipped: usize,
    pub errors: Vec<String>,
}

/// Scan a collection directory and index all matching files
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

    // Walk the directory tree
    let mut found_paths: Vec<String> = Vec::new();

    for entry in WalkDir::new(base_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let abs_path = entry.path();

        // Get relative path from collection root
        let rel_path = match abs_path.strip_prefix(base_path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // Check if file matches glob pattern
        if !pattern.matches(&rel_path) {
            continue;
        }

        // Skip hidden files and common ignore patterns
        if should_ignore(&rel_path) {
            continue;
        }

        result.files_found += 1;
        found_paths.push(rel_path.clone());

        // Process the file
        match index_file(conn, collection_id, base_path, &rel_path) {
            Ok(IndexStatus::Indexed) => result.files_indexed += 1,
            Ok(IndexStatus::Unchanged) => result.files_unchanged += 1,
            Err(e) => {
                let msg = format!("Error indexing {}: {}", rel_path, e);
                warn!("{}", msg);
                result.errors.push(msg);
            }
        }
    }

    // Mark documents not found on disk as inactive
    result.files_removed = mark_missing_documents(conn, collection_id, &found_paths)
        .unwrap_or(0);

    info!(
        "Scan complete for '{}': {} found, {} indexed, {} unchanged, {} removed",
        collection_name, result.files_found, result.files_indexed,
        result.files_unchanged, result.files_removed
    );

    Ok(result)
}

enum IndexStatus {
    Indexed,
    Unchanged,
}

/// Index a single file: read content, hash, chunk, store
fn index_file(
    conn: &Connection,
    collection_id: i64,
    base_path: &Path,
    rel_path: &str,
) -> Result<IndexStatus, String> {
    let abs_path = base_path.join(rel_path);
    let abs_path_str = abs_path.to_string_lossy().to_string();

    let file_ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt")
        .to_lowercase();

    // Read file content intelligently based on type
    let content = match file_ext.as_str() {
        "pdf" => {
            pdf_extract::extract_text(&abs_path).unwrap_or_default()
        },
        "docx" => {
            let file = fs::File::open(&abs_path).map_err(|e| format!("Cannot read docx: {}", e))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Cannot open zip: {}", e))?;
            let mut text = String::new();
            if let Ok(mut doc) = archive.by_name("word/document.xml") {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut doc, &mut xml).unwrap_or_default();
                let mut in_tag = false;
                for c in xml.chars() {
                    match c {
                        '<' => in_tag = true,
                        '>' => { in_tag = false; text.push(' '); },
                        _ if !in_tag => text.push(c),
                        _ => {}
                    }
                }
            }
            text
        },
        _ => {
            fs::read_to_string(&abs_path)
                .unwrap_or_else(|_| "".to_string())
        }
    };

    if content.trim().is_empty() {
        debug!("Skipping empty or unreadable file: {}", rel_path);
        return Ok(IndexStatus::Unchanged);
    }

    // Calculate content hash
    let hash = blake3::hash(content.as_bytes()).to_hex().to_string();

    // Check if file has changed
    let existing_hash = db::get_document_hash(conn, collection_id, rel_path)
        .map_err(|e| format!("DB error: {}", e))?;

    if let Some(ref existing) = existing_hash {
        if existing == &hash {
            debug!("File unchanged: {}", rel_path);
            return Ok(IndexStatus::Unchanged);
        }
    }

    // File is new or changed — index it
    let metadata = fs::metadata(&abs_path)
        .map_err(|e| format!("Cannot read metadata: {}", e))?;

    let file_size = metadata.len() as i64;
    let file_type = file_ext;

    let last_modified = metadata
        .modified()
        .ok()
        .map(|t| {
            let datetime: chrono::DateTime<chrono::Utc> = t.into();
            datetime.to_rfc3339()
        });

    let title = extract_title(&content);

    // Upsert document record
    let doc_id = db::upsert_document(
        conn,
        collection_id,
        rel_path,
        &abs_path_str,
        title.as_deref(),
        &hash,
        file_size,
        &file_type,
        last_modified.as_deref(),
    )
    .map_err(|e| format!("DB upsert error: {}", e))?;

    // Delete old chunks and re-chunk
    db::delete_chunks_for_document(conn, doc_id)
        .map_err(|e| format!("DB delete chunks error: {}", e))?;

    // Use fixed-size chunking as initial pass
    // (Semantic chunking requires embeddings, done in a separate step)
    let config = ChunkerConfig::default();
    let chunks = chunker::fixed_size_chunk(&content, &config);

    for chunk in &chunks {
        db::insert_chunk(
            conn,
            doc_id,
            chunk.chunk_index as i32,
            &chunk.content,
            Some(chunk.start_line as i32),
            Some(chunk.end_line as i32),
            Some(chunk.estimated_tokens as i32),
        )
        .map_err(|e| format!("DB insert chunk error: {}", e))?;
    }

    debug!("Indexed: {} ({} chunks)", rel_path, chunks.len());
    Ok(IndexStatus::Indexed)
}

/// Mark documents that no longer exist on disk as inactive
fn mark_missing_documents(
    conn: &Connection,
    collection_id: i64,
    found_paths: &[String],
) -> Result<usize, String> {
    // Get all active document paths for this collection
    let mut stmt = conn
        .prepare("SELECT id, path FROM documents WHERE collection_id = ?1 AND is_active = 1")
        .map_err(|e| format!("DB error: {}", e))?;

    let db_docs: Vec<(i64, String)> = stmt
        .query_map(rusqlite::params![collection_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("DB error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    let mut removed = 0;
    for (doc_id, path) in &db_docs {
        if !found_paths.contains(path) {
            conn.execute(
                "UPDATE documents SET is_active = 0 WHERE id = ?1",
                rusqlite::params![doc_id],
            )
            .map_err(|e| format!("DB error: {}", e))?;

            // Also delete chunks for removed docs
            db::delete_chunks_for_document(conn, *doc_id)
                .map_err(|e| format!("DB error: {}", e))?;

            removed += 1;
        }
    }

    Ok(removed)
}

/// Check if a path should be ignored
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

    for pattern in &ignore_patterns {
        if rel_path.contains(pattern) || rel_path.starts_with(pattern) {
            return true;
        }
    }

    // Skip hidden files (starting with .)
    for component in rel_path.split('/') {
        if component.starts_with('.') && component != "." && component != ".." {
            return true;
        }
    }

    false
}

/// Index a single file by absolute path (used by file watcher)
pub fn index_single_file(
    conn: &Connection,
    abs_path: &Path,
) -> Result<bool, String> {
    // Find which collection this file belongs to
    let abs_str = abs_path.to_string_lossy().to_string();

    let mut stmt = conn
        .prepare("SELECT id, path, glob_pattern FROM collections")
        .map_err(|e| format!("DB error: {}", e))?;

    let collections: Vec<(i64, String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "**/*".to_string())))
        })
        .map_err(|e| format!("DB error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    for (col_id, col_path, glob_pat) in &collections {
        let base = Path::new(col_path);
        if let Ok(rel_path) = abs_path.strip_prefix(base) {
            let rel_str = rel_path.to_string_lossy().to_string();

            // Check glob match
            if let Ok(pattern) = Pattern::new(glob_pat) {
                if pattern.matches(&rel_str) && !should_ignore(&rel_str) {
                    match index_file(conn, *col_id, base, &rel_str) {
                        Ok(IndexStatus::Indexed) => return Ok(true),
                        Ok(IndexStatus::Unchanged) => return Ok(false),
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    Ok(false) // File doesn't belong to any collection
}

/// Forceably index a single manual file regardless of glob match rules
pub fn index_manual_file(
    conn: &Connection,
    abs_path: &Path,
    collection_id: i64,
) -> Result<bool, String> {
    if !abs_path.is_file() {
        return Err("Path is not a valid file".to_string());
    }

    let parent_dir = abs_path.parent().unwrap_or(Path::new("/"));
    let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    match index_file(conn, collection_id, parent_dir, &file_name) {
        Ok(_) => Ok(true),
        Err(e) => Err(e),
    }
}
