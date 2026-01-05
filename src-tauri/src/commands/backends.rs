// Backend Management Commands

use crate::backends::claude::ANTHROPIC_BASE_URL;
use crate::backends::codex::CODEX_BASE_URL;
use crate::database::{CustomBackendRecord, Database};
use crate::dlp_pattern_config::get_db_path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomBackendResponse {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub settings: String,
    pub enabled: bool,
    pub created_at: String,
}

impl From<CustomBackendRecord> for CustomBackendResponse {
    fn from(record: CustomBackendRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            base_url: record.base_url,
            settings: record.settings,
            enabled: record.enabled,
            created_at: record.created_at,
        }
    }
}

/// Get all custom backends
#[tauri::command]
pub fn get_custom_backends() -> Result<Vec<CustomBackendResponse>, String> {
    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    db.get_custom_backends()
        .map(|backends| backends.into_iter().map(|b| b.into()).collect())
        .map_err(|e| e.to_string())
}

/// Add a new custom backend
#[tauri::command]
pub fn add_custom_backend(
    name: String,
    base_url: String,
    settings: String,
) -> Result<i64, String> {
    // Validate name - must be alphanumeric with hyphens/underscores, no spaces
    let name = name.trim();
    if name.is_empty() {
        return Err("Backend name cannot be empty".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Backend name can only contain letters, numbers, hyphens, and underscores".to_string());
    }

    // Validate URL
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err("Base URL cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("Base URL must start with http:// or https://".to_string());
    }

    // Validate settings is valid JSON
    let settings = settings.trim();
    if !settings.is_empty() && settings != "{}" {
        serde_json::from_str::<serde_json::Value>(settings)
            .map_err(|_| "Settings must be valid JSON".to_string())?;
    }
    let settings = if settings.is_empty() { "{}" } else { settings };

    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    // Check if name already exists
    if db.backend_name_exists(name).map_err(|e| e.to_string())? {
        return Err(format!("Backend name '{}' already exists or is reserved", name));
    }

    db.add_custom_backend(name, base_url, settings)
        .map_err(|e| e.to_string())
}

/// Update an existing custom backend
#[tauri::command]
pub fn update_custom_backend(
    id: i64,
    name: String,
    base_url: String,
    settings: String,
) -> Result<(), String> {
    // Validate name
    let name = name.trim();
    if name.is_empty() {
        return Err("Backend name cannot be empty".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Backend name can only contain letters, numbers, hyphens, and underscores".to_string());
    }

    // Validate URL
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err("Base URL cannot be empty".to_string());
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err("Base URL must start with http:// or https://".to_string());
    }

    // Validate settings is valid JSON
    let settings = settings.trim();
    if !settings.is_empty() && settings != "{}" {
        serde_json::from_str::<serde_json::Value>(settings)
            .map_err(|_| "Settings must be valid JSON".to_string())?;
    }
    let settings = if settings.is_empty() { "{}" } else { settings };

    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    // Check if name already exists (excluding this backend)
    if db.backend_name_exists_excluding(name, id).map_err(|e| e.to_string())? {
        return Err(format!("Backend name '{}' already exists or is reserved", name));
    }

    db.update_custom_backend(id, name, base_url, settings)
        .map_err(|e| e.to_string())
}

/// Toggle a custom backend enabled/disabled
#[tauri::command]
pub fn toggle_custom_backend(id: i64, enabled: bool) -> Result<(), String> {
    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    db.toggle_custom_backend(id, enabled)
        .map_err(|e| e.to_string())
}

/// Delete a custom backend
#[tauri::command]
pub fn delete_custom_backend(id: i64) -> Result<(), String> {
    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    db.delete_custom_backend(id)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Predefined Backend Commands
// ============================================================================

/// Predefined backend information with settings
#[derive(Debug, Serialize, Deserialize)]
pub struct PredefinedBackendResponse {
    pub name: String,
    pub base_url: String,
    pub settings: String,
}

/// List of predefined backends
const PREDEFINED_BACKENDS: &[(&str, &str)] = &[
    ("claude", ANTHROPIC_BASE_URL),
    ("codex", CODEX_BASE_URL),
    ("cursor-hooks", "N/A"),
];

/// Get all predefined backends with their settings
#[tauri::command]
pub fn get_predefined_backends() -> Result<Vec<PredefinedBackendResponse>, String> {
    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    let mut backends = Vec::new();
    for (name, base_url) in PREDEFINED_BACKENDS {
        let settings = db
            .get_predefined_backend_settings(name)
            .map_err(|e| e.to_string())?;

        backends.push(PredefinedBackendResponse {
            name: name.to_string(),
            base_url: base_url.to_string(),
            settings,
        });
    }

    Ok(backends)
}

/// Update settings for a predefined backend
#[tauri::command]
pub fn update_predefined_backend(name: String, settings: String) -> Result<(), String> {
    // Validate name is a known predefined backend
    let valid_names: Vec<&str> = PREDEFINED_BACKENDS.iter().map(|(n, _)| *n).collect();
    if !valid_names.contains(&name.as_str()) {
        return Err(format!("Unknown predefined backend: {}", name));
    }

    // Validate settings is valid JSON
    let settings = settings.trim();
    if !settings.is_empty() && settings != "{}" {
        serde_json::from_str::<serde_json::Value>(settings)
            .map_err(|_| "Settings must be valid JSON".to_string())?;
    }
    let settings = if settings.is_empty() { "{}" } else { settings };

    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    db.update_predefined_backend_settings(&name, settings)
        .map_err(|e| e.to_string())
}

/// Reset predefined backend settings to defaults
#[tauri::command]
pub fn reset_predefined_backend(name: String) -> Result<(), String> {
    // Validate name is a known predefined backend
    let valid_names: Vec<&str> = PREDEFINED_BACKENDS.iter().map(|(n, _)| *n).collect();
    if !valid_names.contains(&name.as_str()) {
        return Err(format!("Unknown predefined backend: {}", name));
    }

    let db = Database::new(get_db_path()).map_err(|e| e.to_string())?;

    db.reset_predefined_backend_settings(&name)
        .map_err(|e| e.to_string())
}
