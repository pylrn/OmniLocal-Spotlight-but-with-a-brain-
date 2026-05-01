// SmartSearch — Model Context Protocol (MCP) Headless Server
// Allows other agents (like Claude Desktop or Cursor) to query local semantic search via stdio JSON-RPC.

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::db;
use crate::core::search;
use crate::core::ai::{AiProvider, EmbeddingProvider};

// Minimal MCP JSON-RPC schemas
#[derive(Deserialize)]
struct RpcRequest {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

pub async fn run_mcp_server() {
    // Resolve the DB path dynamically for the headless engine
    #[cfg(debug_assertions)]
    let app_data = std::env::current_dir().unwrap().join(".data");
    #[cfg(not(debug_assertions))]
    let app_data = directories::ProjectDirs::from("com", "smartsearch", "SmartSearch")
        .expect("Failed to get app data directory")
        .data_dir()
        .to_path_buf();

    let db_path = app_data.join("smartsearch.db");
    let conn = db::init_db(&db_path).expect("Failed to initialize database for MCP Server");

    // Initialize MCP over stdio
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin);
    let mut stdout = io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                if line.trim().is_empty() {
                    continue;
                }

                let req: RpcRequest = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue, // Ignore invalid JSON
                };

                if let Some(id) = req.id {
                    let res = handle_method(&req.method, req.params, &conn).await;
                    let (result, error) = match res {
                        Ok(value) => (Some(value), None),
                        Err(err) => (None, Some(err)),
                    };
                    let response = RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result,
                        error,
                    };

                    let out = serde_json::to_string(&response).unwrap();
                    stdout.write_all(format!("{}\n", out).as_bytes()).await.unwrap();
                    stdout.flush().await.unwrap();
                }
            }
            Err(_) => break,
        }
    }
}

async fn handle_method(method: &str, params: Option<Value>, conn: &rusqlite::Connection) -> Result<Value, RpcError> {
    match method {
        "initialize" => {
            // Standard MCP initialization
            Ok(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "SmartSearch MCP Server",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "tools": {}
                }
            }))
        }
        "tools/list" => {
            Ok(serde_json::json!({
                "tools": [
                    {
                        "name": "search_local_docs",
                        "description": "Performs a semantic vector search across the user's local documents, notes, and code.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {
                                    "type": "string",
                                    "description": "The search query or concept to find"
                                },
                                "limit": {
                                    "type": "number",
                                    "description": "Maximum number of chunks to return (default: 10)"
                                }
                            },
                            "required": ["query"]
                        }
                    }
                ]
            }))
        }
        "tools/call" => {
            let p = params.unwrap_or_default();
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            
            if name == "search_local_docs" {
                let args = p.get("arguments").unwrap_or(&Value::Null);
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                if query.is_empty() {
                    return Err(RpcError {
                        code: -32602,
                        message: "Invalid arguments: query is required".to_string(),
                    });
                }

                // Execute local search bypassing Tauri frontend
                let provider_type = db::get_setting(conn, "ai_provider").unwrap_or_default().unwrap_or_else(|| "ollama".to_string());
                let ollama_url = db::get_setting(conn, "ollama_base_url").unwrap_or_default().unwrap_or_else(|| "http://localhost:11434".to_string());
                let lmstudio_url = db::get_setting(conn, "lmstudio_base_url").unwrap_or_default().unwrap_or_else(|| "http://localhost:1234".to_string());
                let gemini_api_key = db::get_setting(conn, "gemini_api_key").unwrap_or_default().unwrap_or_default();
                let embed_model = db::get_setting(conn, "embed_model").unwrap_or_default().unwrap_or_else(|| "nomic-embed-text".to_string());

                let provider = AiProvider::from_settings(
                    &provider_type,
                    &ollama_url,
                    &lmstudio_url,
                    &gemini_api_key,
                    &embed_model,
                );

                let query_embedding = provider.embed_query(query.to_string()).await.ok();
                let results = search::hybrid_search(conn, query, query_embedding.as_deref(), limit).unwrap_or_default();
                
                // Format for AI consumption
                let mut content = String::new();
                for r in results {
                    content.push_str(&format!(
                        "--- File: {} (Score: {})\nSnippet (Lines {}-{}):\n{}\n\n",
                        r.abs_path,
                        r.score,
                        r.start_line.unwrap_or(0),
                        r.end_line.unwrap_or(0),
                        r.snippet
                    ));
                }

                if content.is_empty() {
                    content = "No local documents completely matched your query. Try a different search.".to_string();
                }

                Ok(serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": content
                        }
                    ]
                }))
            } else {
                Err(RpcError {
                    code: -32601,
                    message: "Tool not found".to_string(),
                })
            }
        }
        _ => Err(RpcError {
            code: -32601,
            message: "Method not found".to_string(),
        })
    }
}
