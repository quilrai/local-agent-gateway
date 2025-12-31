// Database operations and schema management

use crate::dlp::DlpDetection;
use crate::dlp_pattern_config::{DB_PATH, DEFAULT_PORT};
use crate::requestresponsemetadata::{RequestMetadata, ResponseMetadata};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Builtin pattern definition from JSON
#[derive(Deserialize)]
struct BuiltinPattern {
    name: String,
    pattern_type: String,
    patterns: Vec<String>,
    negative_pattern_type: Option<String>,
    negative_patterns: Option<Vec<String>>,
    min_occurrences: i32,
    min_unique_chars: i32,
}

/// Embedded builtin patterns JSON
const BUILTIN_PATTERNS_JSON: &str = include_str!("../builtin_patterns.json");

/// Thread-safe database wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        // Create requests table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                backend TEXT NOT NULL DEFAULT 'claude',
                endpoint_name TEXT NOT NULL,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                model TEXT,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                cache_creation_tokens INTEGER DEFAULT 0,
                latency_ms INTEGER DEFAULT 0,
                has_system_prompt INTEGER DEFAULT 0,
                has_tools INTEGER DEFAULT 0,
                has_thinking INTEGER DEFAULT 0,
                stop_reason TEXT,
                user_message_count INTEGER DEFAULT 0,
                assistant_message_count INTEGER DEFAULT 0,
                response_status INTEGER,
                is_streaming INTEGER NOT NULL DEFAULT 0,
                request_body TEXT,
                response_body TEXT,
                extra_metadata TEXT,
                request_headers TEXT,
                response_headers TEXT
            )",
            [],
        )?;

        // Migration: Add extra_metadata column if it doesn't exist (for existing databases)
        let _ = conn.execute(
            "ALTER TABLE requests ADD COLUMN extra_metadata TEXT",
            [],
        );

        // Migration: Add request_headers column if it doesn't exist (for existing databases)
        let _ = conn.execute(
            "ALTER TABLE requests ADD COLUMN request_headers TEXT",
            [],
        );

        // Migration: Add response_headers column if it doesn't exist (for existing databases)
        let _ = conn.execute(
            "ALTER TABLE requests ADD COLUMN response_headers TEXT",
            [],
        );

        // Create settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Create DLP patterns table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dlp_patterns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                pattern_type TEXT NOT NULL,
                patterns TEXT NOT NULL,
                negative_pattern_type TEXT,
                negative_patterns TEXT,
                enabled INTEGER DEFAULT 1,
                min_occurrences INTEGER DEFAULT 1,
                min_unique_chars INTEGER DEFAULT 0,
                is_builtin INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        // Seed builtin patterns if not exists
        Self::seed_builtin_patterns(&conn)?;

        // Create DLP detections table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dlp_detections (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id INTEGER,
                timestamp TEXT NOT NULL,
                pattern_name TEXT NOT NULL,
                pattern_type TEXT NOT NULL,
                original_value TEXT NOT NULL,
                placeholder TEXT NOT NULL,
                message_index INTEGER,
                FOREIGN KEY (request_id) REFERENCES requests(id)
            )",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Seed builtin DLP patterns, overwriting if they already exist
    fn seed_builtin_patterns(conn: &Connection) -> Result<(), rusqlite::Error> {
        // Parse builtin patterns from embedded JSON
        let builtin_patterns: Vec<BuiltinPattern> =
            serde_json::from_str(BUILTIN_PATTERNS_JSON).unwrap_or_default();

        let created_at = chrono::Utc::now().to_rfc3339();

        for pattern in builtin_patterns {
            let patterns_json =
                serde_json::to_string(&pattern.patterns).unwrap_or_else(|_| "[]".to_string());
            let negative_patterns_json = pattern
                .negative_patterns
                .as_ref()
                .map(|np| serde_json::to_string(np).unwrap_or_else(|_| "[]".to_string()));

            // Check if this builtin pattern already exists
            let existing_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM dlp_patterns WHERE is_builtin = 1 AND name = ?1",
                    rusqlite::params![&pattern.name],
                    |row| row.get(0),
                )
                .ok();

            if let Some(id) = existing_id {
                // Update existing pattern (preserve enabled state)
                conn.execute(
                    "UPDATE dlp_patterns SET pattern_type = ?1, patterns = ?2, negative_pattern_type = ?3, negative_patterns = ?4, min_occurrences = ?5, min_unique_chars = ?6 WHERE id = ?7",
                    rusqlite::params![
                        pattern.pattern_type,
                        patterns_json,
                        pattern.negative_pattern_type,
                        negative_patterns_json,
                        pattern.min_occurrences,
                        pattern.min_unique_chars,
                        id
                    ],
                )?;
            } else {
                // Insert new pattern
                conn.execute(
                    "INSERT INTO dlp_patterns (name, pattern_type, patterns, negative_pattern_type, negative_patterns, enabled, min_occurrences, min_unique_chars, is_builtin, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, 1, ?8)",
                    rusqlite::params![
                        pattern.name,
                        pattern.pattern_type,
                        patterns_json,
                        pattern.negative_pattern_type,
                        negative_patterns_json,
                        pattern.min_occurrences,
                        pattern.min_unique_chars,
                        created_at
                    ],
                )?;
            }
        }

        Ok(())
    }

    /// Clean up data older than 7 days
    pub fn cleanup_old_data(&self) -> Result<usize, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
        let cutoff_ts = cutoff.to_rfc3339();

        conn.execute(
            "DELETE FROM requests WHERE timestamp < ?1",
            rusqlite::params![cutoff_ts],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn log_request(
        &self,
        backend: &str,
        method: &str,
        path: &str,
        endpoint_name: &str,
        request_body: &str,
        response_body: &str,
        response_status: u16,
        is_streaming: bool,
        latency_ms: u64,
        req_meta: &RequestMetadata,
        resp_meta: &ResponseMetadata,
        extra_metadata: Option<&str>,
        request_headers: Option<&str>,
        response_headers: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let timestamp = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO requests (
                timestamp, backend, endpoint_name, method, path, model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                latency_ms, has_system_prompt, has_tools, has_thinking, stop_reason,
                user_message_count, assistant_message_count,
                response_status, is_streaming, request_body, response_body, extra_metadata,
                request_headers, response_headers
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            rusqlite::params![
                timestamp,
                backend,
                endpoint_name,
                method,
                path,
                req_meta.model,
                resp_meta.input_tokens,
                resp_meta.output_tokens,
                resp_meta.cache_read_tokens,
                resp_meta.cache_creation_tokens,
                latency_ms as i64,
                req_meta.has_system_prompt as i32,
                req_meta.has_tools as i32,
                resp_meta.has_thinking as i32,
                resp_meta.stop_reason,
                req_meta.user_message_count,
                req_meta.assistant_message_count,
                response_status,
                is_streaming as i32,
                request_body,
                response_body,
                extra_metadata,
                request_headers,
                response_headers,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    pub fn log_dlp_detections(
        &self,
        request_id: i64,
        detections: &[DlpDetection],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let timestamp = chrono::Utc::now().to_rfc3339();

        for detection in detections {
            conn.execute(
                "INSERT INTO dlp_detections (request_id, timestamp, pattern_name, pattern_type, original_value, placeholder, message_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    request_id,
                    timestamp,
                    detection.pattern_name,
                    detection.pattern_type,
                    detection.original_value,
                    detection.placeholder,
                    detection.message_index,
                ],
            )?;
        }

        Ok(())
    }

    // ========================================================================
    // Cursor Hooks Methods
    // ========================================================================

    /// Log a cursor hook request (creates new entry)
    #[allow(clippy::too_many_arguments)]
    pub fn log_cursor_hook_request(
        &self,
        generation_id: &str,
        endpoint_name: &str,
        model: &str,
        input_tokens: i32,
        output_tokens: i32,
        request_body: &str,
        response_body: &str,
        response_status: u16,
        extra_metadata: Option<&str>,
        request_headers: Option<&str>,
        response_headers: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Check if entry already exists for this generation_id
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM requests WHERE json_extract(extra_metadata, '$.generation_id') = ?1 AND backend = 'cursor-hooks'",
                rusqlite::params![generation_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing_id {
            // Update existing entry
            conn.execute(
                "UPDATE requests SET
                    input_tokens = input_tokens + ?1,
                    response_status = ?2
                 WHERE id = ?3",
                rusqlite::params![input_tokens, response_status, id],
            )?;
            return Ok(id);
        }

        // Create new entry
        conn.execute(
            "INSERT INTO requests (
                timestamp, backend, endpoint_name, method, path, model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                latency_ms, has_system_prompt, has_tools, has_thinking, stop_reason,
                user_message_count, assistant_message_count,
                response_status, is_streaming, request_body, response_body, extra_metadata,
                request_headers, response_headers
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            rusqlite::params![
                timestamp,
                "cursor-hooks",
                endpoint_name,
                "POST",
                "/cursor_hook",
                if model.is_empty() { None } else { Some(model) },
                input_tokens,
                output_tokens,
                0, // cache_read_tokens
                0, // cache_creation_tokens
                0, // latency_ms (not applicable for hooks)
                0, // has_system_prompt
                0, // has_tools
                0, // has_thinking
                None::<String>, // stop_reason
                1, // user_message_count (prompt)
                0, // assistant_message_count
                response_status,
                0, // is_streaming
                request_body,
                response_body,
                extra_metadata,
                request_headers,
                response_headers,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Update cursor hook output tokens, response body, and latency by generation_id
    pub fn update_cursor_hook_output(
        &self,
        generation_id: &str,
        output_token_count: i32,
        response_text: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        // Find the request by generation_id in extra_metadata, also get timestamp for latency calculation
        let existing: Option<(i64, i32, String)> = conn
            .query_row(
                "SELECT id, output_tokens, timestamp FROM requests WHERE json_extract(extra_metadata, '$.generation_id') = ?1 AND backend = 'cursor-hooks'",
                rusqlite::params![generation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        if let Some((id, current_output, timestamp_str)) = existing {
            let new_output = current_output + output_token_count;

            // Calculate latency from stored timestamp
            let latency_ms = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .map(|start_time| {
                    let now = chrono::Utc::now();
                    (now.signed_duration_since(start_time)).num_milliseconds().max(0) as i64
                })
                .unwrap_or(0);

            if let Some(text) = response_text {
                conn.execute(
                    "UPDATE requests SET output_tokens = ?1, response_body = ?2, assistant_message_count = 1, latency_ms = ?3 WHERE id = ?4",
                    rusqlite::params![new_output, text, latency_ms, id],
                )?;
            } else {
                conn.execute(
                    "UPDATE requests SET output_tokens = ?1, latency_ms = ?2 WHERE id = ?3",
                    rusqlite::params![new_output, latency_ms, id],
                )?;
            }
        }

        Ok(())
    }

    /// Add thinking tokens to cursor hook output by generation_id
    pub fn add_cursor_hook_thinking_tokens(
        &self,
        generation_id: &str,
        thinking_word_count: i32,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        // Find and update the request
        conn.execute(
            "UPDATE requests SET
                output_tokens = output_tokens + ?1,
                has_thinking = 1
             WHERE json_extract(extra_metadata, '$.generation_id') = ?2 AND backend = 'cursor-hooks'",
            rusqlite::params![thinking_word_count, generation_id],
        )?;

        Ok(())
    }
}

// Port management helpers

pub fn get_port_from_db() -> u16 {
    let conn = match Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(_) => return DEFAULT_PORT,
    };

    // Ensure settings table exists
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    );

    conn.query_row(
        "SELECT value FROM settings WHERE key = 'proxy_port'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(DEFAULT_PORT)
}

pub fn save_port_to_db(port: u16) -> Result<(), String> {
    let conn = Connection::open(DB_PATH).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('proxy_port', ?1)",
        rusqlite::params![port.to_string()],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

