// Claude (Anthropic) Backend Implementation

use crate::backends::custom::CustomBackendSettings;
use crate::backends::Backend;
use crate::requestresponsemetadata::{RequestMetadata, ResponseMetadata};

pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";

pub struct ClaudeBackend {
    settings: CustomBackendSettings,
}

impl ClaudeBackend {
    pub fn new() -> Self {
        Self {
            settings: CustomBackendSettings::default(),
        }
    }

    pub fn with_settings(settings_json: &str) -> Self {
        let settings: CustomBackendSettings = serde_json::from_str(settings_json)
            .unwrap_or_default();
        Self { settings }
    }
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn base_url(&self) -> &'static str {
        ANTHROPIC_BASE_URL
    }

    fn parse_request_metadata(&self, body: &str) -> RequestMetadata {
        let mut meta = RequestMetadata::default();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(model) = json.get("model").and_then(|v| v.as_str()) {
                meta.model = Some(model.to_string());
            }
            meta.has_system_prompt = json.get("system").is_some();
            meta.has_tools = json.get("tools").is_some();

            if let Some(messages) = json.get("messages").and_then(|v| v.as_array()) {
                for msg in messages {
                    if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                        match role {
                            "user" => meta.user_message_count += 1,
                            "assistant" => meta.assistant_message_count += 1,
                            _ => {}
                        }
                    }
                }
            }
        }

        meta
    }

    fn parse_response_metadata(&self, body: &str, is_streaming: bool) -> ResponseMetadata {
        let mut meta = ResponseMetadata::default();

        if is_streaming {
            // Check for thinking content blocks
            meta.has_thinking = body.contains("\"type\":\"thinking\"");

            // For streaming, look for message_delta event with final usage
            for line in body.lines() {
                if line.starts_with("data: ") && line.contains("message_delta") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line[6..]) {
                        // Get stop_reason
                        if let Some(delta) = json.get("delta") {
                            if let Some(reason) = delta.get("stop_reason").and_then(|v| v.as_str())
                            {
                                meta.stop_reason = Some(reason.to_string());
                            }
                        }

                        // Get usage
                        if let Some(usage) = json.get("usage") {
                            meta.input_tokens = usage
                                .get("input_tokens")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32;
                            meta.output_tokens = usage
                                .get("output_tokens")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32;
                            meta.cache_read_tokens = usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32;
                            meta.cache_creation_tokens = usage
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as i32;
                        }
                    }
                }
            }
        } else {
            // Non-streaming response
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
                // Get stop_reason
                if let Some(reason) = json.get("stop_reason").and_then(|v| v.as_str()) {
                    meta.stop_reason = Some(reason.to_string());
                }

                // Check for thinking in content
                if let Some(content) = json.get("content").and_then(|v| v.as_array()) {
                    meta.has_thinking = content
                        .iter()
                        .any(|c| c.get("type").and_then(|t| t.as_str()) == Some("thinking"));
                }

                if let Some(usage) = json.get("usage") {
                    meta.input_tokens = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    meta.output_tokens = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    meta.cache_read_tokens = usage
                        .get("cache_read_input_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    meta.cache_creation_tokens = usage
                        .get("cache_creation_input_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                }
            }
        }

        meta
    }

    fn should_log(&self, body: &str) -> bool {
        // Check if request body looks like a Messages API call
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            // Must have "messages" array and "model" field
            let has_messages = json.get("messages").and_then(|v| v.as_array()).is_some();
            let has_model = json.get("model").and_then(|v| v.as_str()).is_some();
            has_messages && has_model
        } else {
            false
        }
    }

    fn is_dlp_enabled(&self) -> bool {
        self.settings.dlp_enabled
    }

    fn get_rate_limit(&self) -> (u32, u32) {
        (self.settings.rate_limit_requests, self.settings.rate_limit_minutes.max(1))
    }

    fn get_max_tokens_limit(&self) -> (u32, String) {
        (self.settings.max_tokens_in_a_request, self.settings.action_for_max_tokens_in_a_request.clone())
    }
}
