//! Browser core operations reuse the same authenticated session as CLI and TUI.
use crate::functions::restful::{
    self,
    resources::{self, ResourceKind},
    session::CoreSession,
};
use anyhow::{Context, Result};
use minreq::Method;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tungstenite::Message;

#[derive(Default)]
struct Frames {
    generation: String,
    traffic: Value,
    memory: Value,
    connections: Value,
    logs: VecDeque<Value>,
    sequence: u64,
    status: std::collections::BTreeMap<String, Value>,
}
#[derive(Clone, Default)]
pub struct Streams(Arc<Mutex<Frames>>);
impl Streams {
    pub fn start() -> Self {
        let hub = Self::default();
        hub.0.lock().unwrap().generation = format!("{:016x}", fastrand::u64(..));
        for (kind, path) in [
            ("traffic", "/traffic"),
            ("memory", "/memory"),
            ("connections", "/connections"),
            ("logs", "/logs?level=debug"),
        ] {
            let hub = hub.clone();
            std::thread::spawn(move || {
                loop {
                    let session = CoreSession::current();
                    let outcome = (|| -> Result<()> {
                        let mut socket =
                            restful::stream::connect(session.controller(), session.secret(), path)?;
                        hub.0.lock().unwrap().status.insert(
                            kind.to_owned(),
                            json!({"connected":true,"updated_at":now_ms()}),
                        );
                        loop {
                            match socket.read() {
                                Ok(Message::Text(text)) => {
                                    anyhow::ensure!(session.is_current(), "Core session changed");
                                    let mut value: Value = serde_json::from_str(&text)?;
                                    sanitize(&mut value, session.secret());
                                    if kind == "logs" {
                                        value = bounded_log(value);
                                    }
                                    let mut f = hub.0.lock().unwrap();
                                    f.status.insert(
                                        kind.to_owned(),
                                        json!({"connected":true,"updated_at":now_ms()}),
                                    );
                                    match kind {
                                        "traffic" => f.traffic = value,
                                        "memory" => f.memory = value,
                                        "connections" => f.connections = value,
                                        _ => {
                                            f.sequence += 1;
                                            let id = f.sequence;
                                            f.logs.push_back(
                                                json!({"id":id,"time":now_ms(),"value":value}),
                                            );
                                            while f.logs.len() > 1000 {
                                                f.logs.pop_front();
                                            }
                                        }
                                    }
                                }
                                Ok(Message::Close(_)) => anyhow::bail!("Core stream closed"),
                                Ok(_) => {}
                                Err(tungstenite::Error::Io(e))
                                    if matches!(
                                        e.kind(),
                                        std::io::ErrorKind::WouldBlock
                                            | std::io::ErrorKind::TimedOut
                                    ) =>
                                {
                                    anyhow::ensure!(session.is_current(), "Core session changed");
                                }
                                Err(e) => return Err(e.into()),
                            }
                        }
                    })();
                    let error = outcome
                        .err()
                        .map(|e| redact(&e.to_string(), session.secret()));
                    {
                        let mut f = hub.0.lock().unwrap();
                        f.status.insert(
                            kind.to_owned(),
                            json!({"connected":false,"error":error,"updated_at":now_ms()}),
                        );
                        match kind {
                            "traffic" => f.traffic = Value::Null,
                            "memory" => f.memory = Value::Null,
                            "connections" => f.connections = Value::Null,
                            _ => {}
                        }
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            });
        }
        hub
    }
    pub fn snapshot(&self, after: u64, generation: Option<&str>) -> Value {
        let f = self.0.lock().unwrap();
        let reset = generation.is_some_and(|g| g != f.generation);
        let after = if reset { 0 } else { after };
        json!({"generation":f.generation,"reset":reset,"traffic":f.traffic,"memory":f.memory,"connections":f.connections,"logs":f.logs.iter().filter(|row|row["id"].as_u64().unwrap_or(0)>after).collect::<Vec<_>>(),"cursor":f.sequence,"dropped":f.logs.front().is_some_and(|row|after>0 && row["id"].as_u64().unwrap_or(0)>after.saturating_add(1)),"status":f.status})
    }
}
fn bounded_log(value: Value) -> Value {
    if value.to_string().len() <= 8192 {
        return value;
    }
    json!({"type":value["type"].as_str().unwrap_or("info").chars().take(32).collect::<String>(),"payload":value["payload"].as_str().unwrap_or("Oversized log frame").chars().take(1024).collect::<String>() + " [truncated]","truncated":true})
}
fn sanitize(value: &mut Value, secret: Option<&str>) {
    match value {
        Value::String(text) => *text = redact(text, secret),
        Value::Array(rows) => {
            for row in rows {
                sanitize(row, secret);
            }
        }
        Value::Object(fields) => {
            for row in fields.values_mut() {
                sanitize(row, secret);
            }
        }
        _ => {}
    }
}
fn redact(text: &str, secret: Option<&str>) -> String {
    match secret.filter(|s| !s.is_empty()) {
        Some(s) => text.replace(s, "[redacted]"),
        None => text.to_owned(),
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("Missing {key}"))
}
fn kind(value: &Value) -> Result<ResourceKind> {
    match text(value, "kind")? {
        "rules" => Ok(ResourceKind::Rules),
        "proxy-providers" => Ok(ResourceKind::ProxyProviders),
        "rule-providers" => Ok(ResourceKind::RuleProviders),
        _ => anyhow::bail!("Unknown resource kind"),
    }
}
fn items(kind: ResourceKind) -> Result<Value> {
    Ok(json!(
        resources::fetch(kind)?
            .into_iter()
            .map(|r| json!({"id":r.id,"value":r.value}))
            .collect::<Vec<_>>()
    ))
}
pub fn read(value: &Value, streams: &Streams) -> Result<Value> {
    let session = CoreSession::current();
    match text(value, "view")? {
        "status" => Ok(
            json!({"version":session.request(Method::Get,"/version",None)?.json::<Value>()?,"runtime":runtime()?}),
        ),
        "runtime" => runtime(),
        "proxies" => Ok(session.request(Method::Get, "/proxies", None)?.json()?),
        "resources" => items(kind(value)?),
        "connections" => Ok(session.request(Method::Get, "/connections", None)?.json()?),
        "streams" => Ok(streams.snapshot(
            value["after"].as_u64().unwrap_or(0),
            value["generation"].as_str(),
        )),
        "dns" => {
            let name = resources::encode_path(text(value, "name")?);
            let record = value["type"].as_str().unwrap_or("A");
            anyhow::ensure!(
                ["A", "AAAA", "HTTPS", "TXT", "MX", "NS", "CNAME"].contains(&record),
                "Unsupported DNS record type"
            );
            Ok(session
                .request(
                    Method::Get,
                    &format!("/dns/query?name={name}&type={record}"),
                    None,
                )?
                .json()?)
        }
        _ => anyhow::bail!("Unknown core view"),
    }
}
fn runtime() -> Result<Value> {
    let mut raw = restful::config::fetch_raw()?;
    if let Some(fields) = raw.as_object_mut() {
        fields.remove("secret");
        fields.remove("external-controller");
        fields.remove("external-controller-tls");
    }
    Ok(
        json!({"available":restful::config::writable_fields(&raw,crate::config::CONFIG.core_type()),"config":raw}),
    )
}
fn check_patch_conflicts(patch: &Value, expected: &Value, current: &Value) -> Result<()> {
    let fields = patch.as_object().context("Patch must be an object")?;
    let baseline = expected
        .as_object()
        .context("Expected settings must be an object")?;
    for field in fields.keys() {
        anyhow::ensure!(
            baseline.contains_key(field),
            "Missing expected value for {field}"
        );
        anyhow::ensure!(
            current.get(field) == baseline.get(field),
            "Revision conflict: runtime setting {field} changed; re-read before applying"
        );
    }
    Ok(())
}

pub fn execute(value: Value) -> Result<Value> {
    anyhow::ensure!(
        value["core"].as_str() == Some("mihomo"),
        "Core session changed; refresh before retrying"
    );
    let session = CoreSession::current();
    match text(&value, "operation")? {
        "select" => {
            let group = text(&value, "group")?;
            let node = text(&value, "node")?;
            let all: Value = session.request(Method::Get, "/proxies", None)?.json()?;
            anyhow::ensure!(
                all["proxies"][group]["all"]
                    .as_array()
                    .is_some_and(|nodes| nodes.iter().any(|n| n.as_str() == Some(node))),
                "Node is not in this group"
            );
            restful::proxies::select_proxy(group, node)?;
            Ok(json!({"selected":node}))
        }
        "unfix" => {
            restful::proxies::unfix_proxy(text(&value, "group")?)?;
            Ok(json!({"ok":true}))
        }
        "delay" => {
            let name = text(&value, "name")?;
            let timeout = value["timeout"].as_u64().unwrap_or(5000);
            anyhow::ensure!((1..=3_600_000).contains(&timeout), "Invalid delay timeout");
            let url = value["url"].as_str();
            if let Some(provider) = value["provider"].as_str() {
                Ok(
                    json!({"name":name,"delay":restful::proxies::test_provider_node_delay(provider,name,url,timeout)?}),
                )
            } else if value["group"] == true {
                Ok(json!(restful::proxies::test_group_delay(
                    name, url, timeout
                )?))
            } else {
                Ok(
                    json!({"name":name,"delay":restful::proxies::test_proxy_delay(name,url,timeout)?}),
                )
            }
        }
        "resource" => {
            let kind = kind(&value)?;
            let all = resources::fetch(kind)?;
            let action = value["resource_action"].as_str().unwrap_or("update");
            anyhow::ensure!(
                ["update", "update-all", "health"].contains(&action),
                "Unknown resource action"
            );
            anyhow::ensure!(
                action != "update-all" || kind != ResourceKind::Rules,
                "Toggle rules individually"
            );
            let selected = if action == "update-all" {
                all
            } else {
                vec![
                    all.into_iter()
                        .find(|r| Some(r.id.as_str()) == value["name"].as_str())
                        .context("Resource not found")?,
                ]
            };
            let results: Vec<Value> = selected
                .into_iter()
                .map(|r| {
                    let result = if action == "health" {
                        if kind == ResourceKind::ProxyProviders {
                            resources::healthcheck(&r.id)
                        } else {
                            Err(anyhow::anyhow!("Health checks require a proxy Provider"))
                        }
                    } else {
                        resources::update(kind, &r.id, r.value["disabled"].as_bool())
                    };
                    match result {
                        Ok(()) => json!({"id":r.id,"ok":true}),
                        Err(e) => json!({"id":r.id,"ok":false,"error":e.to_string()}),
                    }
                })
                .collect();
            Ok(
                json!({"partial_failure":results.iter().any(|r|r["ok"]==false),"results":results,"current":items(kind)?}),
            )
        }
        "close-connections" => {
            let ids = value["ids"]
                .as_array()
                .context("Supply captured connection IDs")?;
            anyhow::ensure!(ids.len() <= 10000, "Too many connections");
            let ids: Vec<String> = ids
                .iter()
                .map(|id| {
                    id.as_str()
                        .map(str::to_owned)
                        .context("Invalid connection ID")
                })
                .collect::<Result<_>>()?;
            let failures = restful::connection::terminate_ids(&ids);
            Ok(
                json!({"captured_ids":ids,"partial_failure":!failures.is_empty(),"failures":failures}),
            )
        }
        "patch" => {
            if let Some(expected) = value.get("expected") {
                check_patch_conflicts(&value["patch"], expected, &runtime()?["config"])?;
            }
            let ids = if value["close_connections"] == true {
                restful::connection::get_connections()?
                    .connections
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| c.id)
                    .collect::<Vec<_>>()
            } else {
                vec![]
            };
            let patch = value["patch"].clone();
            restful::config::patch_checked(patch)?;
            if value["close_connections"] == true {
                let failures = restful::connection::terminate_ids(&ids);
                return Ok(
                    json!({"config":runtime()?,"partial_failure":!failures.is_empty(),"failures":failures}),
                );
            }
            runtime()
        }
        "maintenance" => {
            match text(&value, "name")? {
                "flush-dns" => restful::cache::flush_dns()?,
                "flush-fakeip" => restful::cache::flush_fakeip()?,
                "upgrade-geo" => restful::geo::upgrade_geo()?,
                "restart" => {
                    restful::control::restart(None)?;
                    restful::config::wait_until_ready()?
                }
                "upgrade" => {
                    restful::control::upgrade()?;
                    restful::config::wait_until_ready()?
                }
                _ => anyhow::bail!("Unknown maintenance operation"),
            };
            Ok(json!({"accepted":true}))
        }
        _ => anyhow::bail!("Unknown core operation"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setting_conflicts_check_only_changed_fields_and_reject_incomplete_baselines() {
        let patch = json!({"log-level":"warning"});
        let expected = json!({"log-level":"info"});
        assert!(
            check_patch_conflicts(
                &patch,
                &expected,
                &json!({"log-level":"info","mode":"global"})
            )
            .is_ok()
        );
        assert!(
            check_patch_conflicts(&patch, &expected, &json!({"log-level":"error"}))
                .unwrap_err()
                .to_string()
                .contains("Revision conflict")
        );
        assert!(check_patch_conflicts(&patch, &json!({}), &json!({"log-level":"info"})).is_err());
        assert!(
            check_patch_conflicts(
                &json!({"tun":{"enable":true}}),
                &json!({"tun":{"enable":false,"stack":"mixed"}}),
                &json!({"tun":{"enable":false,"stack":"system"}})
            )
            .is_err()
        );
    }
    #[test]
    fn oversized_log_extensions_are_bounded_and_escaped_secrets_are_redacted() {
        let mut value = json!({"type":"info","payload":"quoted-secret: a\"b","detail":{"nested":"a\"b","large":"x".repeat(20000)}});
        sanitize(&mut value, Some("a\"b"));
        assert_eq!(value["detail"]["nested"], "[redacted]");
        let row = bounded_log(value);
        assert_eq!(row["truncated"], true);
        assert!(row.to_string().len() <= 8192);
        assert_eq!(redact("{}", Some("")), "{}");
    }
    #[test]
    fn stream_buffer_is_bounded_and_reports_dropped_rows() {
        let hub = Streams::default();
        let mut f = hub.0.lock().unwrap();
        f.sequence = 1004;
        for id in 5..=1004 {
            f.logs.push_back(json!({"id":id,"value":{"payload":"x"}}));
        }
        drop(f);
        let s = hub.snapshot(1, None);
        assert_eq!(s["logs"].as_array().unwrap().len(), 1000);
        assert_eq!(s["dropped"], true);
        assert_eq!(hub.snapshot(1004, None)["logs"], json!([]));
        let restarted = hub.snapshot(1004, Some("retired-cache"));
        assert_eq!(restarted["reset"], true);
        assert_eq!(restarted["logs"].as_array().unwrap().len(), 1000);
    }
    #[test]
    fn rejects_unlisted_resource_kind() {
        assert!(kind(&json!({"kind":"../../configs"})).is_err());
    }
}
