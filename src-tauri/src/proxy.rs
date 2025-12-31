// HTTP Proxy Server and Handler

use crate::backends::{Backend, ClaudeBackend, CodexBackend};
use crate::cursor_hooks::create_cursor_hooks_router;
use crate::database::Database;
use crate::dlp::{apply_dlp_redaction, apply_dlp_unredaction};
use crate::dlp_pattern_config::DB_PATH;
use crate::{PROXY_PORT, RESTART_SENDER};

use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use flate2::read::GzDecoder;
use futures::StreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::watch;

/// Convert axum HeaderMap to JSON string
fn headers_to_json(headers: &HeaderMap) -> String {
    let map: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Convert reqwest HeaderMap to JSON string
fn reqwest_headers_to_json(headers: &reqwest::header::HeaderMap) -> String {
    let map: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Decompress gzip data
fn decompress_gzip(data: &[u8]) -> Option<String> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = String::new();
    match decoder.read_to_string(&mut decompressed) {
        Ok(_) => Some(decompressed),
        Err(_) => None,
    }
}

#[derive(Clone)]
struct ProxyState {
    db: Database,
    backend: Arc<dyn Backend>,
}

async fn health_handler() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"status":"healthy"}"#))
        .unwrap()
}

async fn proxy_handler(State(state): State<ProxyState>, req: Request) -> impl IntoResponse {
    let start_time = Instant::now();
    let client = Client::new();
    let backend = &state.backend;
    let db = &state.db;

    let method = req.method().clone();
    // When using nest("/claude", ...), axum automatically strips the prefix
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let full_path = format!("{}{}", path, query);
    let headers = req.headers().clone();

    let target_url = format!("{}{}", backend.base_url(), full_path);

    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("Failed to read request body"))
                .unwrap();
        }
    };

    let request_body_str = String::from_utf8_lossy(&body_bytes).to_string();
    let req_meta = backend.parse_request_metadata(&request_body_str);

    // Apply DLP redaction to request body
    let dlp_result = apply_dlp_redaction(&request_body_str);
    let redacted_body = dlp_result.redacted_body;
    let dlp_replacements = dlp_result.replacements;

    let mut reqwest_req = match method.clone() {
        Method::GET => client.get(&target_url),
        Method::POST => client.post(&target_url),
        Method::PUT => client.put(&target_url),
        Method::DELETE => client.delete(&target_url),
        Method::PATCH => client.patch(&target_url),
        _ => client.request(method.clone(), &target_url),
    };

    // Skip headers that we need to recalculate or that shouldn't be forwarded
    let skip_request_headers = ["host", "content-length"];
    for (name, value) in headers.iter() {
        let header_lower = name.as_str().to_lowercase();
        if !skip_request_headers.contains(&header_lower.as_str()) {
            if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_ref()) {
                if let Ok(header_value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes())
                {
                    reqwest_req = reqwest_req.header(header_name, header_value);
                }
            }
        }
    }

    // Use redacted body for the request
    if !body_bytes.is_empty() {
        reqwest_req = reqwest_req.body(redacted_body.clone().into_bytes());
    }

    let is_streaming = body_bytes
        .windows(13)
        .any(|w| w == b"\"stream\":true" || w == b"\"stream\": true");

    println!("[PROXY] Sending request to upstream: {}", target_url);
    let response = match reqwest_req.send().await {
        Ok(resp) => {
            println!("[PROXY] Got response from upstream: {}", resp.status());
            resp
        }
        Err(e) => {
            println!("[PROXY] Upstream error: {:?}", e);
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Proxy error: {}", e)))
                .unwrap();
        }
    };

    let status = response.status();
    let resp_headers = response.headers().clone();

    let mut response_headers = HeaderMap::new();
    let skip_headers = ["content-encoding", "content-length", "transfer-encoding"];

    for (name, value) in resp_headers.iter() {
        if !skip_headers.contains(&name.as_str().to_lowercase().as_str()) {
            if let Ok(header_name) = axum::http::header::HeaderName::from_bytes(name.as_ref()) {
                if let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) {
                    response_headers.insert(header_name, header_value);
                }
            }
        }
    }

    let method_str = method.to_string();
    let backend_name = backend.name().to_string();

    if is_streaming {
        response_headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        response_headers.insert(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );

        let db_clone = db.clone();
        let backend_clone = state.backend.clone();
        let path_clone = full_path.clone();
        let req_body_clone = request_body_str.clone();
        let status_code = status.as_u16();
        let req_meta_clone = req_meta.clone();
        let dlp_replacements_clone = dlp_replacements.clone();
        let dlp_detections_clone = dlp_result.detections.clone();
        let headers_clone = headers.clone();
        let request_headers_json = headers_to_json(&headers);
        let response_headers_json = reqwest_headers_to_json(&resp_headers);

        let collected_chunks: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let chunks_for_stream = collected_chunks.clone();
        let dlp_for_stream = dlp_replacements.clone();

        println!("[PROXY] Starting streaming response...");
        let stream = response.bytes_stream().map(move |result| {
            match result {
                Ok(bytes) => {
                    let chunk_str = String::from_utf8_lossy(&bytes).to_string();
                    println!("[PROXY] Received chunk of {} bytes", bytes.len());
                    chunks_for_stream.lock().unwrap().push(chunk_str.clone());

                    // Apply DLP unredaction to each chunk
                    let unredacted_chunk = apply_dlp_unredaction(&chunk_str, &dlp_for_stream);
                    Ok(Bytes::from(unredacted_chunk))
                }
                Err(e) => {
                    println!("[PROXY] Stream error: {}", e);
                    Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                }
            }
        });

        let logged_stream = async_stream::stream! {
            println!("[PROXY] Stream started, forwarding chunks to client...");
            let mut inner = std::pin::pin!(stream);
            let mut chunk_count = 0;
            while let Some(item) = inner.next().await {
                chunk_count += 1;
                yield item;
            }
            println!("[PROXY] Stream completed. Total chunks: {}", chunk_count);

            let latency_ms = start_time.elapsed().as_millis() as u64;
            let response_body = collected_chunks.lock().unwrap().join("");
            let unredacted_response = apply_dlp_unredaction(&response_body, &dlp_replacements_clone);
            let resp_meta = backend_clone.parse_response_metadata(&unredacted_response, true);

            // Only log if backend says we should
            if backend_clone.should_log(&req_body_clone) {
                // Extract extra metadata
                let extra_meta = backend_clone.extract_extra_metadata(
                    &req_body_clone,
                    &unredacted_response,
                    &headers_clone,
                );

                if let Ok(request_id) = db_clone.log_request(
                    &backend_name,
                    &method_str,
                    &path_clone,
                    "Messages",
                    &req_body_clone,
                    &unredacted_response,
                    status_code,
                    true,
                    latency_ms,
                    &req_meta_clone,
                    &resp_meta,
                    extra_meta.as_deref(),
                    Some(&request_headers_json),
                    Some(&response_headers_json),
                ) {
                    // Log DLP detections if any
                    if !dlp_detections_clone.is_empty() {
                        let _ = db_clone.log_dlp_detections(request_id, &dlp_detections_clone);
                    }
                }
            }
        };

        Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK))
            .body(Body::from_stream(logged_stream))
            .unwrap()
    } else {
        // Check if response is gzip encoded
        let is_gzip = resp_headers
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("gzip"))
            .unwrap_or(false);

        let body = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                return Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from(format!("Failed to read response: {}", e)))
                    .unwrap();
            }
        };

        let latency_ms = start_time.elapsed().as_millis() as u64;

        // Decompress if gzip, otherwise use as-is
        let response_body_str = if is_gzip {
            decompress_gzip(&body).unwrap_or_else(|| String::from_utf8_lossy(&body).to_string())
        } else {
            String::from_utf8_lossy(&body).to_string()
        };

        // Apply DLP unredaction to response
        let unredacted_response = apply_dlp_unredaction(&response_body_str, &dlp_replacements);

        let resp_meta = backend.parse_response_metadata(&unredacted_response, false);

        // Only log if backend says we should
        if backend.should_log(&request_body_str) {
            // Extract extra metadata
            let extra_meta = backend.extract_extra_metadata(
                &request_body_str,
                &unredacted_response,
                &headers,
            );

            // Convert headers to JSON
            let request_headers_json = headers_to_json(&headers);
            let response_headers_json = reqwest_headers_to_json(&resp_headers);

            if let Ok(request_id) = db.log_request(
                backend.name(),
                &method_str,
                &full_path,
                "Messages",
                &request_body_str,
                &unredacted_response,
                status.as_u16(),
                false,
                latency_ms,
                &req_meta,
                &resp_meta,
                extra_meta.as_deref(),
                Some(&request_headers_json),
                Some(&response_headers_json),
            ) {
                // Log DLP detections if any
                if !dlp_result.detections.is_empty() {
                    let _ = db.log_dlp_detections(request_id, &dlp_result.detections);
                }
            }
        }

        let mut resp = Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK));

        for (name, value) in response_headers.iter() {
            resp = resp.header(name, value);
        }

        // Return unredacted response body
        resp.body(Body::from(unredacted_response.into_bytes()))
            .unwrap()
    }
}

pub async fn start_proxy_server() {
    loop {
        // Get current port
        let port = *PROXY_PORT.lock().unwrap();

        let db = Database::new(DB_PATH).expect("Failed to initialize database");
        println!("Database initialized: {}", DB_PATH);

        // Clean up data older than 7 days on startup
        match db.cleanup_old_data() {
            Ok(deleted) => {
                if deleted > 0 {
                    println!("Cleaned up {} old records (>7 days)", deleted);
                }
            }
            Err(e) => eprintln!("Failed to cleanup old data: {}", e),
        }

        // Create backends
        let claude_backend: Arc<dyn Backend> = Arc::new(ClaudeBackend::new());
        let codex_backend: Arc<dyn Backend> = Arc::new(CodexBackend::new());

        // Create states for each backend
        let claude_state = ProxyState {
            db: db.clone(),
            backend: claude_backend,
        };
        let codex_state = ProxyState {
            db: db.clone(),
            backend: codex_backend,
        };

        // Create routers for each backend
        let claude_router = Router::new()
            .fallback(proxy_handler)
            .with_state(claude_state);
        let codex_router = Router::new()
            .fallback(proxy_handler)
            .with_state(codex_state);

        // Create cursor hooks router
        let cursor_hooks_router = create_cursor_hooks_router(db.clone());

        let app = Router::new()
            .route("/", get(health_handler))
            .nest("/claude", claude_router)
            .nest("/codex", codex_router)
            .nest("/cursor_hook", cursor_hooks_router);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind to port {}: {}", port, e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };
        println!("Proxy server running on http://0.0.0.0:{}", port);

        // Create shutdown channel
        let (tx, mut rx) = watch::channel(false);
        {
            let mut sender = RESTART_SENDER.lock().unwrap();
            *sender = Some(tx);
        }

        // Run server with graceful shutdown
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            loop {
                rx.changed().await.ok();
                if *rx.borrow() {
                    println!("Received restart signal, shutting down proxy server...");
                    break;
                }
            }
        });

        if let Err(e) = server.await {
            eprintln!("Proxy server error: {}", e);
        }

        println!("Proxy server stopped, restarting with new configuration...");
        // Small delay before restart
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}
