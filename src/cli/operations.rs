//! Noninteractive entry points for the same workflows used by TUI and Web.
use super::*;
use crate::functions::{file, management, restful};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::Write;

pub fn print_json(value: &Value) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

pub fn manage(command: ArgCommand) -> Result<()> {
    let ArgCommand::Manage {
        action,
        name,
        kind,
        input,
        url,
        new_name,
        template,
        revision,
        with_proxy,
        yes,
    } = command
    else {
        anyhow::bail!("Expected a management command");
    };
    let action = action.name();
    if matches!(
        action,
        "delete"
            | "delete_template"
            | "activate"
            | "persist"
            | "prepare_panel"
            | "stop_all"
            | "stop"
            | "restart"
            | "start"
            | "update_all"
    ) {
        anyhow::ensure!(yes, "This operation requires --yes");
    }
    let _write = crate::functions::file::coordination::WriteGuard::acquire()?;
    let state = management::snapshot()?;
    if action == "generate"
        && state["profiles"].as_array().is_some_and(|profiles| {
            profiles
                .iter()
                .any(|pf| pf["name"].as_str() == name.as_deref())
        })
    {
        anyhow::ensure!(yes, "Replacing a generated profile requires --yes");
    }
    let mut request = json!({"action":action,"name":name.unwrap_or_default(),"kind":kind,"revision":state["revision"],"core":state["core"]});
    if let Some(url) = url {
        request["url"] = json!(url);
    }
    if let Some(name) = new_name {
        request["new_name"] = json!(name);
    }
    if let Some(template) = template {
        request["template"] = json!(template);
    }
    if let Some(proxy) = with_proxy {
        request["with_proxy"] = json!(proxy);
    }
    if matches!(
        action,
        "save" | "save_template_providers" | "delete_template"
    ) {
        let revision = revision
            .context("Supply --revision from read/template_providers; reload after a conflict")?;
        if action == "save" {
            request["revision"] = json!(revision);
        } else {
            request["document_revision"] = json!(revision);
        }
    }
    if let Some(path) = input {
        let content = std::fs::read_to_string(path)?;
        match action {
            "patch" => request["patch"] = serde_json::from_str(&content)?,
            "save_template_providers" => request["groups"] = serde_json::from_str(&content)?,
            _ => request["content"] = json!(content),
        }
    }
    let result = management::execute(request)?;
    print_json(&result)?;
    anyhow::ensure!(
        result["valid"] != false,
        "Configuration validation failed; see JSON diagnostics"
    );
    anyhow::ensure!(
        result["partial_failure"] != true,
        "Some subscription resources failed; see JSON results"
    );
    anyhow::ensure!(
        !result["results"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["ok"] == false)),
        "Some batch operations failed; see JSON results"
    );
    Ok(())
}

fn resource_kind(kind: CoreResource) -> restful::resources::ResourceKind {
    use restful::resources::ResourceKind;
    match kind {
        CoreResource::Rules => ResourceKind::Rules,
        CoreResource::ProxyProviders => ResourceKind::ProxyProviders,
        CoreResource::RuleProviders => ResourceKind::RuleProviders,
    }
}

pub fn core(command: CoreCommand) -> Result<()> {
    let value = match command {
        CoreCommand::Status => {
            let session = restful::session::CoreSession::current();
            json!({"version": session.request(minreq::Method::Get,"/version",None)?.json::<Value>()?, "config": restful::config::fetch_raw()?})
        }
        CoreCommand::Proxies => restful::session::CoreSession::current()
            .request(minreq::Method::Get, "/proxies", None)?
            .json()?,
        CoreCommand::Unfix { group } => {
            restful::proxies::unfix_proxy(&group)?;
            restful::session::CoreSession::current()
                .request(minreq::Method::Get, "/proxies", None)?
                .json()?
        }
        CoreCommand::Select { group, node } => {
            restful::proxies::select_proxy(&group, &node)?;
            restful::session::CoreSession::current()
                .request(minreq::Method::Get, "/proxies", None)?
                .json()?
        }
        CoreCommand::Delay {
            name,
            provider,
            group,
            url,
            timeout,
        } => {
            anyhow::ensure!(timeout > 0, "Timeout must be positive");
            if let Some(provider) = provider {
                json!({"provider":provider,"name":name,"delay":restful::proxies::test_provider_node_delay(&provider,&name,url.as_deref(),timeout)?})
            } else if group {
                json!(restful::proxies::test_group_delay(
                    &name,
                    url.as_deref(),
                    timeout
                )?)
            } else {
                json!({"name":name,"delay":restful::proxies::test_proxy_delay(&name,url.as_deref(),timeout)?})
            }
        }
        CoreCommand::Resources {
            kind,
            action,
            name,
            yes,
        } => {
            let kind = resource_kind(kind);
            let items = restful::resources::fetch(kind)?;
            if matches!(action, ResourceAction::List) {
                json!(
                    items
                        .into_iter()
                        .filter(|item| name.as_ref().is_none_or(|name| name == &item.id))
                        .map(|item| json!({"id":item.id,"value":item.value}))
                        .collect::<Vec<_>>()
                )
            } else {
                anyhow::ensure!(yes, "This operation requires --yes");
                anyhow::ensure!(
                    !matches!(action, ResourceAction::UpdateAll)
                        || kind != restful::resources::ResourceKind::Rules,
                    "Rules must be toggled individually by index"
                );
                let selected: Vec<_> = if matches!(action, ResourceAction::UpdateAll) {
                    items
                } else {
                    let name = name.context("Supply --name (rule API index or Provider name)")?;
                    vec![
                        items
                            .into_iter()
                            .find(|item| item.id == name)
                            .context("Resource not found")?,
                    ]
                };
                let mut results = Vec::new();
                for item in selected {
                    let result = if matches!(action, ResourceAction::Health) {
                        anyhow::ensure!(
                            kind == restful::resources::ResourceKind::ProxyProviders,
                            "Health checks require a proxy Provider"
                        );
                        restful::resources::healthcheck(&item.id)
                    } else {
                        restful::resources::update(kind, &item.id, item.value["disabled"].as_bool())
                    };
                    results.push(match result {
                        Ok(()) => json!({"id":item.id,"ok":true}),
                        Err(error) => json!({"id":item.id,"ok":false,"error":error.to_string()}),
                    });
                }
                let failed = results.iter().any(|row| row["ok"] == false);
                let (current, readback_error) = match restful::resources::fetch(kind) {
                    Ok(items) => (
                        json!(
                            items
                                .into_iter()
                                .map(|item| json!({"id":item.id,"value":item.value}))
                                .collect::<Vec<_>>()
                        ),
                        None,
                    ),
                    Err(error) => (Value::Null, Some(error.to_string())),
                };
                let value =
                    json!({"results":results,"current":current,"readback_error":readback_error});
                print_json(&value)?;
                anyhow::ensure!(!failed, "Some resource operations failed; see JSON results");
                anyhow::ensure!(
                    readback_error.is_none(),
                    "Operations finished, but state readback failed; see JSON results"
                );
                return Ok(());
            }
        }
        CoreCommand::Connections {
            filter,
            close,
            id,
            yes,
        } => {
            let all = restful::connection::get_connections()?;
            let connections: Vec<_> = all
                .connections
                .unwrap_or_default()
                .into_iter()
                .filter(|conn| {
                    id.as_ref().is_none_or(|id| id == &conn.id)
                        && matches_connection(conn, filter.as_deref())
                })
                .collect();
            if close {
                anyhow::ensure!(
                    yes,
                    "Closing connections requires --yes; --filter limits the captured IDs"
                );
                let ids: Vec<_> = connections.iter().map(|conn| conn.id.clone()).collect();
                let failures = restful::connection::terminate_ids(&ids);
                print_json(&json!({"captured_ids":ids,"failures":failures}))?;
                anyhow::ensure!(failures.is_empty(), "Some connections could not be closed");
                return Ok(());
            }
            json!({"downloadTotal":all.download_total,"uploadTotal":all.upload_total,"connections":connections})
        }
        CoreCommand::Logs {
            level,
            filter,
            limit,
            seconds,
            output,
        } => {
            anyhow::ensure!(
                (1..=10000).contains(&limit) && (1..=3600).contains(&seconds),
                "Use limit 1–10000 and seconds 1–3600"
            );
            let value = capture_logs(&level, filter.as_deref(), limit, seconds)?;
            if let Some(path) = output {
                file::export_json(&path, &value)?;
            }
            value
        }
        CoreCommand::Metrics {
            samples,
            interval_ms,
        } => {
            anyhow::ensure!(
                (1..=120).contains(&samples) && (100..=60000).contains(&interval_ms),
                "Use samples 1–120 and interval-ms 100–60000"
            );
            let mut metrics = restful::metrics::Metrics::default();
            let mut rows = Vec::new();
            for index in 0..samples {
                let connections = restful::connection::get_connections()?;
                metrics.sample(&connections, std::time::Instant::now());
                rows.push(json!({"downloadTotal":connections.download_total,"uploadTotal":connections.upload_total,"connection_count":metrics.connections,"session_download":metrics.download,"session_upload":metrics.upload,"download_bytes_per_second":metrics.download_speed,"upload_bytes_per_second":metrics.upload_speed}));
                if index + 1 < samples {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                }
            }
            json!({"samples":rows,"combined_rate_history":metrics.history,"memory":restful::stream::memory().ok(),"scope":"core counters and this CLI capture session"})
        }
        CoreCommand::Maintenance { action, yes } => {
            anyhow::ensure!(yes, "Maintenance requires --yes");
            anyhow::ensure!(
                crate::config::CONFIG.core_type() == crate::config::CoreType::Mihomo,
                "These maintenance APIs require Mihomo"
            );
            match action {
                MaintenanceAction::Restart => restful::control::restart(None)?,
                MaintenanceAction::Upgrade => restful::control::upgrade()?,
                MaintenanceAction::FlushDns => restful::cache::flush_dns()?,
                MaintenanceAction::FlushFakeip => restful::cache::flush_fakeip()?,
                MaintenanceAction::UpgradeGeo => restful::geo::upgrade_geo()?,
            }
            json!({"accepted":true,"message":"Verify core state/version after maintenance"})
        }
    };
    print_json(&value)
}

fn matches_connection(conn: &restful::connection::Conn, filter: Option<&str>) -> bool {
    filter.is_none_or(|filter| {
        serde_json::to_string(conn)
            .unwrap_or_default()
            .to_lowercase()
            .contains(&filter.to_lowercase())
    })
}

fn capture_logs(level: &str, filter: Option<&str>, limit: usize, seconds: u64) -> Result<Value> {
    let session = restful::session::CoreSession::current();
    session.request(minreq::Method::Get, "/configs", None)?;
    let path = format!("/logs?level={}", restful::resources::encode_path(level));
    let mut socket = restful::stream::connect(session.controller(), session.secret(), &path)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut entries = Vec::new();
    while entries.len() < limit && std::time::Instant::now() < deadline {
        anyhow::ensure!(
            session.is_current(),
            "Core session changed during log capture"
        );
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                for entry in restful::api_log::parse_log_entries(&text) {
                    if filter.is_none_or(|query| {
                        serde_json::to_string(&entry)
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                    }) {
                        entries.push(entry);
                    }
                    if entries.len() == limit {
                        break;
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                return Err(anyhow::anyhow!(
                    error
                        .to_string()
                        .replace(session.secret().unwrap_or("\0"), "[redacted]")
                ));
            }
        }
    }
    let _ = socket.close(None);
    Ok(json!({"entries":entries,"level":level,"bounded":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, ValueEnum};

    #[test]
    fn every_management_action_has_a_matching_cli_spelling() {
        for action in ManagementAction::value_variants() {
            let possible = action.to_possible_value().unwrap();
            assert_eq!(possible.get_name(), action.name());
            let parsed = Cmds::try_parse_from(["clashtui", "manage", possible.get_name()]).unwrap();
            assert!(matches!(parsed.command, Some(ArgCommand::Manage { .. })));
        }
    }

    #[test]
    fn cli_exposes_revision_protection_and_bounded_core_operations() {
        assert!(
            Cmds::try_parse_from([
                "clashtui",
                "manage",
                "save",
                "--name",
                "one",
                "--input",
                "config.yaml",
                "--revision",
                "old-document"
            ])
            .is_ok()
        );
        assert!(Cmds::try_parse_from(["clashtui", "manage", "restart", "--yes"]).is_ok());
        assert!(
            Cmds::try_parse_from([
                "clashtui",
                "core",
                "resources",
                "rules",
                "update",
                "--name",
                "10",
                "--yes"
            ])
            .is_ok()
        );
        assert!(
            Cmds::try_parse_from([
                "clashtui",
                "core",
                "connections",
                "--filter",
                "worker.exe",
                "--close",
                "--yes"
            ])
            .is_ok()
        );
        assert!(
            Cmds::try_parse_from([
                "clashtui",
                "core",
                "connections",
                "--id",
                "one",
                "--filter",
                "worker"
            ])
            .is_err()
        );
        assert!(
            Cmds::try_parse_from([
                "clashtui",
                "core",
                "logs",
                "--seconds",
                "10",
                "--limit",
                "100",
                "--output",
                "logs.json"
            ])
            .is_ok()
        );
        assert!(Cmds::try_parse_from(["clashtui", "mode", "set", "custom-mode"]).is_ok());
        assert!(Cmds::try_parse_from(["clashtui", "mode", "rule", "--close-connections"]).is_ok());
    }

    #[test]
    fn connection_filter_keeps_unknown_metadata_and_matches_case_insensitively() {
        let info: restful::connection::ConnInfo =
            serde_json::from_str(include_str!("../../tests/apidata/mihomo/connections.json"))
                .unwrap();
        let mut conn = info.connections.unwrap().into_iter().next().unwrap();
        conn.metadata
            .extra
            .insert("future-field".to_owned(), json!("Unique-Worker-Tag"));
        assert!(matches_connection(&conn, Some("unique-worker-tag")));
        assert!(!matches_connection(&conn, Some("another-worker-tag")));
        assert!(matches_connection(&conn, None));
    }
}
