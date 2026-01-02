// Stats and Monitoring Tauri Commands

use crate::database::{get_port_from_db, save_port_to_db};
use crate::dlp_pattern_config::DB_PATH;
use crate::{PROXY_PORT, RESTART_SENDER};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize)]
pub struct ModelStats {
    model: String,
    count: i64,
}

#[derive(Serialize)]
pub struct FeatureStats {
    with_system_prompt: i64,
    with_tools: i64,
    with_thinking: i64,
    total_requests: i64,
}

#[derive(Serialize)]
pub struct TokenTotals {
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation: i64,
}

#[derive(Serialize)]
pub struct RecentRequest {
    id: i64,
    timestamp: String,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    latency_ms: i64,
    stop_reason: String,
    has_thinking: bool,
}

#[derive(Serialize)]
pub struct MessageLog {
    id: i64,
    timestamp: String,
    backend: String,
    model: String,
    input_tokens: i64,
    output_tokens: i64,
    latency_ms: i64,
    request_body: Option<String>,
    response_body: Option<String>,
    request_headers: Option<String>,
    response_headers: Option<String>,
    dlp_action: i64, // 0=passed, 1=redacted, 2=blocked
}

#[derive(Serialize)]
pub struct PaginatedLogs {
    logs: Vec<MessageLog>,
    total: i64,
}

#[derive(Serialize)]
pub struct LatencyPoint {
    id: i64,
    latency_ms: i64,
}

#[derive(Serialize)]
pub struct DashboardData {
    models: Vec<ModelStats>,
    features: FeatureStats,
    token_totals: TokenTotals,
    recent_requests: Vec<RecentRequest>,
    latency_points: Vec<LatencyPoint>,
    total_requests: i64,
    avg_latency_ms: f64,
}

// Convert time range string to hours
fn time_range_to_hours(time_range: &str) -> i64 {
    match time_range {
        "1h" => 1,
        "6h" => 6,
        "1d" => 24,
        "7d" => 24 * 7,
        _ => 1, // default to 1 hour
    }
}

// Get timestamp for time range filter
fn get_cutoff_timestamp(hours: i64) -> String {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours);
    cutoff.to_rfc3339()
}

// Endpoint filter to include both proxy and cursor hook requests
const ENDPOINT_FILTER: &str = "endpoint_name IN ('Messages', 'CursorChat', 'CursorTab')";

#[tauri::command]
pub fn get_dashboard_stats(time_range: String, backend: String) -> Result<DashboardData, String> {
    let conn = Connection::open(DB_PATH).map_err(|e| e.to_string())?;

    let hours = time_range_to_hours(&time_range);
    let cutoff_ts = get_cutoff_timestamp(hours);

    // Build backend filter clause
    let backend_filter = if backend == "all" {
        String::new()
    } else {
        format!(" AND backend = '{}'", backend.replace('\'', "''"))
    };

    // Get model stats
    let mut model_stmt = conn
        .prepare(&format!(
            "SELECT COALESCE(model, 'unknown') as model, COUNT(*) as count
             FROM requests
             WHERE {} AND model IS NOT NULL AND timestamp >= ?1{}
             GROUP BY model
             ORDER BY count DESC",
            ENDPOINT_FILTER, backend_filter
        ))
        .map_err(|e| e.to_string())?;

    let models: Vec<ModelStats> = model_stmt
        .query_map([&cutoff_ts], |row| {
            Ok(ModelStats {
                model: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get feature stats
    let features: FeatureStats = conn
        .query_row(
            &format!(
                "SELECT
                    COALESCE(SUM(has_system_prompt), 0),
                    COALESCE(SUM(has_tools), 0),
                    COALESCE(SUM(has_thinking), 0),
                    COUNT(*)
                 FROM requests
                 WHERE {} AND timestamp >= ?1{}",
                ENDPOINT_FILTER, backend_filter
            ),
            [&cutoff_ts],
            |row| {
                Ok(FeatureStats {
                    with_system_prompt: row.get(0)?,
                    with_tools: row.get(1)?,
                    with_thinking: row.get(2)?,
                    total_requests: row.get(3)?,
                })
            },
        )
        .unwrap_or(FeatureStats {
            with_system_prompt: 0,
            with_tools: 0,
            with_thinking: 0,
            total_requests: 0,
        });

    // Get token totals
    let token_totals: TokenTotals = conn
        .query_row(
            &format!(
                "SELECT
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_read_tokens), 0),
                    COALESCE(SUM(cache_creation_tokens), 0)
                 FROM requests
                 WHERE {} AND timestamp >= ?1{}",
                ENDPOINT_FILTER, backend_filter
            ),
            [&cutoff_ts],
            |row| {
                Ok(TokenTotals {
                    input: row.get(0)?,
                    output: row.get(1)?,
                    cache_read: row.get(2)?,
                    cache_creation: row.get(3)?,
                })
            },
        )
        .unwrap_or(TokenTotals {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_creation: 0,
        });

    // Get recent requests (last 20) for token chart
    let mut recent_stmt = conn
        .prepare(&format!(
            "SELECT id, timestamp, COALESCE(model, 'unknown'), input_tokens, output_tokens,
                    cache_read_tokens, cache_creation_tokens, latency_ms,
                    COALESCE(stop_reason, 'unknown'), has_thinking
             FROM requests
             WHERE {} AND timestamp >= ?1{}
             ORDER BY id DESC
             LIMIT 20",
            ENDPOINT_FILTER, backend_filter
        ))
        .map_err(|e| e.to_string())?;

    let recent_requests: Vec<RecentRequest> = recent_stmt
        .query_map([&cutoff_ts], |row| {
            Ok(RecentRequest {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                model: row.get(2)?,
                input_tokens: row.get(3)?,
                output_tokens: row.get(4)?,
                cache_read_tokens: row.get(5)?,
                cache_creation_tokens: row.get(6)?,
                latency_ms: row.get(7)?,
                stop_reason: row.get(8)?,
                has_thinking: row.get::<_, i32>(9)? == 1,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get latency points for chart (last 50)
    let mut latency_stmt = conn
        .prepare(&format!(
            "SELECT id, latency_ms
             FROM requests
             WHERE {} AND latency_ms > 0 AND timestamp >= ?1{}
             ORDER BY id DESC
             LIMIT 50",
            ENDPOINT_FILTER, backend_filter
        ))
        .map_err(|e| e.to_string())?;

    let latency_points: Vec<LatencyPoint> = latency_stmt
        .query_map([&cutoff_ts], |row| {
            Ok(LatencyPoint {
                id: row.get(0)?,
                latency_ms: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get totals
    let total_requests: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM requests WHERE {} AND timestamp >= ?1{}",
                ENDPOINT_FILTER, backend_filter
            ),
            [&cutoff_ts],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let avg_latency_ms: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(AVG(latency_ms), 0)
                 FROM requests
                 WHERE {} AND latency_ms > 0 AND timestamp >= ?1{}",
                ENDPOINT_FILTER, backend_filter
            ),
            [&cutoff_ts],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    Ok(DashboardData {
        models,
        features,
        token_totals,
        recent_requests,
        latency_points,
        total_requests,
        avg_latency_ms,
    })
}

#[tauri::command]
pub fn get_backends() -> Result<Vec<String>, String> {
    let conn = Connection::open(DB_PATH).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT DISTINCT backend FROM requests ORDER BY backend")
        .map_err(|e| e.to_string())?;

    let backends: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(backends)
}

#[tauri::command]
pub fn get_models() -> Result<Vec<String>, String> {
    let conn = Connection::open(DB_PATH).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT DISTINCT COALESCE(model, 'unknown') FROM requests WHERE {} ORDER BY model",
            ENDPOINT_FILTER
        ))
        .map_err(|e| e.to_string())?;

    let models: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(models)
}

#[tauri::command]
pub fn get_message_logs(
    time_range: String,
    backend: String,
    model: String,
    dlp_action: String,
    page: i64,
) -> Result<PaginatedLogs, String> {
    let conn = Connection::open(DB_PATH).map_err(|e| e.to_string())?;

    let hours = time_range_to_hours(&time_range);
    let cutoff_ts = get_cutoff_timestamp(hours);

    let backend_filter = if backend == "all" {
        String::new()
    } else {
        format!(" AND backend = '{}'", backend.replace('\'', "''"))
    };

    let model_filter = if model == "all" {
        String::new()
    } else {
        format!(" AND COALESCE(model, 'unknown') = '{}'", model.replace('\'', "''"))
    };

    let dlp_filter = match dlp_action.as_str() {
        "passed" => " AND COALESCE(dlp_action, 0) = 0".to_string(),
        "redacted" => " AND dlp_action = 1".to_string(),
        "blocked" => " AND dlp_action = 2".to_string(),
        _ => String::new(),
    };

    let filters = format!("{}{}{}", backend_filter, model_filter, dlp_filter);

    // Get total count
    let total: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM requests WHERE {} AND timestamp >= ?1{}",
                ENDPOINT_FILTER, filters
            ),
            [&cutoff_ts],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let offset = page * 50;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, timestamp, backend, COALESCE(model, 'unknown'),
                    input_tokens, output_tokens, latency_ms, request_body, response_body,
                    request_headers, response_headers, COALESCE(dlp_action, 0)
             FROM requests
             WHERE {} AND timestamp >= ?1{}
             ORDER BY id DESC
             LIMIT 50 OFFSET ?2",
            ENDPOINT_FILTER, filters
        ))
        .map_err(|e| e.to_string())?;

    let logs: Vec<MessageLog> = stmt
        .query_map(rusqlite::params![&cutoff_ts, offset], |row| {
            Ok(MessageLog {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                backend: row.get(2)?,
                model: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                latency_ms: row.get(6)?,
                request_body: row.get(7)?,
                response_body: row.get(8)?,
                request_headers: row.get(9)?,
                response_headers: row.get(10)?,
                dlp_action: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(PaginatedLogs { logs, total })
}

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
pub fn get_port_setting() -> u16 {
    get_port_from_db()
}

#[tauri::command]
pub fn save_port_setting(port: u16) -> Result<(), String> {
    // Validate port range
    if !(1024..=65535).contains(&port) {
        return Err("Port must be between 1024 and 65535".to_string());
    }

    // Save to database
    save_port_to_db(port)?;

    // Update global state
    let mut current_port = PROXY_PORT.lock().unwrap();
    *current_port = port;

    Ok(())
}

#[tauri::command]
pub fn restart_proxy() -> Result<String, String> {
    let port = *PROXY_PORT.lock().unwrap();

    // Send restart signal
    let sender_guard = RESTART_SENDER.lock().unwrap();
    if let Some(sender) = sender_guard.as_ref() {
        sender.send(true).map_err(|e| e.to_string())?;
        Ok(format!("Proxy server restarting on port {}", port))
    } else {
        Err("Proxy server not initialized".to_string())
    }
}

// Get env var name and route for a given tool
fn get_tool_env_config(tool: &str) -> Result<(&'static str, &'static str), String> {
    match tool {
        "claude-code" => Ok(("ANTHROPIC_BASE_URL", "/claude")),
        "codex" => Ok(("OPENAI_BASE_URL", "/codex")),
        _ => Err(format!("Unknown tool: {}", tool)),
    }
}

#[tauri::command]
pub fn set_shell_env(shell: String, tool: String) -> Result<String, String> {
    let port = *PROXY_PORT.lock().unwrap();
    let (env_var, route) = get_tool_env_config(&tool)?;
    let base_url = format!("http://localhost:{}{}", port, route);

    match shell.as_str() {
        "fish" => {
            // Fish: use set -Ux for universal export (persists automatically)
            let output = std::process::Command::new("fish")
                .args(["-c", &format!("set -Ux {} \"{}\"", env_var, base_url)])
                .output()
                .map_err(|e| format!("Failed to run fish: {}", e))?;

            if output.status.success() {
                Ok(format!("{} set globally for Fish", env_var))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        }
        "bash" => {
            // Bash: append to ~/.bashrc
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let bashrc_path = format!("{}/.bashrc", home);
            let export_line = format!("export {}=\"{}\"", env_var, base_url);

            update_shell_config(&bashrc_path, &export_line, env_var)?;
            Ok(format!("{} added to ~/.bashrc. Run 'source ~/.bashrc' or restart your terminal.", env_var))
        }
        "zsh" => {
            // Zsh: append to ~/.zshrc
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let zshrc_path = format!("{}/.zshrc", home);
            let export_line = format!("export {}=\"{}\"", env_var, base_url);

            update_shell_config(&zshrc_path, &export_line, env_var)?;
            Ok(format!("{} added to ~/.zshrc. Run 'source ~/.zshrc' or restart your terminal.", env_var))
        }
        _ => Err(format!("Unknown shell: {}", shell)),
    }
}

#[tauri::command]
pub fn check_shell_env(shell: String, tool: String) -> Result<bool, String> {
    let (env_var, _) = get_tool_env_config(&tool)?;

    match shell.as_str() {
        "fish" => {
            // Fish: check if universal variable is set
            let output = std::process::Command::new("fish")
                .args(["-c", &format!("set -q {}; and echo 1; or echo 0", env_var)])
                .output()
                .map_err(|e| format!("Failed to run fish: {}", e))?;

            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(result == "1")
        }
        "bash" => {
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let bashrc_path = format!("{}/.bashrc", home);
            Ok(check_env_in_config(&bashrc_path, env_var))
        }
        "zsh" => {
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let zshrc_path = format!("{}/.zshrc", home);
            Ok(check_env_in_config(&zshrc_path, env_var))
        }
        _ => Err(format!("Unknown shell: {}", shell)),
    }
}

// Check if env var is in config file
fn check_env_in_config(path: &str, env_var: &str) -> bool {
    if let Ok(content) = std::fs::read_to_string(path) {
        let prefix = format!("export {}=", env_var);
        content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with(&prefix) && !trimmed.starts_with('#')
        })
    } else {
        false
    }
}

#[tauri::command]
pub fn remove_shell_env(shell: String, tool: String) -> Result<String, String> {
    let (env_var, _) = get_tool_env_config(&tool)?;

    match shell.as_str() {
        "fish" => {
            // Fish: erase universal variable
            let output = std::process::Command::new("fish")
                .args(["-c", &format!("set -Ue {}", env_var)])
                .output()
                .map_err(|e| format!("Failed to run fish: {}", e))?;

            if output.status.success() {
                Ok(format!("{} removed from Fish", env_var))
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        }
        "bash" => {
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let bashrc_path = format!("{}/.bashrc", home);
            remove_env_from_config(&bashrc_path, env_var)?;
            Ok(format!("{} removed from ~/.bashrc. Restart your terminal.", env_var))
        }
        "zsh" => {
            let home = std::env::var("HOME").map_err(|_| "Could not get HOME directory")?;
            let zshrc_path = format!("{}/.zshrc", home);
            remove_env_from_config(&zshrc_path, env_var)?;
            Ok(format!("{} removed from ~/.zshrc. Restart your terminal.", env_var))
        }
        _ => Err(format!("Unknown shell: {}", shell)),
    }
}

// Remove env var from config file
fn remove_env_from_config(path: &str, env_var: &str) -> Result<(), String> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    let content = fs::read_to_string(path).unwrap_or_default();
    let mut new_lines: Vec<&str> = Vec::new();
    let mut prev_was_comment = false;
    let export_prefix = format!("export {}=", env_var);

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip the "# Agent Gateway" comment if followed by the env var
        if trimmed == "# Agent Gateway" {
            prev_was_comment = true;
            continue;
        }

        if trimmed.starts_with(&export_prefix) {
            prev_was_comment = false;
            continue;
        }

        // If previous was comment but this isn't the env var, add back the comment
        if prev_was_comment {
            new_lines.push("# Agent Gateway");
            prev_was_comment = false;
        }

        new_lines.push(line);
    }

    // Remove trailing empty lines
    while new_lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        new_lines.pop();
    }

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|e| format!("Failed to open {}: {}", path, e))?;

    for line in new_lines {
        writeln!(file, "{}", line).map_err(|e| format!("Failed to write: {}", e))?;
    }

    Ok(())
}

// Helper function to update shell config files
fn update_shell_config(path: &str, export_line: &str, env_var: &str) -> Result<(), String> {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    // Read existing content
    let content = fs::read_to_string(path).unwrap_or_default();

    // Check if env var is already set
    let lines: Vec<&str> = content.lines().collect();
    let mut found = false;
    let mut new_lines: Vec<String> = Vec::new();

    for line in &lines {
        if line.contains(env_var) {
            // Replace existing line
            new_lines.push(export_line.to_string());
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !found {
        // Add new line at the end
        new_lines.push(String::new()); // Empty line before
        new_lines.push("# Agent Gateway".to_string());
        new_lines.push(export_line.to_string());
    }

    // Write back
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .map_err(|e| format!("Failed to open {}: {}", path, e))?;

    for line in new_lines {
        writeln!(file, "{}", line).map_err(|e| format!("Failed to write: {}", e))?;
    }

    Ok(())
}
