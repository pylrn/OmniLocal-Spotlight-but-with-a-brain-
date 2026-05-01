// SmartSearch — Typed IPC Wrappers

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface Collection {
  id: number;
  name: string;
  path: string;
  glob_pattern: string | null;
  context: string | null;
  created_at: string | null;
  doc_count: number;
}

export interface DocumentRow {
  id: number;
  collection_id: number;
  path: string;
  abs_path: string;
  title: string | null;
  file_type: string;
  file_size: number;
  collection_name: string;
}

export interface DocumentStatusRow {
  document_id: number;
  title: string | null;
  path: string;
  abs_path: string;
  file_type: string;
  collection_name: string;
  chunk_count: number;
  embedded_chunk_count: number;
  pending_chunk_count: number;
  failed_chunk_count: number;
  last_indexed: string | null;
  last_error: string | null;
}

export interface SearchResult {
  document_id: number;
  chunk_id: number;
  title: string | null;
  path: string;
  abs_path: string;
  collection_name: string;
  collection_context: string | null;
  file_type: string;
  snippet: string;
  score: number;
  chunk_index: number;
  start_line: number | null;
  end_line: number | null;
  context_boosted: boolean;
}

export interface ScanResult {
  collection_name: string;
  files_found: number;
  files_indexed: number;
  files_unchanged: number;
  files_removed: number;
  errors: string[];
}

export interface IndexStats {
  total_collections: number;
  total_documents: number;
  total_chunks: number;
  embedded_chunks: number;
  pending_chunks: number;
  failed_chunks: number;
}

export interface ProviderStatus {
  connected: boolean;
  provider: 'Ollama' | 'LMStudio' | 'Gemini';
  model_available: boolean;
  model_name: string;
  dimensions: number | null;
  error: string | null;
}

export interface ProviderHealth {
  provider: string;
  model_name: string;
  connected: boolean;
  model_available: boolean;
  dimensions: number | null;
  error: string | null;
}

export interface IndexOverview {
  db_path: string;
  total_collections: number;
  total_documents: number;
  total_chunks: number;
  embedded_chunks: number;
  pending_chunks: number;
  failed_chunks: number;
  watcher_enabled: boolean;
  provider_status: ProviderHealth;
  last_indexed_at: string | null;
}

export interface EmbeddingRuntimeStatus {
  phase: string;
  current_title: string | null;
  current_path: string | null;
  provider: string | null;
  model: string | null;
  message: string;
}

export interface EmbeddingProgressEvent {
  runtime: EmbeddingRuntimeStatus;
  stats: IndexStats;
}

export async function addCollection(
  name: string,
  path: string,
  globPattern?: string,
  context?: string,
): Promise<number> {
  return invoke('add_collection', {
    name,
    path,
    globPattern: globPattern || null,
    context: context || null,
  });
}

export async function listCollections(): Promise<Collection[]> {
  return invoke('list_collections');
}

export async function removeCollection(name: string): Promise<boolean> {
  return invoke('remove_collection', { name });
}

export async function addManualFiles(paths: string[]): Promise<void> {
  return invoke('add_manual_files', { paths });
}

export async function listIndexedFiles(): Promise<DocumentRow[]> {
  return invoke('list_indexed_files');
}

export async function listDocumentStatuses(): Promise<DocumentStatusRow[]> {
  return invoke('list_document_statuses');
}

export async function removeManualFile(documentId: number): Promise<void> {
  return invoke('remove_manual_file', { documentId });
}

export async function scanCollections(): Promise<ScanResult[]> {
  return invoke('scan_collections');
}

export async function searchKeyword(query: string, limit?: number): Promise<SearchResult[]> {
  return invoke('search_keyword', { query, limit: limit || null });
}

export async function getIndexStats(): Promise<IndexStats> {
  return invoke('get_index_stats');
}

export async function getIndexOverview(): Promise<IndexOverview> {
  return invoke('get_index_overview');
}

export async function getEmbeddingRuntime(): Promise<EmbeddingRuntimeStatus> {
  return invoke('get_embedding_runtime');
}

export async function checkAiStatus(): Promise<ProviderStatus> {
  return invoke('check_ai_status');
}

export async function testAiProvider(): Promise<ProviderStatus> {
  return invoke('test_ai_provider');
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke('get_setting', { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  return invoke('set_setting', { key, value });
}

export async function retryFailedEmbeddings(): Promise<void> {
  return invoke('retry_failed_embeddings');
}

export async function retryDocumentEmbeddings(documentId: number): Promise<void> {
  return invoke('retry_document_embeddings', { documentId });
}

export async function reembedAll(): Promise<void> {
  return invoke('reembed_all');
}

export async function getForegroundApp(): Promise<string | null> {
  return invoke('get_foreground_app');
}

export interface ContextSnippet {
  snippet: string;
  path: string;
}

export async function queryWithContext(
  query: string,
  snippets: ContextSnippet[],
): Promise<string> {
  return invoke('query_with_context', { query, snippets });
}

export function listenToEmbeddingProgress(
  handler: (payload: EmbeddingProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<EmbeddingProgressEvent>('embedding-progress', (event) => handler(event.payload));
}

export function listenToIndexProgress(
  handler: (payload: unknown) => void,
): Promise<UnlistenFn> {
  return listen('index-progress', (event) => handler(event.payload));
}

export function getFileTypeColor(fileType: string): string {
  const colors: Record<string, string> = {
    md: 'var(--filetype-md)',
    markdown: 'var(--filetype-md)',
    ts: 'var(--filetype-ts)',
    tsx: 'var(--filetype-ts)',
    js: 'var(--filetype-js)',
    jsx: 'var(--filetype-js)',
    py: 'var(--filetype-py)',
    rs: 'var(--filetype-rs)',
    go: 'var(--filetype-go)',
    pdf: 'var(--filetype-pdf)',
    docx: 'var(--filetype-docx)',
  };
  return colors[fileType] || 'var(--filetype-default)';
}

export function getFileTypeIcon(fileType: string): string {
  const icons: Record<string, string> = {
    md: 'Document',
    markdown: 'Document',
    ts: 'Code',
    tsx: 'Code',
    js: 'Code',
    jsx: 'Code',
    py: 'Python',
    rs: 'Rust',
    go: 'Go',
    txt: 'Text',
    json: 'JSON',
    yaml: 'YAML',
    yml: 'YAML',
    html: 'HTML',
    css: 'CSS',
    pdf: 'PDF',
    docx: 'DOCX',
  };
  return icons[fileType] || 'File';
}

export function formatScore(score: number): string {
  return `${Math.round(score * 100)}%`;
}

export function getScoreColor(score: number): string {
  if (score >= 0.7) return 'var(--score-high)';
  if (score >= 0.4) return 'var(--score-medium)';
  return 'var(--score-low)';
}

export function formatRelativeDate(value: string | null): string {
  if (!value) return 'Never';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;

  const diffMs = Date.now() - date.getTime();
  const diffMinutes = Math.round(diffMs / 60000);
  if (diffMinutes < 1) return 'Just now';
  if (diffMinutes < 60) return `${diffMinutes}m ago`;

  const diffHours = Math.round(diffMinutes / 60);
  if (diffHours < 24) return `${diffHours}h ago`;

  const diffDays = Math.round(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString();
}
