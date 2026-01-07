// Database operations and schema management

use crate::builtin_patterns::get_builtin_patterns;
use crate::dlp::DlpDetection;
use crate::dlp_pattern_config::{get_db_path, DEFAULT_PORT};
use crate::requestresponsemetadata::{RequestMetadata, ResponseMetadata};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

// ============================================================================
// DLP Action Status Codes
// ============================================================================

/// DLP action: Content passed without any sensitive data detected
pub const DLP_ACTION_PASSED: i32 = 0;

/// DLP action: Sensitive data was detected and redacted
pub const DLP_ACTION_REDACTED: i32 = 1;

/// DLP action: Sensitive data was detected and request was blocked
pub const DLP_ACTION_BLOCKED: i32 = 2;

/// DLP action: Request was blocked due to rate limit or token limit
pub const DLP_ACTION_RATELIMITED: i32 = 3;

/// DLP action: Token limit exceeded but request was allowed (notify mode)
pub const DLP_ACTION_NOTIFY_RATELIMIT: i32 = 4;

/// Thread-safe database wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;

        // SQLite performance settings
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;
            PRAGMA temp_store = MEMORY;
        ")?;

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

        // Migration: Add dlp_action column if it doesn't exist
        // Uses DLP_ACTION_PASSED (0), DLP_ACTION_REDACTED (1), DLP_ACTION_BLOCKED (2)
        let _ = conn.execute(
            "ALTER TABLE requests ADD COLUMN dlp_action INTEGER DEFAULT 0",
            [],
        );

        // Create index for faster generation_id lookups (timestamp + backend filtering)
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_requests_timestamp_backend ON requests(timestamp, backend)",
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

        // Index for faster cleanup of dlp_detections by request_id
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_dlp_detections_request_id ON dlp_detections(request_id)",
            [],
        );

        // Create custom backends table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS custom_backends (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                base_url TEXT NOT NULL,
                settings TEXT DEFAULT '{}',
                enabled INTEGER DEFAULT 1,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        // Create index for faster backend name lookups
        let _ = conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_custom_backends_name ON custom_backends(name)",
            [],
        );

        // Create predefined backend settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS predefined_backend_settings (
                name TEXT PRIMARY KEY,
                settings TEXT DEFAULT '{}',
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Seed builtin DLP patterns, overwriting if they already exist
    fn seed_builtin_patterns(conn: &Connection) -> Result<(), rusqlite::Error> {
        let builtin_patterns = get_builtin_patterns();
        let created_at = chrono::Utc::now().to_rfc3339();

        for pattern in builtin_patterns {
            // Convert static slices to JSON strings for storage
            let patterns_vec: Vec<&str> = pattern.patterns.to_vec();
            let patterns_json =
                serde_json::to_string(&patterns_vec).unwrap_or_else(|_| "[]".to_string());
            let negative_patterns_json = pattern.negative_patterns.map(|np| {
                let np_vec: Vec<&str> = np.to_vec();
                serde_json::to_string(&np_vec).unwrap_or_else(|_| "[]".to_string())
            });

            // Check if this builtin pattern already exists
            let existing_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM dlp_patterns WHERE is_builtin = 1 AND name = ?1",
                    rusqlite::params![pattern.name],
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

        // Delete DLP detections for requests that will be deleted (by relationship, not timestamp)
        conn.execute(
            "DELETE FROM dlp_detections WHERE request_id IN (SELECT id FROM requests WHERE timestamp < ?1)",
            rusqlite::params![cutoff_ts],
        )?;

        // Delete old requests
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
        dlp_action: i32,
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
                request_headers, response_headers, dlp_action
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
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
                dlp_action,
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
        dlp_action: i32,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Check if entry already exists for this generation_id (within last 5 minutes for faster lookup)
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM requests WHERE timestamp >= ?1 AND backend = 'cursor-hooks' AND json_extract(extra_metadata, '$.generation_id') = ?2",
                rusqlite::params![cutoff, generation_id],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing_id {
            // Update existing entry - only upgrade dlp_action (blocked > redacted > passed)
            conn.execute(
                "UPDATE requests SET
                    input_tokens = input_tokens + ?1,
                    response_status = CASE WHEN ?2 > response_status THEN ?2 ELSE response_status END,
                    dlp_action = CASE WHEN ?3 > dlp_action THEN ?3 ELSE dlp_action END
                 WHERE id = ?4",
                rusqlite::params![input_tokens, response_status, dlp_action, id],
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
                request_headers, response_headers, dlp_action
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
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
                dlp_action,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Update cursor hook output tokens, response body, and latency by generation_id
    /// Returns true if an entry was found and updated, false otherwise
    pub fn update_cursor_hook_output(
        &self,
        generation_id: &str,
        output_token_count: i32,
        response_text: Option<&str>,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        // Find the request by generation_id in extra_metadata (within last 5 minutes for faster lookup)
        // Also get timestamp for latency calculation
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let existing: Option<(i64, i32, String)> = conn
            .query_row(
                "SELECT id, output_tokens, timestamp FROM requests WHERE timestamp >= ?1 AND backend = 'cursor-hooks' AND json_extract(extra_metadata, '$.generation_id') = ?2",
                rusqlite::params![cutoff, generation_id],
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
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Add thinking tokens to cursor hook output by generation_id
    /// Returns true if an entry was found and updated, false otherwise
    pub fn add_cursor_hook_thinking_tokens(
        &self,
        generation_id: &str,
        thinking_word_count: i32,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        // Find and update the request (within last 5 minutes for faster lookup)
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let rows_affected = conn.execute(
            "UPDATE requests SET
                output_tokens = output_tokens + ?1,
                has_thinking = 1
             WHERE timestamp >= ?2 AND backend = 'cursor-hooks' AND json_extract(extra_metadata, '$.generation_id') = ?3",
            rusqlite::params![thinking_word_count, cutoff, generation_id],
        )?;

        Ok(rows_affected > 0)
    }

    // ========================================================================
    // Custom Backends Methods
    // ========================================================================

    /// Get all custom backends
    pub fn get_custom_backends(&self) -> Result<Vec<CustomBackendRecord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, settings, enabled, created_at FROM custom_backends ORDER BY created_at DESC",
        )?;

        let backends = stmt
            .query_map([], |row| {
                Ok(CustomBackendRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_url: row.get(2)?,
                    settings: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? == 1,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(backends)
    }

    /// Get enabled custom backends only
    pub fn get_enabled_custom_backends(&self) -> Result<Vec<CustomBackendRecord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, settings, enabled, created_at FROM custom_backends WHERE enabled = 1 ORDER BY created_at DESC",
        )?;

        let backends = stmt
            .query_map([], |row| {
                Ok(CustomBackendRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_url: row.get(2)?,
                    settings: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? == 1,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(backends)
    }

    /// Add a new custom backend
    pub fn add_custom_backend(
        &self,
        name: &str,
        base_url: &str,
        settings: &str,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let created_at = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO custom_backends (name, base_url, settings, enabled, created_at) VALUES (?1, ?2, ?3, 1, ?4)",
            rusqlite::params![name, base_url, settings, created_at],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Update a custom backend
    pub fn update_custom_backend(
        &self,
        id: i64,
        name: &str,
        base_url: &str,
        settings: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE custom_backends SET name = ?1, base_url = ?2, settings = ?3 WHERE id = ?4",
            rusqlite::params![name, base_url, settings, id],
        )?;

        Ok(())
    }

    /// Toggle a custom backend enabled/disabled
    pub fn toggle_custom_backend(&self, id: i64, enabled: bool) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE custom_backends SET enabled = ?1 WHERE id = ?2",
            rusqlite::params![enabled as i32, id],
        )?;

        Ok(())
    }

    /// Delete a custom backend
    pub fn delete_custom_backend(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute("DELETE FROM custom_backends WHERE id = ?1", rusqlite::params![id])?;

        Ok(())
    }

    /// Check if a backend name already exists (reserved or custom)
    pub fn backend_name_exists(&self, name: &str) -> Result<bool, rusqlite::Error> {
        // Check reserved names first
        let reserved = ["claude", "codex", "cursor_hook", "cursor-hooks"];
        if reserved.contains(&name.to_lowercase().as_str()) {
            return Ok(true);
        }

        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM custom_backends WHERE LOWER(name) = LOWER(?1)",
            rusqlite::params![name],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// Check if a backend name exists excluding a specific id (for updates)
    pub fn backend_name_exists_excluding(&self, name: &str, exclude_id: i64) -> Result<bool, rusqlite::Error> {
        // Check reserved names first
        let reserved = ["claude", "codex", "cursor_hook", "cursor-hooks"];
        if reserved.contains(&name.to_lowercase().as_str()) {
            return Ok(true);
        }

        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM custom_backends WHERE LOWER(name) = LOWER(?1) AND id != ?2",
            rusqlite::params![name, exclude_id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    // ========================================================================
    // Predefined Backend Settings Methods
    // ========================================================================

    /// Get settings for a predefined backend (returns default settings if not set)
    pub fn get_predefined_backend_settings(&self, name: &str) -> Result<String, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        let settings: Option<String> = conn
            .query_row(
                "SELECT settings FROM predefined_backend_settings WHERE name = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .ok();

        // Return stored settings or default
        Ok(settings.unwrap_or_else(|| "{}".to_string()))
    }

    /// Update settings for a predefined backend
    pub fn update_predefined_backend_settings(&self, name: &str, settings: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let updated_at = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO predefined_backend_settings (name, settings, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET settings = ?2, updated_at = ?3",
            rusqlite::params![name, settings, updated_at],
        )?;

        Ok(())
    }

    /// Reset predefined backend settings to defaults (delete the record)
    pub fn reset_predefined_backend_settings(&self, name: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM predefined_backend_settings WHERE name = ?1",
            rusqlite::params![name],
        )?;

        Ok(())
    }
}

/// Custom backend record from database
#[derive(Debug, Clone)]
pub struct CustomBackendRecord {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub settings: String,
    pub enabled: bool,
    pub created_at: String,
}

// Port management helpers

pub fn get_port_from_db() -> u16 {
    let conn = match Connection::open(get_db_path()) {
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
    let conn = Connection::open(get_db_path()).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('proxy_port', ?1)",
        rusqlite::params![port.to_string()],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

// DLP action setting helpers

pub fn get_dlp_action_from_db() -> String {
    let conn = match Connection::open(get_db_path()) {
        Ok(c) => c,
        Err(_) => return "block".to_string(),
    };

    // Ensure settings table exists
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    );

    conn.query_row(
        "SELECT value FROM settings WHERE key = 'dlp_action'",
        [],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "block".to_string())
}

pub fn save_dlp_action_to_db(action: &str) -> Result<(), String> {
    // Validate action value
    if action != "redact" && action != "block" {
        return Err("Invalid dlp_action value. Must be 'redact' or 'block'".to_string());
    }

    let conn = Connection::open(get_db_path()).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('dlp_action', ?1)",
        rusqlite::params![action],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}


