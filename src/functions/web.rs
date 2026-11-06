//! Authenticated core dashboard and local Profile/template/service workflows.
use anyhow::Result;
use serde_json::{Value, json};
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Response, Server, StatusCode};
mod auth;
mod core;
mod jobs;
use auth::authorized;
type JobState = Arc<Mutex<Option<jobs::Job>>>;

pub fn serve(listen: SocketAddr, token_file: &Path) -> Result<()> {
    super::command::BACKGROUND.with(|background| background.set(true));
    anyhow::ensure!(
        listen.ip().is_loopback(),
        "The management companion must listen on a loopback address; use an authenticated TLS reverse proxy for remote access"
    );
    let token = auth::load_token(token_file)?;
    let server = Server::http(listen).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let task_path = crate::config::config_dir_path().join(".web-task.json");
    let job: JobState = Arc::new(Mutex::new(jobs::Job::load(&task_path)?));
    let streams = core::Streams::start();
    eprintln!("Management companion: http://{listen}/ (ClashTui Vue dashboard)");
    // Slow core reads have a bounded pool; local state, jobs and actions keep
    // their own dispatch lane even when every read worker is waiting on Mihomo.
    let (send, receive) = std::sync::mpsc::sync_channel::<tiny_http::Request>(16);
    let receive = Arc::new(Mutex::new(receive));
    for _ in 0..4 {
        let receive = Arc::clone(&receive);
        let token = token.to_owned();
        let job = Arc::clone(&job);
        let streams = streams.clone();
        std::thread::spawn(move || {
            super::command::BACKGROUND.with(|background| background.set(true));
            loop {
                let request = match receive.lock().unwrap().recv() {
                    Ok(request) => request,
                    Err(_) => break,
                };
                handle_request(request, &token, &job, &streams);
            }
        });
    }
    for request in server.incoming_requests() {
        if request.method().as_str() == "POST"
            && request.url() == "/api/core/read"
            && authorized(request.headers(), &token)
        {
            if let Err(error) = send.try_send(request) {
                let request = match error {
                    std::sync::mpsc::TrySendError::Full(request)
                    | std::sync::mpsc::TrySendError::Disconnected(request) => request,
                };
                let _ = request.respond(
                    Response::from_string(
                        json!({"error":"Core read capacity is busy; retry shortly"}).to_string(),
                    )
                    .with_status_code(StatusCode(503))
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
                    .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap()),
                );
            }
        } else {
            handle_request(request, &token, &job, &streams);
        }
    }
    Ok(())
}

fn handle_request(
    mut request: tiny_http::Request,
    token: &str,
    job: &JobState,
    streams: &core::Streams,
) {
    let (status, content_type, body) = if request.method().as_str() == "GET" && request.url() == "/"
    {
        (
            200,
            "text/html; charset=utf-8",
            super::file::panel::HTML.to_owned(),
        )
    } else if !authorized(request.headers(), token) {
        (
            401,
            "application/json",
            serde_json::json!({"error":"Management authentication required"}).to_string(),
        )
    } else {
        let result = match (request.method().as_str(), request.url()) {
            ("GET", "/api/job") => Ok(match job.lock().unwrap().as_ref() {
                Some(job) => job.snapshot(),
                None => json!({"pending":false}),
            }),
            ("GET", "/api/state") => {
                if job.lock().unwrap().as_ref().is_some_and(jobs::Job::pending) {
                    Err(anyhow::anyhow!(
                        "Management operation is running; wait for its result"
                    ))
                } else {
                    super::management::snapshot()
                }
            }
            ("POST", "/api/core/read") => (|| {
                let mut bytes = Vec::new();
                request.as_reader().take(65537).read_to_end(&mut bytes)?;
                anyhow::ensure!(bytes.len() <= 65536, "Request exceeds 64 KiB limit");
                core::read(&serde_json::from_slice::<Value>(&bytes)?, streams)
            })(),
            ("POST", "/api/action") => (|| {
                let mut bytes = Vec::new();
                request
                    .as_reader()
                    .take(2 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    bytes.len() <= 2 * 1024 * 1024,
                    "Request exceeds 2 MiB limit"
                );
                let value: Value = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(
                    value.is_object() && value["action"].as_str().is_some_and(|a| !a.is_empty()),
                    "Request must be an object with an action"
                );
                let mut state = job.lock().unwrap();
                anyhow::ensure!(
                    !state.as_ref().is_some_and(jobs::Job::pending),
                    "Another operation is running"
                );
                let action = value["action"].as_str().unwrap_or("unknown").to_owned();
                let mut task = jobs::Job::running(&action);
                let id = task.id.clone();
                let path = crate::config::config_dir_path().join(".web-task.json");
                task.save(&path)?;
                *state = Some(task.clone());
                let job = Arc::clone(job);
                let task_token = token.to_owned();
                std::thread::spawn(move || {
                    super::command::BACKGROUND.with(|background| background.set(true));
                    let result = jobs::execute(
                        || {
                            if value["action"] == "core" {
                                core::execute(value)
                            } else {
                                super::management::execute(value)
                            }
                        },
                        &task_token,
                    );
                    task.finish(result);
                    if let Err(error) = task.save(&path) {
                        task.finish(json!({"ok":false,"code":"recovery_save_failed","operation_result":task.result,"error":format!("Operation finished but its recovery record could not be saved: {}. Refresh before retrying.", format!("{error:#}").replace(&task_token, "[redacted]"))}));
                    }
                    *job.lock().unwrap() = Some(task);
                });
                Ok(json!({"job":id.to_string()}))
            })(),
            _ => Err(anyhow::anyhow!("Route not found")),
        };
        match result {
            Ok(value) => (200, "application/json", value.to_string()),
            Err(error) => {
                let message = error.to_string();
                let status = if message.starts_with("Request exceeds") {
                    413
                } else if message == "Another operation is running" {
                    409
                } else if message.starts_with("Management state is busy")
                    || message.starts_with("Management operation is running")
                {
                    503
                } else if message.contains("Revision conflict") {
                    409
                } else if message == "Route not found" {
                    404
                } else {
                    400
                };
                (
                    status,
                    "application/json",
                    serde_json::json!({"error":message.replace(token, "[redacted]")}).to_string(),
                )
            }
        }
    };
    let response = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
        .with_header(Header::from_bytes("Referrer-Policy", "no-referrer").unwrap())
        .with_header(Header::from_bytes("X-Frame-Options", "DENY").unwrap());
    let _ = request.respond(response);
}
