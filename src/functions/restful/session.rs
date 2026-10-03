//! A request uses one immutable endpoint/authentication snapshot.
//! Never derive Debug: the session contains the controller secret.

use crate::config::{CONFIG, CoreType};
use minreq::{Method, Response};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static CURRENT: Mutex<Option<CoreSession>> = Mutex::new(None);
thread_local! {
    static THREAD_SESSION: std::cell::RefCell<Option<CoreSession>> = const { std::cell::RefCell::new(None) };
}
#[cfg(feature = "tui")]
tokio::task_local! {
    pub(crate) static TASK_SESSION: CoreSession;
}

#[derive(Clone)]
pub struct CoreSession {
    core_type: CoreType,
    controller: String,
    secret: Option<String>,
    timeout: u64,
    generation: Option<u64>,
    identity: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApiError {
    Authentication(u16),
    Unavailable(u16),
    Http {
        status: u16,
        message: String,
    },
    CoreMismatch {
        expected: CoreType,
        actual: CoreType,
    },
    SessionChanged,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication(status) => write!(f, "HTTP {status}: core authentication failed"),
            Self::Unavailable(status) => {
                write!(f, "HTTP {status}: endpoint is unavailable or unsupported")
            }
            Self::Http { status, message } => write!(f, "HTTP {status}: {message}"),
            Self::CoreMismatch { expected, actual } => {
                write!(
                    f,
                    "API returned {actual} data, but {expected} is configured"
                )
            }
            Self::SessionChanged => write!(
                f,
                "Core changed while the request was running; refresh and retry"
            ),
        }
    }
}

impl std::error::Error for ApiError {}

impl CoreSession {
    pub(crate) fn controller(&self) -> &str {
        &self.controller
    }
    pub(crate) fn secret(&self) -> Option<&str> {
        self.secret.as_deref()
    }
    pub(super) fn core_type(&self) -> CoreType {
        self.core_type
    }
    pub fn current() -> Self {
        if let Some(session) = THREAD_SESSION.with(|s| s.borrow().clone()) {
            return session;
        }
        #[cfg(feature = "tui")]
        if let Ok(session) = TASK_SESSION.try_with(Clone::clone) {
            return session;
        }
        Self::snapshot()
    }

    pub fn try_current() -> Option<Self> {
        crate::config::initialized().then(Self::current)
    }

    pub fn is_current(&self) -> bool {
        self.generation
            .is_none_or(|generation| generation == crate::config::core_generation())
    }

    pub(crate) fn snapshot() -> Self {
        // Read the active core once so endpoint and secret cannot come from
        // different cores if another task changes the selection.
        let data = CONFIG.data.lock().unwrap();
        let core_type = data.core_type;
        let generation = Some(crate::config::core_generation());
        let mut cached = CURRENT.lock().unwrap();
        if let Some(session) = cached.as_ref()
            && session.generation == generation
        {
            return session.clone();
        }
        let (controller, secret) = match core_type {
            CoreType::Mihomo => (&CONFIG.external_controller, &CONFIG.secret),
        };
        let session = Self {
            core_type,
            controller: controller.trim_end_matches('/').to_owned(),
            secret: secret.clone(),
            timeout: CONFIG.cfg_file.timeout.unwrap_or(super::DEFAULT_TIMEOUT),
            generation,
            identity: Arc::default(),
        };
        *cached = Some(session.clone());
        session
    }

    pub fn request(
        &self,
        method: Method,
        path: &str,
        payload: Option<String>,
    ) -> super::Result<Response> {
        self.request_with_timeout(method, path, payload, self.timeout)
    }

    /// Override only the operation timeout; identity checks retain their limit.
    pub fn request_with_timeout(
        &self,
        method: Method,
        path: &str,
        payload: Option<String>,
        timeout_seconds: u64,
    ) -> super::Result<Response> {
        if !self.is_current() {
            return Err(api_error(ApiError::SessionChanged));
        }
        if path != "/version" {
            // Verify the actual backend, including when Status is not active.
            // Errors from /version (auth, network, JSON) retain their meaning.
            let mut identity = self.identity.lock().unwrap();
            let mutation =
                !matches!(method, Method::Get | Method::Head) || path.contains("/healthcheck");
            if mutation || identity.is_none_or(|time| time.elapsed() >= Duration::from_secs(2)) {
                *identity = None;
                let version = self.send(Method::Get, "/version", None)?;
                let actual = super::core_detect::parse_core_type(version.json()?)?;
                if actual != self.core_type {
                    return Err(api_error(ApiError::CoreMismatch {
                        expected: self.core_type,
                        actual,
                    }));
                }
                *identity = Some(Instant::now());
            }
        }
        if !self.is_current() {
            return Err(api_error(ApiError::SessionChanged));
        }
        let result = self.send_with_timeout(method, path, payload, timeout_seconds);
        if !self.is_current() {
            return Err(api_error(ApiError::SessionChanged));
        }
        result
    }

    pub fn patch_and_fetch(
        &self,
        payload: String,
    ) -> super::Result<super::config_struct::ClashConfig> {
        self.request(Method::Patch, "/configs", Some(payload))?;
        self.request(Method::Get, "/configs", None)?.json()
    }

    fn send(&self, method: Method, path: &str, payload: Option<String>) -> super::Result<Response> {
        self.send_with_timeout(method, path, payload, self.timeout)
    }

    fn send_with_timeout(
        &self,
        method: Method,
        path: &str,
        payload: Option<String>,
        timeout_seconds: u64,
    ) -> super::Result<Response> {
        let mut req = minreq::Request::new(method, format!("{}{path}", self.controller));
        if let Some(body) = payload {
            req = req
                .with_header("Content-Type", "application/json")
                .with_body(body);
        }
        if let Some(secret) = &self.secret {
            req = req.with_header(super::headers::AUTHORIZATION, format!("Bearer {secret}"));
        }
        let response = req.with_timeout(timeout_seconds).send()?;
        check_response(&response, self.secret.as_deref())?;
        Ok(response)
    }
}

pub fn spawn_blocking<F, R>(task: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let session = CoreSession::try_current();
    tokio::task::spawn_blocking(move || with_session(session, task))
}

pub(crate) fn with_session<T>(session: Option<CoreSession>, task: impl FnOnce() -> T) -> T {
    struct Reset(Option<CoreSession>);
    impl Drop for Reset {
        fn drop(&mut self) {
            THREAD_SESSION.with(|s| *s.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(THREAD_SESSION.with(|s| s.replace(session)));
    task()
}

pub(super) fn api_error(error: ApiError) -> minreq::Error {
    minreq::Error::IoError(std::io::Error::other(error))
}

fn check_response(response: &Response, secret: Option<&str>) -> super::Result<()> {
    if (200..300).contains(&response.status_code) {
        return Ok(());
    }
    let status = response.status_code as u16;
    let error = match status {
        401 | 403 => ApiError::Authentication(status),
        404 | 405 => ApiError::Unavailable(status),
        _ => {
            let body = response.as_str().unwrap_or("non-UTF-8 response");
            // Redact before truncation so a long credential cannot leave a
            // prefix in a displayed or logged error.
            let body = match secret.filter(|s| !s.is_empty()) {
                Some(secret) => body.replace(secret, "[redacted]"),
                None => body.to_owned(),
            };
            let message = body.chars().take(512).collect();
            ApiError::Http { status, message }
        }
    };
    Err(api_error(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (CoreSession, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let controller = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            responses.into_iter().map(|(status, body)| {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
                let mut request = Vec::new();
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") { break; }
                }
                let headers = String::from_utf8(request.clone()).unwrap();
                let length = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                }).unwrap_or(0);
                let mut body_bytes = vec![0; length];
                stream.read_exact(&mut body_bytes).unwrap();
                request.extend_from_slice(&body_bytes);
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                String::from_utf8(request).unwrap()
            }).collect()
        });
        (
            CoreSession {
                core_type: CoreType::Mihomo,
                controller,
                secret: Some("test-secret".to_owned()),
                timeout: 3,
                generation: None,
                identity: Arc::default(),
            },
            handle,
        )
    }

    #[test]
    fn validates_backend_before_mutation_and_sends_auth() {
        let (session, handle) = server(vec![
            ("200 OK", r#"{"meta":true,"version":"v1.19.0"}"#),
            ("204 No Content", ""),
        ]);
        session
            .request(Method::Delete, "/connections", None)
            .unwrap();
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /version "));
        assert!(requests[1].starts_with("DELETE /connections "));
        assert!(requests.iter().all(|r| {
            r.to_lowercase()
                .contains("authorization: bearer test-secret")
        }));
    }

    #[test]
    fn mismatched_backend_never_receives_mutation() {
        let (session, handle) = server(vec![("200 OK", r#"{"version":"sing-box 1.13.11"}"#)]);
        let error = session
            .request(Method::Delete, "/connections", None)
            .unwrap_err();
        assert!(error.to_string().contains("not a supported Mihomo backend"));
        assert_eq!(handle.join().unwrap().len(), 1);
    }

    #[test]
    fn http_failures_are_not_successful_mutations() {
        for status in [
            "401 Unauthorized",
            "403 Forbidden",
            "404 Not Found",
            "405 Method Not Allowed",
            "500 Internal Server Error",
        ] {
            let (session, handle) = server(vec![
                ("200 OK", r#"{"meta":true,"version":"v1.19.0"}"#),
                (status, "test-secret rejected"),
            ]);
            let error = session
                .request(Method::Delete, "/connections", None)
                .unwrap_err()
                .to_string();
            assert!(error.contains(&status[..3]));
            assert!(!error.contains("test-secret"));
            handle.join().unwrap();
        }
    }

    #[test]
    fn auth_failure_at_identity_check_retains_auth_error() {
        let (session, handle) = server(vec![("401 Unauthorized", "")]);
        let error = session
            .request(Method::Delete, "/connections", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("authentication"));
        assert!(!error.contains("mismatch"));
        handle.join().unwrap();
    }

    #[test]
    fn retired_session_cannot_send_any_request() {
        let (mut session, handle) = server(vec![]);
        session.generation = Some(u64::MAX);
        assert!(
            session
                .request(Method::Delete, "/connections", None)
                .unwrap_err()
                .to_string()
                .contains("Core changed")
        );
        assert!(handle.join().unwrap().is_empty());
    }

    #[test]
    fn mutation_readback_returns_effective_value_not_requested_value() {
        let (session, handle) = server(vec![
            ("200 OK", r#"{"meta":true,"version":"v1.19.0"}"#),
            ("204 No Content", ""),
            ("200 OK", r#"{"mode":"direct","allow-lan":false}"#),
        ]);
        let config = session
            .patch_and_fetch(r#"{"mode":"global"}"#.to_owned())
            .unwrap();
        assert_eq!(config.mode.to_string(), "Direct");
        let requests = handle.join().unwrap();
        assert!(requests[1].starts_with("PATCH /configs "));
        assert!(requests[1].ends_with(r#"{"mode":"global"}"#));
        assert!(requests[2].starts_with("GET /configs "));
    }
}
