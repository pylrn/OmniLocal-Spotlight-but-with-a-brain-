// SmartSearch — Database Layer
// Handles SQLite schema creation, migrations, and query helpers.

use log::info;
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::Path;
use keyring::Entry;

/// Initialize the database with the full schema
pub fn init_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;

    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    create_tables(&conn)?;
    migrate_schema(&conn)?;
    insert_default_settings(&conn)?;

    info!("Database initialized at {:?}", db_path);
    Ok(conn)
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
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

        CREATE TABLE IF NOT EXISTS chunks (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id       INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            chunk_index       INTEGER NOT NULL,
            content           TEXT    NOT NULL,
            start_line        INTEGER,
            end_line          INTEGER,
            token_count       INTEGER,
            has_embedding     INTEGER DEFAULT 0,
            embedding_status  TEXT    DEFAULT 'pending',
            embedding_error   TEXT,
            updated_at        TEXT    DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);

        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id     INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
            provider     TEXT    NOT NULL,
            model        TEXT    NOT NULL,
            dimensions   INTEGER NOT NULL,
            embedding    BLOB    NOT NULL,
            created_at   TEXT    DEFAULT (datetime('now')),
            updated_at   TEXT    DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_provider ON chunk_embeddings(provider);
        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_model ON chunk_embeddings(model);

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            content,
            content='chunks',
            content_rowid='id',
            tokenize='porter unicode61'
        );

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

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
            embedding float[768]
        );
        ",
    )?;

    Ok(())
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "chunks", "embedding_status")? {
        conn.execute(
            "ALTER TABLE chunks ADD COLUMN embedding_status TEXT DEFAULT 'pending'",
            [],
        )?;
    }

    if !column_exists(conn, "chunks", "embedding_error")? {
        conn.execute("ALTER TABLE chunks ADD COLUMN embedding_error TEXT", [])?;
    }

    if !column_exists(conn, "chunks", "updated_at")? {
        conn.execute(
            "ALTER TABLE chunks ADD COLUMN updated_at TEXT DEFAULT NULL",
            [],
        )?;
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id     INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
            provider     TEXT    NOT NULL,
            model        TEXT    NOT NULL,
            dimensions   INTEGER NOT NULL,
            embedding    BLOB    NOT NULL,
            created_at   TEXT    DEFAULT (datetime('now')),
            updated_at   TEXT    DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_embedding_status ON chunks(embedding_status);
        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_provider ON chunk_embeddings(provider);
        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_model ON chunk_embeddings(model);
        ",
    )?;

    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({})", table);
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;

    for name in rows {
        if name? == column {
            return Ok(true);
        }
    }

    Ok(false)
}

fn insert_default_settings(conn: &Connection) -> Result<()> {
    let defaults = [
        ("ai_provider", "ollama"),
        ("ollama_base_url", "http://localhost:11434"),
        ("lmstudio_base_url", "http://localhost:1234"),
        ("embed_model", "nomic-embed-text"),
        ("rerank_model", ""),
        ("embed_dimensions", "768"),
        ("embed_batch_size", "12"),
        ("active_context_enabled", "true"),
        ("watch_enabled", "true"),
        ("auto_embed_enabled", "true"),
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

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    if key == "gemini_api_key" {
        if let Ok(entry) = Entry::new("smart-search", "gemini_api_key") {
            if let Ok(password) = entry.get_password() {
                return Ok(Some(password));
            }
        }
    }

    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let result = stmt.query_row(params![key], |row| row.get(0));
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    if key == "gemini_api_key" {
        if let Ok(entry) = Entry::new("smart-search", "gemini_api_key") {
            let _ = entry.set_password(value);
        }
        // Also delete from SQLite if it exists
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        return Ok(());
    }

    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

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

pub fn list_collections(conn: &Connection) -> Result<Vec<CollectionRow>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.path, c.glob_pattern, c.context, c.created_at,
                (SELECT COUNT(*) FROM documents WHERE collection_id = c.id AND is_active = 1) as doc_count
         FROM collections c
         ORDER BY c.name",
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

pub fn remove_collection(conn: &Connection, name: &str) -> Result<bool> {
    let affected = conn.execute("DELETE FROM collections WHERE name = ?1", params![name])?;
    Ok(affected > 0)
}

pub fn list_documents(conn: &Connection) -> Result<Vec<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.collection_id, d.path, d.abs_path, d.title, d.file_type, d.file_size,
                c.name as collection_name, c.context as collection_context
         FROM documents d
         JOIN collections c ON c.id = d.collection_id
         WHERE d.is_active = 1
         ORDER BY d.last_indexed DESC",
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

    rows.collect()
}

pub fn list_document_statuses(conn: &Connection) -> Result<Vec<DocumentStatusRow>> {
    let mut stmt = conn.prepare(
        "SELECT
            d.id,
            d.title,
            d.path,
            d.abs_path,
            d.file_type,
            c.name,
            COUNT(ch.id) as chunk_count,
            SUM(CASE WHEN ch.embedding_status = 'embedded' THEN 1 ELSE 0 END) as embedded_chunks,
            SUM(CASE WHEN ch.embedding_status = 'pending' THEN 1 ELSE 0 END) as pending_chunks,
            SUM(CASE WHEN ch.embedding_status = 'failed' THEN 1 ELSE 0 END) as failed_chunks,
            MAX(d.last_indexed) as last_indexed,
            MAX(ch.embedding_error) as last_error
         FROM documents d
         JOIN collections c ON c.id = d.collection_id
         LEFT JOIN chunks ch ON ch.document_id = d.id
         WHERE d.is_active = 1
         GROUP BY d.id, d.title, d.path, d.abs_path, d.file_type, c.name
         ORDER BY d.last_indexed DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(DocumentStatusRow {
            document_id: row.get(0)?,
            title: row.get(1)?,
            path: row.get(2)?,
            abs_path: row.get(3)?,
            file_type: row.get(4)?,
            collection_name: row.get(5)?,
            chunk_count: row.get(6)?,
            embedded_chunk_count: row.get(7)?,
            pending_chunk_count: row.get(8)?,
            failed_chunk_count: row.get(9)?,
            last_indexed: row.get(10)?,
            last_error: row.get(11)?,
        })
    })?;

    rows.collect()
}

pub fn remove_document(conn: &Connection, document_id: i64) -> Result<()> {
    conn.execute("DELETE FROM documents WHERE id = ?1", params![document_id])?;
    Ok(())
}

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
        params![
            collection_id,
            path,
            abs_path,
            title,
            content_hash,
            file_size,
            file_type,
            last_modified
        ],
    )?;

    conn.query_row(
        "SELECT id FROM documents WHERE collection_id = ?1 AND path = ?2",
        params![collection_id, path],
        |row| row.get(0),
    )
}

pub fn get_document_hash(conn: &Connection, collection_id: i64, path: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT content_hash FROM documents WHERE collection_id = ?1 AND path = ?2 AND is_active = 1",
    )?;
    let result = stmt.query_row(params![collection_id, path], |row| row.get(0));
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn delete_chunks_for_document(conn: &Connection, document_id: i64) -> Result<()> {
    conn.execute("DELETE FROM chunks WHERE document_id = ?1", params![document_id])?;
    Ok(())
}

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
        "INSERT INTO chunks(document_id, chunk_index, content, start_line, end_line, token_count, has_embedding, embedding_status, embedding_error, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'pending', NULL, datetime('now'))",
        params![document_id, chunk_index, content, start_line, end_line, token_count],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn search_bm25(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, c.content, c.chunk_index, c.start_line, c.end_line, rank
         FROM chunks_fts
         JOIN chunks c ON c.id = chunks_fts.rowid
         JOIN documents d ON d.id = c.document_id
         WHERE chunks_fts MATCH ?1
           AND d.is_active = 1
         ORDER BY rank
         LIMIT ?2",
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
                1.0 / (1.0 + rank.abs())
            },
        })
    })?;

    rows.collect()
}

pub fn get_document_by_id(conn: &Connection, doc_id: i64) -> Result<Option<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.collection_id, d.path, d.abs_path, d.title, d.file_type, d.file_size,
                c.name as collection_name, c.context as collection_context
         FROM documents d
         JOIN collections c ON c.id = d.collection_id
         WHERE d.id = ?1 AND d.is_active = 1",
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

pub fn get_index_stats(conn: &Connection) -> Result<IndexStats> {
    let total_collections: i64 = conn.query_row("SELECT COUNT(*) FROM collections", [], |row| row.get(0))?;
    let total_documents: i64 =
        conn.query_row("SELECT COUNT(*) FROM documents WHERE is_active = 1", [], |row| row.get(0))?;
    let total_chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
    let embedded_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks WHERE embedding_status = 'embedded'",
        [],
        |row| row.get(0),
    )?;
    let pending_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks WHERE embedding_status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    let failed_chunks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunks WHERE embedding_status = 'failed'",
        [],
        |row| row.get(0),
    )?;

    Ok(IndexStats {
        total_collections,
        total_documents,
        total_chunks,
        embedded_chunks,
        pending_chunks,
        failed_chunks,
    })
}

pub fn get_index_overview(
    conn: &Connection,
    db_path: &str,
    provider_status: ProviderHealth,
    watcher_enabled: bool,
) -> Result<IndexOverview> {
    let stats = get_index_stats(conn)?;
    let last_indexed_at: Option<String> =
        conn.query_row("SELECT MAX(last_indexed) FROM documents", [], |row| row.get(0))?;

    Ok(IndexOverview {
        db_path: db_path.to_string(),
        total_collections: stats.total_collections,
        total_documents: stats.total_documents,
        total_chunks: stats.total_chunks,
        embedded_chunks: stats.embedded_chunks,
        pending_chunks: stats.pending_chunks,
        failed_chunks: stats.failed_chunks,
        watcher_enabled,
        provider_status,
        last_indexed_at,
    })
}

pub fn list_pending_chunks(conn: &Connection, limit: usize) -> Result<Vec<PendingChunk>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, c.content, d.title, d.path, d.abs_path
         FROM chunks c
         JOIN documents d ON d.id = c.document_id
         WHERE d.is_active = 1
           AND c.embedding_status = 'pending'
         ORDER BY c.updated_at ASC, c.id ASC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(PendingChunk {
            chunk_id: row.get(0)?,
            document_id: row.get(1)?,
            content: row.get(2)?,
            document_title: row.get(3)?,
            path: row.get(4)?,
            abs_path: row.get(5)?,
        })
    })?;

    rows.collect()
}

pub fn mark_chunk_embedding_in_progress(conn: &Connection, chunk_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE chunks
         SET embedding_status = 'processing', embedding_error = NULL, updated_at = datetime('now')
         WHERE id = ?1",
        params![chunk_id],
    )?;
    Ok(())
}

pub fn mark_chunk_embedding_pending(conn: &Connection, chunk_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE chunks
         SET has_embedding = 0, embedding_status = 'pending', embedding_error = NULL, updated_at = datetime('now')
         WHERE id = ?1",
        params![chunk_id],
    )?;
    Ok(())
}

pub fn mark_chunk_embedding_success(
    conn: &Connection,
    chunk_id: i64,
    provider: &str,
    model: &str,
    embedding: &[f32],
) -> Result<()> {
    let mut embedding_blob = Vec::with_capacity(embedding.len() * 4);
    for &f in embedding {
        embedding_blob.extend_from_slice(&f.to_le_bytes());
    }
    let dimensions = embedding.len() as i64;

    conn.execute(
        "INSERT INTO chunk_embeddings(chunk_id, provider, model, dimensions, embedding, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))
         ON CONFLICT(chunk_id) DO UPDATE SET
            provider = excluded.provider,
            model = excluded.model,
            dimensions = excluded.dimensions,
            embedding = excluded.embedding,
            updated_at = datetime('now')",
        params![chunk_id, provider, model, dimensions, embedding_blob],
    )?;

    conn.execute(
        "UPDATE chunks
         SET has_embedding = 1, embedding_status = 'embedded', embedding_error = NULL, updated_at = datetime('now')
         WHERE id = ?1",
         params![chunk_id],
    )?;

    // Also insert into sqlite-vec virtual table
    use zerocopy::IntoBytes;
    conn.execute(
        "INSERT INTO chunks_vec(rowid, embedding) VALUES (?1, ?2)
         ON CONFLICT(rowid) DO UPDATE SET embedding = excluded.embedding",
        params![chunk_id, embedding.as_bytes()],
    )?;

    Ok(())
}

pub fn mark_chunk_embedding_failed(conn: &Connection, chunk_id: i64, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE chunks
         SET has_embedding = 0, embedding_status = 'failed', embedding_error = ?2, updated_at = datetime('now')
         WHERE id = ?1",
        params![chunk_id, error],
    )?;
    Ok(())
}

pub fn retry_failed_chunks(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE chunks
         SET embedding_status = 'pending', embedding_error = NULL, updated_at = datetime('now')
         WHERE embedding_status = 'failed'",
        [],
    )
}

pub fn retry_failed_chunks_for_document(conn: &Connection, document_id: i64) -> Result<usize> {
    conn.execute(
        "UPDATE chunks
         SET has_embedding = 0, embedding_status = 'pending', embedding_error = NULL, updated_at = datetime('now')
         WHERE document_id = ?1
           AND embedding_status = 'failed'",
        params![document_id],
    )
}

pub fn mark_all_chunks_pending(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE chunks
         SET has_embedding = 0, embedding_status = 'pending', embedding_error = NULL, updated_at = datetime('now')",
        [],
    )
}


pub fn get_chunks_for_document(conn: &Connection, document_id: i64) -> Result<Vec<ChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, chunk_index, start_line, end_line, token_count, embedding_status
         FROM chunks
         WHERE document_id = ?1
         ORDER BY chunk_index ASC",
    )?;

    let rows = stmt.query_map(params![document_id], |row| {
        Ok(ChunkRow {
            id: row.get(0)?,
            content: row.get(1)?,
            chunk_index: row.get(2)?,
            start_line: row.get(3)?,
            end_line: row.get(4)?,
            token_count: row.get(5)?,
            embedding_status: row.get(6)?,
        })
    })?;

    rows.collect()
}

pub fn get_chunk_embedding_info(conn: &Connection, chunk_id: i64) -> Result<Option<ChunkEmbeddingInfo>> {
    conn.query_row(
        "SELECT provider, model, dimensions, updated_at
         FROM chunk_embeddings
         WHERE chunk_id = ?1",
        params![chunk_id],
        |row| {
            Ok(ChunkEmbeddingInfo {
                provider: row.get(0)?,
                model: row.get(1)?,
                dimensions: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
}

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
pub struct DocumentStatusRow {
    pub document_id: i64,
    pub title: Option<String>,
    pub path: String,
    pub abs_path: String,
    pub file_type: String,
    pub collection_name: String,
    pub chunk_count: i64,
    pub embedded_chunk_count: i64,
    pub pending_chunk_count: i64,
    pub failed_chunk_count: i64,
    pub last_indexed: Option<String>,
    pub last_error: Option<String>,
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
    pub pending_chunks: i64,
    pub failed_chunks: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderHealth {
    pub provider: String,
    pub model_name: String,
    pub connected: bool,
    pub model_available: bool,
    pub dimensions: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexOverview {
    pub db_path: String,
    pub total_collections: i64,
    pub total_documents: i64,
    pub total_chunks: i64,
    pub embedded_chunks: i64,
    pub pending_chunks: i64,
    pub failed_chunks: i64,
    pub watcher_enabled: bool,
    pub provider_status: ProviderHealth,
    pub last_indexed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingChunk {
    pub chunk_id: i64,
    pub document_id: i64,
    pub content: String,
    pub document_title: Option<String>,
    pub path: String,
    pub abs_path: String,
}


#[derive(Debug, Clone, serde::Serialize)]
pub struct ChunkRow {
    pub id: i64,
    pub content: String,
    pub chunk_index: i32,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub token_count: Option<i32>,
    pub embedding_status: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChunkEmbeddingInfo {
    pub provider: String,
    pub model: String,
    pub dimensions: i64,
    pub updated_at: Option<String>,
}
