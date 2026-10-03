//! Local workflows shared by the terminal and its Web companion.
use crate::config::{CONFIG, CoreType};
use crate::functions::file::{self, profile, template};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub fn revision(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn document_path(kind: &str, name: &str) -> Result<PathBuf> {
    let path = match kind {
        "override" => Ok::<_, anyhow::Error>(match CONFIG.core_type() {
            CoreType::Mihomo => {
                crate::config::config_dir_path().join("mihomo/core_override_config.yaml")
            }
        }),
        "profile" => {
            profile::validate_profile_name(name)?;
            anyhow::ensure!(profile::db::get(name).is_some(), "Profile not found");
            Ok(profile::local_profile_path(name))
        }
        "template" => {
            profile::validate_profile_name(name)?;
            let dir = match CONFIG.core_type() {
                CoreType::Mihomo => crate::config::template_path(),
            };
            Ok(dir.join(name))
        }
        _ => anyhow::bail!("Unknown document type"),
    }?;
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Symlink documents are not managed through the Web API"
        );
    }
    Ok(path)
}

pub fn read_document(kind: &str, name: &str) -> Result<Value> {
    let path = document_path(kind, name)?;
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && kind == "template" => {
            String::new()
        }
        Err(error) => return Err(error.into()),
    };
    Ok(json!({"content": content, "revision": revision(content.as_bytes())}))
}

pub fn save_document(kind: &str, name: &str, content: &str, expected: &str) -> Result<Value> {
    let _write = file::coordination::WriteGuard::acquire()?;
    let path = document_path(kind, name)?;
    save_document_at(&path, content, expected, false)?;
    read_document(kind, name)
}

fn save_document_at(path: &Path, content: &str, expected: &str, json_format: bool) -> Result<()> {
    let old = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        revision(&old) == expected,
        "Revision conflict: reload the document before saving"
    );
    if json_format {
        let value: Value = serde_json::from_str(content)?;
        anyhow::ensure!(value.is_object(), "Configuration must be an object");
    } else {
        let _: serde_yml::Mapping = serde_yml::from_str(content)?;
    }
    file::activation::atomic_write(path, content.as_bytes())
}

pub fn snapshot() -> Result<Value> {
    let _write = file::coordination::WriteGuard::try_acquire()?;
    file::coordination::database().ensure_loaded()?;
    let profiles: Vec<Value> = profile::db::get_all().into_iter().map(|pf| json!({
        "name": pf.name, "url": match &pf.dtype { crate::config::database::ProfileType::Url(url) => Some(url), _ => None }, "type": pf.dtype, "no_pp": pf.no_pp, "update_with_proxy": pf.update_with_proxy,
    })).collect();
    let db = std::fs::read(crate::config::config_dir_path().join("clashtui.db"))?;
    Ok(
        json!({"core": CONFIG.core_type().to_string(), "profiles": profiles,
        "current": profile::db::get_current().name, "templates": template::get_all_templates()?,
        "revision": revision(&db), "service_running": crate::functions::command::is_core_service_running(),
        "metacubexd": file::panel::VERSION, "panel_deployment": file::panel::deployment_state(), "system_proxy": system_proxy_state(), "service_install": cfg!(windows), "panel": format!("{}/ui/", CONFIG.controller_for_core().trim_end_matches('/'))}),
    )
}

fn system_proxy_state() -> Option<bool> {
    #[cfg(windows)]
    {
        super::command::get_system_proxy_state().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing string field: {key}"))
}

pub fn persist_runtime() -> Result<Value> {
    let _write = file::coordination::WriteGuard::acquire()?;
    let current = super::restful::config::fetch_raw()?;
    let document = read_document("override", "")?;
    let mut overlay: Value = match CONFIG.core_type() {
        CoreType::Mihomo => serde_json::to_value(serde_yml::from_str::<serde_yml::Mapping>(
            text(&document, "content")?,
        )?)?,
    };
    merge_runtime(&current, &mut overlay, CONFIG.core_type())?;
    let content = match CONFIG.core_type() {
        CoreType::Mihomo => serde_yml::to_string(&overlay)?,
    };
    save_document("override", "", &content, text(&document, "revision")?)
}

fn merge_runtime(current: &Value, overlay: &mut Value, core: CoreType) -> Result<()> {
    anyhow::ensure!(
        overlay.is_object(),
        "Override configuration must be an object"
    );
    match core {
        CoreType::Mihomo => {
            for key in super::restful::config::WRITABLE {
                if let Some(value) = current.get(*key).filter(|v| !v.is_null()) {
                    if *key == "tun" {
                        let tun = object_field(overlay, "tun")?;
                        for field in ["enable", "stack"] {
                            if let Some(value) = value.get(field) {
                                tun[field] = value.clone();
                            }
                        }
                    } else {
                        overlay[*key] = value.clone();
                    }
                }
            }
        }
    }
    Ok(())
}

fn object_field<'a>(value: &'a mut Value, key: &str) -> Result<&'a mut Value> {
    let field = value
        .as_object_mut()
        .context("Configuration section must be an object")?
        .entry(key)
        .or_insert_with(|| json!({}));
    if field.is_null() {
        *field = json!({});
    }
    anyhow::ensure!(
        field.is_object(),
        "Configuration section {key} must be an object"
    );
    Ok(field)
}

pub fn execute(value: Value) -> Result<Value> {
    let _write = file::coordination::WriteGuard::acquire()?;
    file::coordination::database().ensure_loaded()?;
    anyhow::ensure!(
        super::restful::session::CoreSession::current().is_current(),
        "Core session changed: refresh and retry"
    );
    if let Some(expected_core) = value["core"].as_str() {
        anyhow::ensure!(
            expected_core == CONFIG.core_type().to_string(),
            "Core changed: refresh before applying this action"
        );
    }
    let action = text(&value, "action")?;
    if action == "state" {
        return snapshot();
    }
    if action == "prepare_panel" {
        file::panel::prepare()?;
        return snapshot();
    }
    if action == "runtime" {
        return Ok(super::restful::config::fetch_raw()?);
    }
    if action == "patch" {
        super::restful::config::patch_checked(value["patch"].clone())?;
        return Ok(super::restful::config::fetch_raw()?);
    }
    if action == "persist" {
        return persist_runtime();
    }
    if action == "read" {
        return read_document(text(&value, "kind")?, value["name"].as_str().unwrap_or(""));
    }
    if action == "save" {
        return save_document(
            text(&value, "kind")?,
            value["name"].as_str().unwrap_or(""),
            text(&value, "content")?,
            text(&value, "revision")?,
        );
    }
    let db = std::fs::read(crate::config::config_dir_path().join("clashtui.db"))?;
    anyhow::ensure!(
        text(&value, "revision")? == revision(&db),
        "Revision conflict: refresh before applying this action"
    );
    let name = value["name"].as_str().unwrap_or("");
    let runtime = tokio::runtime::Runtime::new()?;
    match action {
        "profile_url" => {
            let pf = profile::db::get(name).context("Profile not found")?;
            match pf.dtype {
                crate::config::database::ProfileType::Url(url) => return Ok(json!({"url":url})),
                _ => anyhow::bail!("This profile has no subscription URL"),
            }
        }
        "traffic" => return subscription_traffic(name),
        "check" | "test" => {
            let path = document_path("profile", name)?;
            return super::command::configuration_test(&path);
        }
        "preview" => return read_document("profile", name),
        "preview_template" => {
            let content = runtime.block_on(template::preview_template(
                name,
                value["with_proxy"].as_bool().unwrap_or(false),
            ))?;
            return Ok(json!({"content":content}));
        }
        "template_providers" => {
            document_path("template", name)?;
            return Ok(
                json!({"groups":template::read_template_ppg(name)?,"revision":read_document("template",name)?["revision"]}),
            );
        }
        "save_template_providers" => {
            let document = read_document("template", name)?;
            anyhow::ensure!(
                value["document_revision"] == document["revision"],
                "Revision conflict: reload template before saving providers"
            );
            let groups = serde_json::from_value(value["groups"].clone())?;
            template::write_template_ppg(name, &groups)?;
        }
        "delete_template" => {
            let path = document_path("template", name)?;
            anyhow::ensure!(!profile::db::get_all().iter().any(|pf| matches!(&pf.dtype, crate::config::database::ProfileType::Template { template } if template == name)), "Template is referenced by a profile; delete or replace those profiles first");
            let document = read_document("template", name)?;
            anyhow::ensure!(
                value["document_revision"] == document["revision"],
                "Revision conflict: reload template before deleting"
            );
            std::fs::remove_file(path)?;
        }
        "update_all" => {
            let selected = profile::db::get_all();
            let mut results = Vec::new();
            for pf in selected {
                let name = pf.name.clone();
                let proxy = value["with_proxy"]
                    .as_bool()
                    .unwrap_or(pf.update_with_proxy);
                let result = runtime.block_on(profile::update_profile(pf, proxy));
                results.push(match result {
                    Ok(update) => json!({"name":name,"ok":update.net_updates.iter().all(|resource|resource.ok),"resources":update.net_updates}),
                    Err(error) => json!({"name":name,"ok":false,"error":error.to_string()}),
                });
            }
            return Ok(json!({"results":results}));
        }
        "stop_all" => {
            super::command::stop_all_services()?;
        }
        "create" => {
            profile::validate_profile_name(name)?;
            anyhow::ensure!(profile::db::get(name).is_none(), "Profile already exists");
            let url = text(&value, "url")?;
            profile::validate_subscription_url(url)?;
            let mut response = super::restful::download::profile(
                url,
                value["with_proxy"].as_bool().unwrap_or(false),
            )?;
            let mut content = String::new();
            std::io::Read::read_to_string(&mut response, &mut content)?;
            import_content(
                name,
                &content,
                crate::config::database::ProfileType::Url(url.to_owned()),
            )?;
        }
        "rename" => profile::edit_profile(name, text(&value, "new_name")?, value["url"].as_str())?,
        "import" => {
            let content = text(&value, "content")?;
            import_content(name, content, crate::config::database::ProfileType::File)?;
        }
        "delete" | "update" | "activate" | "no_pp" | "with_proxy" => {
            let pf = profile::db::get(name).context("Profile not found")?;
            match action {
                "delete" => {
                    anyhow::ensure!(
                        profile::db::get_current().name != name,
                        "Select another profile before deleting the active profile"
                    );
                    profile::db::remove(pf)?;
                }
                "update" => {
                    let with_proxy = value["with_proxy"]
                        .as_bool()
                        .unwrap_or(pf.update_with_proxy);
                    let update = runtime.block_on(profile::update_profile(pf, with_proxy))?;
                    return Ok(
                        json!({"name":update.name,"partial_failure":update.net_updates.iter().any(|resource|!resource.ok),"resources":update.net_updates}),
                    );
                }
                "activate" => runtime.block_on(profile::select(pf))?,
                "no_pp" => {
                    profile::db::toggle_no_pp(name)?;
                }
                "with_proxy" => {
                    profile::db::toggle_update_with_proxy(name)?;
                }
                _ => unreachable!(),
            }
        }
        "generate" => {
            profile::validate_profile_name(name)?;
            let tpl = text(&value, "template")?;
            profile::validate_profile_name(tpl)?;
            match CONFIG.core_type() {
                CoreType::Mihomo => template::apply_template(tpl, name)?,
            }
        }
        #[cfg(windows)]
        "system_proxy" => {
            anyhow::ensure!(
                local_controller(CONFIG.controller_for_core()),
                "Remote endpoints cannot modify the local system proxy"
            );
            if super::command::get_system_proxy_state()? {
                super::command::disable_system_proxy()?;
            } else {
                super::command::enable_system_proxy(super::command::get_mixed_port()?)?;
            }
        }
        #[cfg(windows)]
        "install" => {
            super::command::install_core_service(CONFIG.core_type())?;
        }
        #[cfg(windows)]
        "uninstall" => {
            super::command::uninstall_core_service(CONFIG.core_type())?;
        }
        "restart" | "stop" | "start" => {
            anyhow::ensure!(
                local_controller(CONFIG.controller_for_core()),
                "Remote endpoints cannot control local services"
            );
            match action {
                "restart" => {
                    crate::functions::command::restart_service()?;
                }
                "stop" => {
                    crate::functions::command::stop_service()?;
                }
                _ => {
                    crate::functions::command::start_core_service(CONFIG.core_type())?;
                }
            }
        }
        _ => anyhow::bail!("Unknown management action"),
    }
    snapshot()
}

pub fn local_controller(controller: &str) -> bool {
    let Some(authority) = controller
        .strip_prefix("http://")
        .or_else(|| controller.strip_prefix("https://"))
        .and_then(|url| url.split('/').next())
    else {
        return false;
    };
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn subscription_traffic(name: &str) -> Result<Value> {
    use crate::config::database::ProfileType;
    use file::net_resource::{ExtractNetResources, ResourceSection};
    let pf = profile::db::get(name).context("Profile not found")?;
    let mut urls = std::collections::BTreeMap::new();
    if let ProfileType::Url(url) = &pf.dtype {
        urls.insert(url.clone(), name.to_owned());
    }
    if let Ok(groups) = template::read_profile_ppg(name) {
        for providers in groups.values() {
            for (label, url) in providers {
                urls.entry(url.clone()).or_insert_with(|| label.clone());
            }
        }
    }
    if let Ok(document) = read_document("profile", name) {
        let mapping: serde_yml::Mapping = serde_yml::from_str(text(&document, "content")?)?;
        for resource in mapping.extract(&[ResourceSection::ProxyProvider]) {
            urls.entry(resource.url).or_insert(resource.name);
        }
    }
    let mut results = Vec::new();
    for (url, label) in urls {
        let result = match super::restful::download::fetch_subscription_userinfo(
            &url,
            pf.update_with_proxy,
        ) {
            Ok(Some(header)) => {
                json!({"name":label,"available":true,"usage":parse_userinfo(&header)})
            }
            Ok(None) => {
                json!({"name":label,"available":false,"message":"Subscription does not report traffic quota"})
            }
            Err(error) => {
                json!({"name":label,"available":false,"error":error.to_string().replace(&url,"[subscription]")})
            }
        };
        results.push(result);
    }
    Ok(json!({"subscriptions":results}))
}

fn import_content(
    name: &str,
    content: &str,
    dtype: crate::config::database::ProfileType,
) -> Result<()> {
    profile::validate_profile_name(name)?;
    anyhow::ensure!(profile::db::get(name).is_none(), "Profile already exists");
    validate_import_content(content, CONFIG.core_type())?;
    let path = profile::local_profile_path(name);
    anyhow::ensure!(!path.exists(), "Profile file already exists");
    file::activation::atomic_write(&path, content.as_bytes())?;
    let mut db = file::coordination::database();
    db.insert(name, dtype);
    if let Err(error) = db.to_file() {
        db.remove(name);
        let cleanup = std::fs::remove_file(&path);
        anyhow::bail!(
            "Could not register imported profile: {error}; cleaning up new file: {cleanup:?}"
        );
    }
    Ok(())
}

fn validate_import_content(content: &str, _core: CoreType) -> Result<()> {
    let config: serde_yml::Mapping = serde_yml::from_str(content)?;
    anyhow::ensure!(
        config.get("proxies").is_some_and(|v| v.is_sequence())
            || config
                .get("proxy-providers")
                .is_some_and(|v| v.is_mapping()),
        "Not a Clash profile: missing proxies or proxy-providers"
    );
    Ok(())
}

fn parse_userinfo(header: &str) -> Value {
    let fields: serde_json::Map<String, Value> = header
        .split(';')
        .filter_map(|field| {
            let (key, value) = field.trim().split_once('=')?;
            Some((
                key.trim().to_owned(),
                json!(value.trim().parse::<u64>().ok()?),
            ))
        })
        .collect();
    let mut result = Value::Object(fields);
    let used = result["upload"]
        .as_u64()
        .unwrap_or(0)
        .saturating_add(result["download"].as_u64().unwrap_or(0));
    result["used"] = json!(used);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisting_runtime_handles_null_sections_and_rejects_scalars_without_panicking() {
        let mut overlay = json!({"tun":null});
        merge_runtime(
            &json!({"tun":{"enable":true}}),
            &mut overlay,
            CoreType::Mihomo,
        )
        .unwrap();
        assert_eq!(overlay["tun"]["enable"], true);
        assert!(
            merge_runtime(
                &json!({"tun":{"enable":true}}),
                &mut json!({"tun":[]}),
                CoreType::Mihomo
            )
            .is_err()
        );
    }
    #[test]
    fn quota_keeps_expiry_and_unknown_numeric_fields_without_overflow() {
        let usage = parse_userinfo(
            " upload = 18446744073709551615 ; download=20; total=0; expire=42; ignored=text; malformed; extra=7",
        );
        assert_eq!(usage["used"], u64::MAX);
        assert_eq!(usage["total"], 0);
        assert_eq!(usage["expire"], 42);
        assert_eq!(usage["extra"], 7);
        assert!(usage.get("ignored").is_none());
    }
    #[test]
    fn importing_a_configuration_rejects_the_wrong_core_and_scalars() {
        assert!(validate_import_content("[]", CoreType::Mihomo).is_err());
        assert!(validate_import_content("{}", CoreType::Mihomo).is_err());
        assert!(validate_import_content(r#"{"outbounds": []}"#, CoreType::Mihomo).is_err());
        assert!(validate_import_content("proxies: []", CoreType::Mihomo).is_ok());
        assert!(validate_import_content("proxy-providers: {}", CoreType::Mihomo).is_ok());
    }
    #[test]
    fn only_loopback_controllers_allow_local_service_operations() {
        for value in [
            "http://localhost:9090",
            "https://127.0.0.2:9090",
            "http://[::1]:9090",
        ] {
            assert!(local_controller(value), "{value}");
        }
        for value in [
            "http://192.168.1.2:9090",
            "http://localhost.evil:9090",
            "http://localhost@evil:9090",
            "http://[2001:db8::1]:9090",
            "file://localhost",
        ] {
            assert!(!local_controller(value), "{value}");
        }
    }
    #[test]
    fn stale_editor_cannot_overwrite_external_changes() {
        let path = std::env::temp_dir().join(format!("clashtui-editor-{}.json", fastrand::u64(..)));
        std::fs::write(&path, "{}").unwrap();
        let old = revision(b"{}");
        std::fs::write(&path, r#"{"mode":"direct"}"#).unwrap();
        assert!(
            save_document_at(&path, "{}", &old, true)
                .unwrap_err()
                .to_string()
                .contains("conflict")
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"mode":"direct"}"#
        );
        std::fs::remove_file(path).unwrap();
    }
}
