//! Authenticated companion for local Profile/template/service workflows.
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Response, Server, StatusCode};
type JobState = Arc<Mutex<Option<(u64, Option<Value>)>>>;

fn record_task(id: u64, action: &str, state: &str, result: Option<&Value>) -> Result<()> {
    let value = json!({"id":id.to_string(),"action":action,"state":state,"result":result});
    let path = crate::config::config_dir_path().join(".web-task.json");
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Task recovery record must be a regular file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    super::file::activation::atomic_write(&path, value.to_string().as_bytes())
}

fn authorized(headers: &[Header], token: &str) -> bool {
    headers
        .iter()
        .any(|h| h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {token}"))
}

pub fn serve(listen: SocketAddr, token_file: &Path) -> Result<()> {
    super::command::BACKGROUND.with(|background| background.set(true));
    anyhow::ensure!(
        listen.ip().is_loopback(),
        "The management companion must listen on a loopback address; use an authenticated TLS reverse proxy for remote access"
    );
    let token = std::fs::read_to_string(token_file).context("Read management token file")?;
    let token = token.trim();
    anyhow::ensure!(
        token.len() >= 24 && !token.chars().any(char::is_whitespace),
        "Use a management token with at least 24 characters and no whitespace"
    );
    let server = Server::http(listen).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let job: JobState = Arc::default();
    if let Ok(bytes) = std::fs::read(crate::config::config_dir_path().join(".web-task.json"))
        && let Ok(previous) = serde_json::from_slice::<Value>(&bytes)
        && let Some(id) = previous["id"]
            .as_str()
            .and_then(|id| id.parse::<u64>().ok())
    {
        *job.lock().unwrap() = Some((id, Some(recovered_result(&previous))));
    }
    eprintln!("Management companion: http://{listen}/ (MetaCubeXD v1.273.1)");
    for mut request in server.incoming_requests() {
        let (status, content_type, body) = if request.method().as_str() == "GET"
            && request.url() == "/"
        {
            (
                200,
                "text/html; charset=utf-8",
                include_str!("../../web/index.html").to_owned(),
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
                    Some((id, result)) => {
                        json!({"id":id.to_string(),"pending":result.is_none(),"result":result})
                    }
                    None => json!({"pending":false}),
                }),
                ("GET", "/api/state") => {
                    if job
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|(_, result)| result.is_none())
                    {
                        Err(anyhow::anyhow!(
                            "Management operation is running; wait for its result"
                        ))
                    } else {
                        super::management::snapshot()
                    }
                }
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
                    let mut state = job.lock().unwrap();
                    anyhow::ensure!(
                        !state.as_ref().is_some_and(|(_, result)| result.is_none()),
                        "Another operation is running"
                    );
                    let id = fastrand::u64(..);
                    let action = value["action"].as_str().unwrap_or("unknown").to_owned();
                    record_task(id, &action, "running", None)?;
                    *state = Some((id, None));
                    let job = Arc::clone(&job);
                    let task_token = token.to_owned();
                    std::thread::spawn(move || {
                        super::command::BACKGROUND.with(|background| background.set(true));
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            super::management::execute(value)
                        }));
                        let mut result = match result {
                            Ok(Ok(value)) => json!({"ok":true,"value":value}),
                            Ok(Err(error)) => {
                                json!({"ok":false,"error":error.to_string().replace(&task_token, "[redacted]")})
                            }
                            Err(_) => {
                                json!({"ok":false,"error":"Management task failed unexpectedly; refresh before retrying"})
                            }
                        };
                        if let Err(error) = record_task(
                            id,
                            &action,
                            if result["ok"] == true {
                                "completed"
                            } else {
                                "failed"
                            },
                            Some(&result),
                        ) {
                            result = json!({"ok":false,"operation_result":result,"error":format!("Operation finished but its recovery record could not be saved: {}. Refresh before retrying.", error.to_string().replace(&task_token, "[redacted]"))});
                        }
                        *job.lock().unwrap() = Some((id, Some(result)));
                    });
                    Ok(json!({"job":id.to_string()}))
                })(),
                _ => Err(anyhow::anyhow!("Route not found")),
            };
            match result {
                Ok(value) => (200, "application/json", value.to_string()),
                Err(error) => {
                    let message = error.to_string();
                    let status = if message.starts_with("Management state is busy") {
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
                        serde_json::json!({"error":message.replace(token, "[redacted]")})
                            .to_string(),
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
    Ok(())
}

fn recovered_result(record: &Value) -> Value {
    if matches!(record["state"].as_str(), Some("completed" | "failed"))
        && record["result"].is_object()
    {
        record["result"].clone()
    } else {
        json!({"ok":false,"error":"Management server restarted during a task; refresh and check the current state before retrying"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_restores_completed_outcomes_without_replaying_pending_tasks() {
        let success = json!({"state":"completed","result":{"ok":true,"value":{"name":"new"}}});
        assert_eq!(recovered_result(&success), success["result"]);
        let failure = json!({"state":"failed","result":{"ok":false,"error":"validation failed"}});
        assert_eq!(recovered_result(&failure), failure["result"]);
        assert_eq!(recovered_result(&json!({"state":"running"}))["ok"], false);
    }
    #[test]
    fn core_secret_is_not_an_alternative_to_management_auth() {
        let headers = vec![Header::from_bytes("Authorization", "Bearer core-secret").unwrap()];
        assert!(!authorized(&headers, "independent-management-token"));
        assert!(authorized(&headers, "core-secret"));
        assert!(!authorized(&[], "core-secret"));
    }
}
