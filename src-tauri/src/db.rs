// SmartSearch — Database Layer
// Handles SQLite schema creation, migrations, and query helpers

use rusqlite::{Connection, Result, params};
use std::path::Path;
use log::info;

/// Initialize the database with the full schema
pub fn init_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;

    // Enable WAL mode for better concurrent read performance
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    create_tables(&conn)?;
    insert_default_settings(&conn)?;

    info!("Database initialized at {:?}", db_path);
    Ok(conn)
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- ═══════════════════════════════════════════════════════════════
        -- COLLECTIONS
        -- ═══════════════════════════════════════════════════════════════
        CREATE TABLE IF NOT EXISTS collections (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT    NOT NULL UNIQUE,
            path            TEXT    NOT NULL,
            glob_pattern    TEXT    DEFAULT '**/*.md',
            ignore_patterns TEXT,
            context         TEXT,
            created_at      TEXT    DEFAULT (datetime('now')),
            updated_at      TEXT    DEFAULT (datetime('now'))
        );

        -- ═══════════════════════════════════════════════════════════════
        -- DOCUMENTS
        -- ═══════════════════════════════════════════════════════════════
        CREATE TABLE IF NOT EXISTS documents (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id   INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            path            TEXT    NOT NULL,
            abs_path        TEXT    NOT NULL,
            title           TEXT,
            content_hash    TEXT    NOT NULL,
            file_size       INTEGER NOT NULL,
            file_type       TEXT    NOT NULL,
            last_indexed    TEXT    DEFAULT (datetime('now')),
            last_modified   TEXT,
            is_active       INTEGER DEFAULT 1,
            UNIQUE(collection_id, path)
        );

        CREATE INDEX IF NOT EXISTS idx_documents_hash ON documents(content_hash);
        CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(file_type);
        CREATE INDEX IF NOT EXISTS idx_documents_active ON documents(is_active);

        -- ═══════════════════════════════════════════════════════════════
        -- CHUNKS (semantic boundary chunks)
        -- ═══════════════════════════════════════════════════════════════
        CREATE TABLE IF NOT EXISTS chunks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id     INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            chunk_index     INTEGER NOT NULL,
            content         TEXT    NOT NULL,
            start_line      INTEGER,
            end_line        INTEGER,
            token_count     INTEGER,
            has_embedding   INTEGER DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);

        -- ═══════════════════════════════════════════════════════════════
        -- FTS5 (BM25 keyword search)
        -- ═══════════════════════════════════════════════════════════════
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            content,
            content='chunks',
            content_rowid='id',
            tokenize='porter unicode61'
        );

        -- Triggers to keep FTS in sync
        CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content)
                VALUES('delete', old.id, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, content)
                VALUES('delete', old.id, old.content);
            INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
        END;

        -- ═══════════════════════════════════════════════════════════════
        -- SETTINGS
        -- ═══════════════════════════════════════════════════════════════
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "
    )?;

    Ok(())
}

fn insert_default_settings(conn: &Connection) -> Result<()> {
    let defaults = vec![
        ("ai_provider", "ollama"),
        ("ollama_base_url", "http://localhost:11434"),
        ("lmstudio_base_url", "http://localhost:1234"),
        ("embed_model", "nomic-embed-text"),
        ("rerank_model", ""),
        ("embed_dimensions", "768"),
        ("active_context_enabled", "true"),
        ("watch_enabled", "true"),
        ("theme", "dark"),
    ];

    for (key, value) in defaults {
        conn.execute(
            "INSERT OR IGNORE INTO settings(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Query helpers
// ═══════════════════════════════════════════════════════════════════════

/// Get a setting value by key
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let result = stmt.query_row(params![key], |row| row.get(0));
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Set a setting value
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Insert a new collection, returns its ID
pub fn insert_collection(
    conn: &Connection,
    name: &str,
    path: &str,
    glob_pattern: &str,
    context: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO collections(name, path, glob_pattern, context)
         VALUES (?1, ?2, ?3, ?4)",
        params![name, path, glob_pattern, context],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all collections
pub fn list_collections(conn: &Connection) -> Result<Vec<CollectionRow>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.path, c.glob_pattern, c.context, c.created_at,
                (SELECT COUNT(*) FROM documents WHERE collection_id = c.id AND is_active = 1) as doc_count
         FROM collections c
         ORDER BY c.name"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(CollectionRow {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            glob_pattern: row.get(3)?,
            context: row.get(4)?,
            created_at: row.get(5)?,
            doc_count: row.get(6)?,
        })
    })?;

    rows.collect()
}

/// Remove a collection by name
pub fn remove_collection(conn: &Connection, name: &str) -> Result<bool> {
    let affected = conn.execute("DELETE FROM collections WHERE name = ?1", params![name])?;
    Ok(affected > 0)
}

/// List all tracked documents
pub fn list_documents(conn: &Connection) -> Result<Vec<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.collection_id, d.path, d.abs_path, d.title, d.file_type, d.file_size,
                c.name as collection_name, c.context as collection_context
         FROM documents d
         JOIN collections c ON c.id = d.collection_id
         WHERE d.is_active = 1
         ORDER BY d.last_indexed DESC"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(DocumentRow {
            id: row.get(0)?,
            collection_id: row.get(1)?,
            path: row.get(2)?,
            abs_path: row.get(3)?,
            title: row.get(4)?,
            file_type: row.get(5)?,
            file_size: row.get(6)?,
            collection_name: row.get(7)?,
            collection_context: row.get(8)?,
        })
    })?;

    let mut docs = Vec::new();
    for r in rows {
        if let Ok(doc) = r {
            docs.push(doc);
        }
    }
    Ok(docs)
}

/// Remove isolated document and trigger cascade deletions
pub fn remove_document(conn: &Connection, document_id: i64) -> Result<()> {
    conn.execute("DELETE FROM documents WHERE id = ?1", params![document_id])?;
    Ok(())
}

/// Insert or update a document, returns document ID
pub fn upsert_document(
    conn: &Connection,
    collection_id: i64,
    path: &str,
    abs_path: &str,
    title: Option<&str>,
    content_hash: &str,
    file_size: i64,
    file_type: &str,
    last_modified: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO documents(collection_id, path, abs_path, title, content_hash, file_size, file_type, last_modified)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(collection_id, path) DO UPDATE SET
            abs_path = excluded.abs_path,
            title = excluded.title,
            content_hash = excluded.content_hash,
            file_size = excluded.file_size,
            file_type = excluded.file_type,
            last_modified = excluded.last_modified,
            last_indexed = datetime('now'),
            is_active = 1",
        params![collection_id, path, abs_path, title, content_hash, file_size, file_type, last_modified],
    )?;

    // Get the document ID (whether inserted or updated)
    let doc_id: i64 = conn.query_row(
        "SELECT id FROM documents WHERE collection_id = ?1 AND path = ?2",
        params![collection_id, path],
        |row| row.get(0),
    )?;

    Ok(doc_id)
}

/// Get the content hash of a document
pub fn get_document_hash(conn: &Connection, collection_id: i64, path: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash FROM documents WHERE collection_id = ?1 AND path = ?2 AND is_active = 1"
    )?;
    let result = stmt.query_row(params![collection_id, path], |row| row.get(0));
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Delete all chunks for a document
pub fn delete_chunks_for_document(conn: &Connection, document_id: i64) -> Result<()> {
    conn.execute("DELETE FROM chunks WHERE document_id = ?1", params![document_id])?;
    Ok(())
}

/// Insert a chunk
pub fn insert_chunk(
    conn: &Connection,
    document_id: i64,
    chunk_index: i32,
    content: &str,
    start_line: Option<i32>,
    end_line: Option<i32>,
    token_count: Option<i32>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO chunks(document_id, chunk_index, content, start_line, end_line, token_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![document_id, chunk_index, content, start_line, end_line, token_count],
    )?;
    Ok(conn.last_insert_rowid())
}

/// BM25 search via FTS5
pub fn search_bm25(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, c.content, c.chunk_index, c.start_line, c.end_line,
                rank
         FROM chunks_fts
         JOIN chunks c ON c.id = chunks_fts.rowid
         JOIN documents d ON d.id = c.document_id
         WHERE chunks_fts MATCH ?1
           AND d.is_active = 1
         ORDER BY rank
         LIMIT ?2"
    )?;

    let rows = stmt.query_map(params![query, limit as i64], |row| {
        Ok(SearchHit {
            chunk_id: row.get(0)?,
            document_id: row.get(1)?,
            content: row.get(2)?,
            chunk_index: row.get(3)?,
            start_line: row.get(4)?,
            end_line: row.get(5)?,
            score: {
                let rank: f64 = row.get(6)?;
                // FTS5 rank is negative (lower = better), normalize to 0..1
                1.0 / (1.0 + rank.abs())
            },
        })
    })?;

    rows.collect()
}

/// Get document info by ID
pub fn get_document_by_id(conn: &Connection, doc_id: i64) -> Result<Option<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.collection_id, d.path, d.abs_path, d.title, d.file_type, d.file_size,
                c.name as collection_name, c.context as collection_context
         FROM documents d
         JOIN collections c ON c.id = d.collection_id
         WHERE d.id = ?1 AND d.is_active = 1"
    )?;

    let result = stmt.query_row(params![doc_id], |row| {
        Ok(DocumentRow {
            id: row.get(0)?,
            collection_id: row.get(1)?,
            path: row.get(2)?,
            abs_path: row.get(3)?,
            title: row.get(4)?,
            file_type: row.get(5)?,
            file_size: row.get(6)?,
            collection_name: row.get(7)?,
            collection_context: row.get(8)?,
        })
    });

    match result {
        Ok(doc) => Ok(Some(doc)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Get index statistics
pub fn get_index_stats(conn: &Connection) -> Result<IndexStats> {
    let total_collections: i64 = conn.query_row(
        "SELECT COUNT(*) FROM collections", [], |row| row.get(0)
    )?;
    let total_documents: i64 = conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE is_active = 1", [], |row| row.get(0)
    )?;
    let total_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks", [], |row| row.get(0)
    )?;
    let embedded_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks WHERE has_embedding = 1", [], |row| row.get(0)
    )?;

    Ok(IndexStats {
        total_collections,
        total_documents,
        total_chunks,
        embedded_chunks,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// Data types
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub glob_pattern: Option<String>,
    pub context: Option<String>,
    pub created_at: Option<String>,
    pub doc_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentRow {
    pub id: i64,
    pub collection_id: i64,
    pub path: String,
    pub abs_path: String,
    pub title: Option<String>,
    pub file_type: String,
    pub file_size: i64,
    pub collection_name: String,
    pub collection_context: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub chunk_id: i64,
    pub document_id: i64,
    pub content: String,
    pub chunk_index: i32,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStats {
    pub total_collections: i64,
    pub total_documents: i64,
    pub total_chunks: i64,
    pub embedded_chunks: i64,
}
