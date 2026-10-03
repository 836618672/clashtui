use crate::config::CONFIG;
use minreq::Method;

pub mod config_struct;
pub mod core_detect;
pub mod metrics;
pub mod resources;
pub mod session;
pub mod stream;
#[macro_use]
mod utils;

use utils::*;

const DEFAULT_PAYLOAD: &str = r#"'{"path": "", "payload": ""}'"#;
const DEFAULT_TIMEOUT: u64 = 5;

mod headers {
    pub const USER_AGENT: &str = "user-agent";
    pub const AUTHORIZATION: &str = "authorization";
}

type Result<T, E = minreq::Error> = core::result::Result<T, E>;

pub mod control {
    use super::*;

    /// Restart clash core via http
    ///
    /// usually, an empty str is returned
    pub fn restart(payload: Option<String>) -> Result<()> {
        request(
            Method::Post,
            "/restart",
            Some(payload.unwrap_or(DEFAULT_PAYLOAD.to_string())),
        )
        .map(|_| ())
    }

    pub fn upgrade() -> Result<()> {
        request(Method::Post, "/upgrade", Some("{}".to_owned())).map(|_| ())
    }

    /// Get clash core version
    ///
    /// for mihomo, it's like `{"meta": true, "version": "v1.1.1"}`
    pub fn version() -> Result<String> {
        request(Method::Get, "/version", None).and_then(|r| r.as_str().map(|s| s.to_owned()))
    }
}

pub mod cache {
    use super::*;

    /// Flush fake-IP cache
    ///
    /// API: POST /cache/fakeip/flush
    pub fn flush_fakeip() -> Result<()> {
        request(Method::Post, "/cache/fakeip/flush", None).map(|_| ())
    }

    /// Flush DNS cache
    ///
    /// API: POST /cache/dns/flush
    pub fn flush_dns() -> Result<()> {
        request(Method::Post, "/cache/dns/flush", None).map(|_| ())
    }
}

pub mod geo {
    use super::*;

    /// Update GEO databases
    ///
    /// API: POST /configs/geo
    pub fn upgrade_geo() -> Result<()> {
        request(Method::Post, "/configs/geo", None).map(|_| ())
    }
}

pub mod config {
    use super::*;

    pub fn fetch() -> Result<config_struct::ClashConfig> {
        request(Method::Get, "/configs", None).and_then(|r| r.json())
    }

    pub fn fetch_raw() -> Result<serde_json::Value> {
        request(Method::Get, "/configs", None).and_then(|r| r.json())
    }

    pub const WRITABLE: &[&str] = &[
        "mode",
        "log-level",
        "allow-lan",
        "bind-address",
        "ipv6",
        "port",
        "socks-port",
        "redir-port",
        "tproxy-port",
        "mixed-port",
        "tun",
        "unified-delay",
        "tcp-concurrent",
        "find-process-mode",
        "global-client-fingerprint",
        "global-ua",
        "geodata-mode",
        "geo-auto-update",
        "geo-update-interval",
        "interface-name",
    ];

    pub fn writable_fields(
        current: &serde_json::Value,
        core: crate::config::CoreType,
    ) -> Vec<&'static str> {
        WRITABLE
            .iter()
            .copied()
            .filter(|key| {
                current.get(*key).is_some_and(|value| !value.is_null())
                    && (core == crate::config::CoreType::Mihomo || *key == "mode")
            })
            .collect()
    }

    pub fn patch_checked(patch: serde_json::Value) -> anyhow::Result<config_struct::ClashConfig> {
        let session = session::CoreSession::current();
        let current: serde_json::Value = session.request(Method::Get, "/configs", None)?.json()?;
        let fields = patch
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Patch must be a JSON object"))?;
        anyhow::ensure!(!fields.is_empty(), "Patch is empty");
        let available = writable_fields(&current, session.core_type());
        for key in fields.keys() {
            anyhow::ensure!(
                available.contains(&key.as_str()),
                "Field unavailable or read-only: {key}"
            );
        }
        Ok(session.patch_and_fetch(patch.to_string())?)
    }

    pub fn reload<S: AsRef<str>>(path: S) -> Result<String> {
        request(
            Method::Put,
            "/configs?force=true",
            Some(
                serde_json::json!({
                    "path": path.as_ref(),
                    "payload": ""
                })
                .to_string(),
            ),
        )
        .and_then(|r| {
            if r.status_code >= 200 && r.status_code < 300 {
                r.as_str().map(|s| s.to_owned())
            } else {
                let body = r.as_str().unwrap_or("(non-utf8 body)");
                Err(minreq::Error::IoError(std::io::Error::other(format!(
                    "HTTP {}: {body}",
                    r.status_code
                ))))
            }
        })
    }

    /// Local service managers may return before the controller accepts requests.
    pub fn wait_until_ready() -> Result<()> {
        let mut last_error = None;
        for attempt in 0..10 {
            let session = session::CoreSession::current();
            if !session.is_current() {
                return Err(minreq::Error::IoError(std::io::Error::other(
                    "Core session changed",
                )));
            }
            let ready = session
                .request(minreq::Method::Get, "/version", None)
                .and_then(|response| super::core_detect::parse_core_type(response.json()?))
                .and_then(|_| fetch());
            match ready {
                Ok(_) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
            if attempt < 9 {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        Err(last_error.expect("at least one controller read was attempted"))
    }

    pub fn patch_and_fetch(payload: String) -> Result<config_struct::ClashConfig> {
        let session = session::CoreSession::current();
        let config = session.patch_and_fetch(payload);
        if session.core_type() != CONFIG.core_type() {
            return Err(session::api_error(session::ApiError::SessionChanged));
        }
        config
    }
}

pub mod download {
    use super::*;

    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn base64_encode(input: &[u8]) -> String {
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
            out.push(B64[((n >> 18) & 0x3F) as usize] as char);
            out.push(B64[((n >> 12) & 0x3F) as usize] as char);
            out.push(if chunk.len() > 1 {
                B64[((n >> 6) & 0x3F) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                B64[(n & 0x3F) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    fn strip_userinfo(url: &str) -> (String, Option<String>) {
        let Some(scheme_end) = url.find("://") else {
            return (url.to_string(), None);
        };
        let rest = &url[(scheme_end + 3)..];
        let at_pos = rest.find('@');
        let slash_pos = rest.find('/');
        let is_in_authority = match (at_pos, slash_pos) {
            (Some(a), Some(s)) => a < s,
            (Some(_), None) => true,
            _ => false,
        };
        if !is_in_authority {
            return (url.to_string(), None);
        }
        let userinfo = &rest[..at_pos.unwrap()];
        let auth_value = if userinfo.contains(':') {
            userinfo.to_string()
        } else {
            format!("{userinfo}:")
        };
        let auth_header = format!("Basic {}", base64_encode(auth_value.as_bytes()));
        let prefix = &url[..(scheme_end + 3)];
        let suffix = &rest[(at_pos.unwrap() + 1)..];
        (format!("{prefix}{suffix}"), Some(auth_header))
    }

    pub fn profile(url: &str, with_proxy: bool) -> Result<minreq::ResponseLazy> {
        let (clean_url, auth_header) = strip_userinfo(url);
        let mut req = minreq::get(&clean_url);
        if with_proxy {
            req = req.with_proxy(minreq::Proxy::new(&CONFIG.proxy_addr)?)
        }
        req = req.with_timeout(timeout!()).with_header(
            headers::USER_AGENT,
            CONFIG.global_ua.as_deref().unwrap_or("clash.meta"),
        );
        if let Some(auth) = auth_header {
            req = req.with_header(headers::AUTHORIZATION, auth);
        }
        let response = req.send_lazy()?;
        if !(200..300).contains(&response.status_code) {
            return Err(minreq::Error::IoError(std::io::Error::other(format!(
                "Subscription download failed: HTTP {}",
                response.status_code
            ))));
        }
        Ok(response)
    }

    pub fn fetch_subscription_userinfo(url: &str, with_proxy: bool) -> Result<Option<String>> {
        let (clean_url, auth_header) = strip_userinfo(url);
        let mut req = minreq::get(&clean_url)
            .with_timeout(timeout!())
            .with_header(
                headers::USER_AGENT,
                CONFIG.global_ua.as_deref().unwrap_or("clash.meta"),
            );
        if with_proxy {
            req = req.with_proxy(minreq::Proxy::new(&CONFIG.proxy_addr)?);
        }
        if let Some(auth) = auth_header {
            req = req.with_header(headers::AUTHORIZATION, auth);
        }
        let resp = req.send()?;
        if !(200..300).contains(&resp.status_code) {
            return Err(minreq::Error::IoError(std::io::Error::other(format!(
                "Subscription metadata failed: HTTP {}",
                resp.status_code
            ))));
        }
        let info: Option<String> = resp.headers.get("subscription-userinfo").cloned();
        Ok(info)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn strip_token_from_github_url() {
            let url = "https://ghp_token@raw.githubusercontent.com/user/repo/main/config.yaml";
            let (clean, auth) = strip_userinfo(url);
            assert_eq!(
                clean,
                "https://raw.githubusercontent.com/user/repo/main/config.yaml"
            );
            assert_eq!(auth.unwrap(), "Basic Z2hwX3Rva2VuOg==");
        }

        #[test]
        fn strip_user_pass_from_url() {
            let url = "https://user:pass@example.com/path";
            let (clean, auth) = strip_userinfo(url);
            assert_eq!(clean, "https://example.com/path");
            assert_eq!(auth.unwrap(), "Basic dXNlcjpwYXNz");
        }

        #[test]
        fn no_userinfo_no_change() {
            let url = "https://example.com/path";
            let (clean, auth) = strip_userinfo(url);
            assert_eq!(clean, url);
            assert!(auth.is_none());
        }

        #[test]
        fn at_in_path_not_userinfo() {
            let url = "https://example.com/path?q=@test";
            let (clean, auth) = strip_userinfo(url);
            assert_eq!(clean, url);
            assert!(auth.is_none());
        }
    }
}

pub mod proxies;

pub mod connection {
    use super::*;

    use serde::Deserialize;

    #[cfg_attr(test, derive(Debug))]
    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    pub struct ConnInfo {
        pub download_total: u64,
        pub upload_total: u64,
        pub connections: Option<Vec<Conn>>,
    }

    #[derive(Debug, Clone, serde::Serialize, Deserialize)]
    pub struct Conn {
        #[serde(flatten)]
        pub extra: std::collections::HashMap<String, serde_json::Value>,
        pub id: String,
        pub metadata: ConnMetaData,
        pub upload: u64,
        pub download: u64,
        #[allow(dead_code)]
        pub start: String,
        pub chains: Vec<String>,
        #[serde(default)]
        pub rule: Option<String>,
        #[allow(dead_code)]
        #[serde(default, rename = "rulePayload")]
        pub rule_payload: Option<String>,
    }

    #[derive(Debug, Clone, serde::Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ConnMetaData {
        #[serde(flatten)]
        pub extra: std::collections::HashMap<String, serde_json::Value>,
        #[cfg_attr(not(test), allow(dead_code))]
        pub network: String,
        #[serde(rename = "type", default)]
        #[allow(dead_code)]
        pub ctype: String,
        pub host: String,
        #[serde(default)]
        #[allow(dead_code)]
        pub process: String,
        #[serde(default)]
        #[cfg_attr(not(test), allow(dead_code))]
        pub process_path: String,

        #[serde(rename = "sourceIP")]
        #[allow(dead_code)]
        pub source_ip: String,
        #[allow(dead_code)]
        pub source_port: String,
        #[serde(default)]
        pub remote_destination: String,
        #[serde(default, rename = "destinationPort")]
        pub destination_port: String,
        #[serde(default, rename = "destinationIP")]
        pub destination_ip: Option<String>,
        #[allow(dead_code)]
        #[serde(default, rename = "sniffHost")]
        pub sniff_host: Option<String>,
    }

    /// return [ConnInfo]
    pub fn get_connections() -> Result<ConnInfo> {
        request(Method::Get, "/connections", None).and_then(|r| r.json())
    }

    /// Terminate all active connections
    pub fn terminate_all_connections() -> Result<()> {
        request(Method::Delete, "/connections", None).map(|_| ())
    }

    /// Close an immutable selection; continue after an individual failure.
    pub fn terminate_ids(ids: &[String]) -> Vec<String> {
        ids.iter()
            .filter_map(|id| match terminate_connection(Some(id.clone())) {
                Ok(true) => None,
                Ok(false) => Some(format!("{id}: closure not confirmed")),
                Err(error) => Some(format!("{id}: {error}")),
            })
            .collect()
    }

    /// if `id` is some, will try to terminate that connection,
    /// otherwise try to terminate **all** connections.
    ///
    /// Return true on success
    ///
    /// NOTE:
    /// Empty str is returned if connection is terminated successfully
    pub fn terminate_connection(id: Option<String>) -> Result<bool> {
        request(
            Method::Delete,
            &format!(
                "/connections{}",
                id.map(|c| format!("/{}", resources::encode_path(&c)))
                    .unwrap_or_default()
            ),
            None,
        )
        .and_then(|r| {
            r.as_str().map(|s| {
                // try to catch failure
                log::debug!("terminate conn:{s}");
                s.is_empty()
            })
        })
    }
}

pub mod api_log {

    #[derive(Clone, serde::Serialize)]
    pub struct LogEntry {
        pub type_: String,
        pub payload: String,
        pub time: String,
    }

    pub(crate) fn timestamp() -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hh = time_of_day / 3600;
        let mm = (time_of_day % 3600) / 60;
        let ss = time_of_day % 60;

        let mut y: i64 = 1970;
        let mut remaining_days = days;
        loop {
            let days_in_year = if is_leap(y) { 366 } else { 365 };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            y += 1;
        }
        let dims: &[i64] = if is_leap(y) {
            &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };
        let mut mo = 1;
        for dim in dims {
            if remaining_days < *dim {
                break;
            }
            remaining_days -= dim;
            mo += 1;
        }
        let yy = y % 100;
        let dd = remaining_days + 1;
        format!("{yy:02}-{mo:02}-{dd:02} {hh:02}:{mm:02}:{ss:02}")
    }

    fn is_leap(y: i64) -> bool {
        (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn parse_log_entries(body: &str) -> Vec<LogEntry> {
        body.lines()
            .filter(|line| !line.is_empty())
            .filter_map(
                |line| match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(v) => {
                        let type_ = v
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("unknown")
                            .to_owned();
                        let payload = v
                            .get("payload")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_owned();
                        Some(LogEntry {
                            type_,
                            payload,
                            time: timestamp(),
                        })
                    }
                    Err(_) => {
                        log::warn!("Failed to parse log line as JSON: {line}");
                        None
                    }
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod connection_tests {}
