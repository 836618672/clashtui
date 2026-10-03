#[allow(clippy::module_inception)]
mod profile;

use super::PROFILE_YAMLS_PATH;
use super::net_resource::{ExtractNetResources, ResourceSection};
use crate::config::database::{Profile, ProfileType};

pub mod db {
    use super::*;

    pub fn remove(pf: Profile) -> anyhow::Result<()> {
        let _write = super::super::coordination::WriteGuard::acquire()?;
        let path = local_profile_path(&pf.name);
        let mut pm = pm!();
        pm.ensure_loaded()?;
        anyhow::ensure!(
            pm.get_current()
                .is_none_or(|current| current.name != pf.name),
            "Select another profile before deleting the active profile"
        );
        anyhow::ensure!(pm.get(&pf.name).is_some(), "Profile no longer exists");
        let previous = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
        let old = pm.clone();
        pm.remove(&pf.name);
        if let Err(error) = pm.to_file() {
            *pm = old;
            if let Some(bytes) = previous {
                super::super::activation::atomic_write(&path, &bytes).map_err(|recovery| {
                    anyhow::anyhow!(
                        "Delete failed: {error}; restoring profile file failed: {recovery}"
                    )
                })?;
            }
            return Err(error.context("Delete failed; profile file preserved"));
        }
        Ok(())
    }
    pub fn get(name: impl AsRef<str>) -> Option<Profile> {
        pm!().get(name)
    }
    pub fn get_all() -> Vec<Profile> {
        let pm = pm!();
        pm.all_for_core()
            .into_iter()
            .map(|k| pm.get(k).unwrap())
            .collect()
    }
    pub fn get_current() -> Profile {
        pm!().get_current().unwrap_or_default()
    }
    pub fn set_current(pf: Profile) -> anyhow::Result<()> {
        let mut pm = pm!();
        anyhow::ensure!(
            pm.get(&pf.name).is_some(),
            "Profile was removed before activation; refresh and retry"
        );
        pm.set_current(pf);
        pm.to_file()
    }
    pub fn toggle_no_pp(name: impl AsRef<str>) -> anyhow::Result<bool> {
        let mut pm = pm!();
        let current = pm.get(name.as_ref()).map(|pf| pf.no_pp).unwrap_or(false);
        let new = !current;
        pm.set_no_pp(name.as_ref(), new);
        pm.to_file()?;
        Ok(new)
    }
    pub fn toggle_update_with_proxy(name: impl AsRef<str>) -> anyhow::Result<bool> {
        let mut pm = pm!();
        let current = pm
            .get(name.as_ref())
            .map(|pf| pf.update_with_proxy)
            .unwrap_or(false);
        let new = !current;
        pm.set_update_with_proxy(name.as_ref(), new);
        pm.to_file()?;
        Ok(new)
    }
}

pub fn local_profile_path(name: &str) -> std::path::PathBuf {
    match crate::config::CONFIG.core_type() {
        crate::config::CoreType::Mihomo => PROFILE_YAMLS_PATH.join(format!("{name}.yaml")),
    }
}

pub fn validate_profile_name(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !name.is_empty()
            && name.trim() == name
            && name != "."
            && name != ".."
            && !name.ends_with('.')
            && !name
                .chars()
                .any(|c| c.is_control() || "/\\<>:\"|?*".contains(c)),
        "Enter a non-empty profile name without path separators or reserved filename characters"
    );
    Ok(())
}

pub fn validate_subscription_url(url: &str) -> anyhow::Result<()> {
    let authority = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|rest| rest.split(['/', '?', '#']).next());
    anyhow::ensure!(
        authority.is_some_and(|host| !host.is_empty()) && !url.chars().any(char::is_whitespace),
        "Enter an HTTP or HTTPS subscription URL with a host"
    );
    Ok(())
}

/// Save metadata without downloading or applying a configuration. Renaming keeps
/// the cached profile and current selection; failure leaves the old entry intact.
pub fn edit_profile(old_name: &str, new_name: &str, url: Option<&str>) -> anyhow::Result<()> {
    let _write = super::coordination::WriteGuard::acquire()?;
    let path = local_profile_path(old_name);
    let mut database = pm!();
    database.ensure_loaded()?;
    edit_profile_in(
        &mut database,
        path.parent().unwrap(),
        &crate::config::config_dir_path().join("clashtui.db"),
        old_name,
        new_name,
        url,
    )
}

fn edit_profile_in(
    pm: &mut crate::config::database::ProfileManager,
    profile_dir: &std::path::Path,
    db_path: &std::path::Path,
    old_name: &str,
    new_name: &str,
    url: Option<&str>,
) -> anyhow::Result<()> {
    use anyhow::Context;
    validate_profile_name(old_name)?;
    validate_profile_name(new_name)?;
    let mut next = pm.clone();
    let active = next.active_mut();
    anyhow::ensure!(
        old_name == new_name || !active.profiles.contains_key(new_name),
        "Profile '{new_name}' already exists"
    );
    let mut data = active
        .profiles
        .remove(old_name)
        .context("Profile no longer exists")?;
    if let Some(url) = url {
        validate_subscription_url(url)?;
        anyhow::ensure!(
            matches!(data.dtype, ProfileType::Url(_)),
            "This profile has no subscription URL"
        );
        data.dtype = ProfileType::Url(url.to_owned());
    }
    active.profiles.insert(new_name.to_owned(), data);
    if active.cur_profile.as_deref() == Some(old_name) {
        active.cur_profile = Some(new_name.to_owned());
    }
    let extension = match pm.core_type {
        crate::config::CoreType::Mihomo => "yaml",
    };
    let old_path = profile_dir.join(format!("{old_name}.{extension}"));
    let new_path = profile_dir.join(format!("{new_name}.{extension}"));
    if old_name != new_name {
        anyhow::ensure!(
            !new_path.exists(),
            "Profile file '{}' already exists",
            new_path.display()
        );
    }
    let tmp = super::activation::TemporaryFile::create_beside(
        db_path,
        serde_yml::to_string(&next)?.as_bytes(),
    )?;
    let moved = old_name != new_name && old_path.exists();
    let commit = (|| -> anyhow::Result<()> {
        if moved {
            std::fs::rename(&old_path, &new_path).context("Failed to rename profile file")?;
        }
        if let Err(err) = std::fs::rename(&tmp.0, db_path) {
            if moved {
                std::fs::rename(&new_path, &old_path)
                    .context("Failed to restore profile file after database save failed")?;
            }
            return Err(err).context("Failed to save profile changes");
        }
        Ok(())
    })();
    drop(tmp);
    commit?;
    *pm = next;
    Ok(())
}

// Import registration is centralized in functions::management for all clients.
pub struct UpdateResult {
    pub name: String,
    pub net_updates: Vec<crate::functions::file::net_resource::NetResourceUpdate>,
}

pub async fn update_profile(profile: Profile, with_proxy: bool) -> anyhow::Result<UpdateResult> {
    super::coordination::transaction(move || async move {
        validate_registered_profile(&profile)?;
        update_profile_locked(profile, with_proxy).await
    })
    .await
}

fn validate_registered_profile(profile: &Profile) -> anyhow::Result<()> {
    let db = super::coordination::database();
    db.ensure_loaded()?;
    let current = db
        .get(&profile.name)
        .ok_or_else(|| anyhow::anyhow!("Profile was removed or renamed; refresh and retry"))?;
    anyhow::ensure!(
        current.dtype == profile.dtype
            && current.no_pp == profile.no_pp
            && current.update_with_proxy == profile.update_with_proxy,
        "Profile changed; refresh and retry"
    );
    Ok(())
}

async fn update_profile_locked(profile: Profile, with_proxy: bool) -> anyhow::Result<UpdateResult> {
    use super::template::fetch_net_resource_statuses;

    let result = if matches!(profile.dtype, ProfileType::Template { .. }) {
        update_template_profile(profile.clone(), with_proxy).await
    } else {
        let path = PROFILE_YAMLS_PATH.join(format!("{}.yaml", profile.name));

        if let ProfileType::Url(ref url) = profile.dtype {
            let mut response = crate::functions::restful::download::profile(url, with_proxy)?;
            let content: serde_yml::Mapping = serde_yml::from_reader(&mut response)
                .map_err(|e| anyhow::anyhow!("Failed to parse downloaded profile YAML: {e}"))?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            super::activation::atomic_write(&path, serde_yml::to_string(&content)?.as_bytes())?;
        }

        anyhow::ensure!(
            path.exists(),
            "Profile file not found: {}. Download it first.",
            path.display()
        );

        let content: serde_yml::Mapping = {
            let file = std::fs::File::open(&path)?;
            serde_yml::from_reader(file)
                .map_err(|e| anyhow::anyhow!("Failed to read profile YAML: {e}"))?
        };

        let net_updates = fetch_net_resource_statuses(&content, with_proxy).await;
        Ok(UpdateResult {
            name: profile.name.clone(),
            net_updates,
        })
    };

    if result
        .as_ref()
        .is_ok_and(|result| result.net_updates.iter().all(|resource| resource.ok))
    {
        let cur = db::get_current();
        if cur.name == profile.name {
            select_locked(profile).await.map_err(|error| {
                anyhow::anyhow!("Subscription updated, but activation failed: {error:#}")
            })?;
        }
    }

    result
}

fn apply_generated_config(
    out_path: &std::path::Path,
    bytes: &[u8],
    profile: Profile,
) -> anyhow::Result<()> {
    let mapping: serde_yml::Mapping = serde_yml::from_slice(bytes)?;
    super::net_resource::validate_cache_paths(&mapping)?;
    let core = crate::config::CONFIG.core_type();
    let expected = super::evidence::parse(bytes, core)?;
    super::evidence::validate_endpoint(
        &expected,
        core,
        crate::config::CONFIG.controller_for_core(),
        crate::config::CONFIG.secret_for_core(),
    )?;
    let mut recovering = false;
    super::activation::activate(
        out_path,
        bytes,
        crate::functions::command::check_config,
        |path| {
            let on_disk = super::evidence::parse(&std::fs::read(path)?, core)?;
            let result = crate::functions::restful::config::reload(path.display().to_string())
                .map_err(anyhow::Error::from)
                .and_then(|_| super::evidence::verify(&on_disk, core));
            let recovery_attempt = recovering;
            recovering = true;
            if recovery_attempt && result.is_err() {
                crate::functions::command::restart_service()?;
                crate::functions::restful::config::wait_until_ready()?;
                super::evidence::verify(&on_disk, core)
            } else {
                result
            }
        },
        || db::set_current(profile),
    )
}

async fn update_template_profile(
    profile: Profile,
    with_proxy: bool,
) -> anyhow::Result<UpdateResult> {
    anyhow::ensure!(
        matches!(profile.dtype, ProfileType::Template { .. }),
        "Expected template profile"
    );
    let path = super::PROFILE_YAMLS_PATH.join(format!("{}.yaml", profile.name));
    let mapping: serde_yml::Mapping = serde_yml::from_slice(&std::fs::read(path)?)?;
    let statuses = super::template::fetch_net_resource_statuses(&mapping, with_proxy).await;
    Ok(UpdateResult {
        name: profile.name,
        net_updates: statuses,
    })
}

pub async fn select(profile: Profile) -> anyhow::Result<()> {
    super::coordination::transaction(move || async move {
        validate_registered_profile(&profile)?;
        select_locked(profile).await
    })
    .await
}

async fn select_locked(profile: Profile) -> anyhow::Result<()> {
    anyhow::ensure!(
        crate::functions::management::local_controller(crate::config::CONFIG.controller_for_core()),
        "Local configuration activation is unavailable for a remote core endpoint"
    );
    use super::template::{
        check_template_ppg_availability, fetch_net_resource_statuses, update_profile_without_pp,
    };

    // For Template profiles, verify proxy-provider files exist before selection
    if matches!(profile.dtype, ProfileType::Template { .. }) {
        check_template_ppg_availability(&profile)?;
    }

    let cfg = &crate::config::CONFIG.cfg_file.mihomo.core;
    let mut lprofile = profile.clone().load_local_profile()?;
    anyhow::ensure!(
        lprofile.content.is_some(),
        "Profile {} is empty or not yet downloaded. Run update first.",
        profile.name
    );

    if profile.no_pp {
        let content = lprofile.content.take().unwrap_or_default();
        let (new_content, statuses) = update_profile_without_pp(content, false).await?;
        anyhow::ensure!(
            statuses.iter().all(|status| status.ok),
            "Provider preparation failed:\n{}",
            super::net_resource::format_net_updates(&statuses)
        );
        lprofile.content = Some(new_content);
    } else if let Some(ref content) = lprofile.content {
        let statuses = fetch_net_resource_statuses(content, false).await;
        // Downloads may fail offline with a usable old cache. Unsafe paths must
        // still be rejected before handing the configuration to the core.
        for resource in content.extract(&[
            ResourceSection::ProxyProvider,
            ResourceSection::RuleProvider,
        ]) {
            let path = super::net_resource::cache_path(
                std::path::Path::new(&cfg.config_dir),
                &resource.path,
            )?;
            if statuses.iter().any(|status| {
                !status.ok && status.name == resource.name && status.section == resource.section
            }) {
                resource.validate(&std::fs::read(&path)?, &path)?;
            }
        }
    }

    rewrite_provider_paths(lprofile.content.as_mut());

    lprofile.merge(&crate::config::load_basic()?)?;
    // Strip clashtui metadata before writing to core config
    if let Some(ref mut content) = lprofile.content {
        content.remove("clashtui");
    }
    let out_path = std::path::absolute(std::path::PathBuf::from(&cfg.config_path))
        .map_err(|e| anyhow::anyhow!("Failed to resolve config path: {e}"))?;
    let bytes = serde_yml::to_string(&lprofile.content)?.into_bytes();
    apply_generated_config(&out_path, &bytes, profile)?;
    Ok(())
}

fn rewrite_provider_paths(_content: Option<&mut serde_yml::Mapping>) {
    // Paths are kept as-is (relative to mihomo's -d working directory).
    // Mihomo resolves relative proxy-provider/rule-provider paths against
    // its config directory, avoiding hard-coded absolute paths that break
    // when config_dir changes (e.g. switching between user/system mode).
}

pub fn extract_domain(url: &str) -> Option<&str> {
    if let Some(protocol_end) = url.find("://") {
        let rest = &url[(protocol_end + 3)..];
        let rest = if let Some(at_pos) = rest.find('@') {
            if let Some(slash_pos) = rest.find('/') {
                if at_pos < slash_pos {
                    &rest[(at_pos + 1)..]
                } else {
                    rest
                }
            } else {
                &rest[(at_pos + 1)..]
            }
        } else {
            rest
        };
        return if let Some(path_start) = rest.find('/') {
            Some(&rest[..path_start])
        } else {
            Some(rest)
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EditFixture(std::path::PathBuf);

    impl EditFixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("clashtui-edit-{:x}", fastrand::u128(..)));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for EditFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn editing_subscription_renames_cache_and_current_selection() {
        use crate::config::{CoreType, database::ProfileManager};
        {
            let (core, extension) = (CoreType::Mihomo, "yaml");
            let fixture = EditFixture::new();
            let mut pm = ProfileManager {
                core_type: core,
                ..Default::default()
            };
            pm.insert("old", ProfileType::Url("https://old.example/sub".into()));
            pm.set_no_pp("old", true);
            pm.set_update_with_proxy("old", true);
            pm.active_mut().cur_profile = Some("old".into());
            let db_path = fixture.0.join("clashtui.db");
            std::fs::write(&db_path, serde_yml::to_string(&pm).unwrap()).unwrap();
            std::fs::write(fixture.0.join(format!("old.{extension}")), "cached config").unwrap();
            edit_profile_in(
                &mut pm,
                &fixture.0,
                &db_path,
                "old",
                "新订阅",
                Some("https://new.example/sub"),
            )
            .unwrap();
            assert!(pm.get("old").is_none());
            let edited = pm.get_current().unwrap();
            assert_eq!(edited.name, "新订阅");
            assert_eq!(
                edited.dtype,
                ProfileType::Url("https://new.example/sub".into())
            );
            assert!(edited.no_pp && edited.update_with_proxy);
            assert_eq!(
                std::fs::read_to_string(fixture.0.join(format!("新订阅.{extension}"))).unwrap(),
                "cached config"
            );
            assert!(!fixture.0.join(format!("old.{extension}")).exists());
            let saved: ProfileManager =
                serde_yml::from_str(&std::fs::read_to_string(db_path).unwrap()).unwrap();
            assert_eq!(saved, pm);
        }
    }

    #[test]
    fn edit_rejects_duplicates_and_invalid_input_without_changes() {
        let fixture = EditFixture::new();
        let mut pm = crate::config::database::ProfileManager::default();
        pm.insert("old", ProfileType::Url("https://old.example".into()));
        pm.insert("taken", ProfileType::File);
        let before = pm.clone();
        for (name, url) in [
            ("taken", None),
            ("../escape", None),
            ("", None),
            ("old", Some("https://")),
            ("old", Some("file:///tmp/a")),
        ] {
            assert!(
                edit_profile_in(&mut pm, &fixture.0, &fixture.0.join("db"), "old", name, url)
                    .is_err()
            );
            assert_eq!(before, pm);
        }
        assert!(!fixture.0.join("db").exists());
    }

    #[test]
    fn failed_database_save_rolls_back_profile_rename() {
        let fixture = EditFixture::new();
        let mut pm = crate::config::database::ProfileManager::default();
        pm.insert("old", ProfileType::File);
        let before = pm.clone();
        std::fs::write(fixture.0.join("old.yaml"), "cached").unwrap();
        let db_path = fixture.0.join("db");
        std::fs::create_dir(&db_path).unwrap();
        assert!(edit_profile_in(&mut pm, &fixture.0, &db_path, "old", "new", None).is_err());
        assert_eq!(pm, before);
        assert!(fixture.0.join("old.yaml").exists());
        assert!(!fixture.0.join("new.yaml").exists());
    }

    #[test]
    fn url_can_be_edited_before_download_without_changing_other_profiles() {
        let fixture = EditFixture::new();
        let mut pm = crate::config::database::ProfileManager::default();
        pm.insert("same", ProfileType::Url("https://mihomo.example".into()));
        pm.insert("other", ProfileType::Url("https://other.example".into()));
        edit_profile_in(
            &mut pm,
            &fixture.0,
            &fixture.0.join("db"),
            "same",
            "same",
            Some("https://changed.example"),
        )
        .unwrap();
        assert_eq!(
            pm.get("same").unwrap().dtype,
            ProfileType::Url("https://changed.example".into())
        );
        assert_eq!(
            pm.get("other").unwrap().dtype,
            ProfileType::Url("https://other.example".into())
        );
    }

    // ── Tests for standalone proxy-provider URL extraction during update ──────

    use crate::config::database::ProxyProviderGroups;
    use crate::functions::file::net_resource::{ExtractNetResources, ResourceSection};
    use std::collections::BTreeMap;

    /// Collect all proxy-provider download URLs from groups + generated profile,
    /// with deduplication (same logic as in `update_template_profile`).
    fn collect_proxy_provider_urls(
        profile_yaml: &str,
        groups: &ProxyProviderGroups,
    ) -> Vec<(String, String)> {
        let mut urls: Vec<(String, String)> = Vec::new();
        for providers in groups.values() {
            for (name, url) in providers {
                urls.push((name.clone(), url.clone()));
            }
        }

        if let Ok(mapping) = serde_yml::from_str::<serde_yml::Mapping>(profile_yaml) {
            for resource in mapping.extract(&[ResourceSection::ProxyProvider]) {
                let already_in_groups = groups
                    .values()
                    .flat_map(|providers| providers.values())
                    .any(|url| url == &resource.url);
                if !already_in_groups {
                    urls.push((resource.name, resource.url));
                }
            }
        }
        urls
    }

    #[test]
    fn standalone_proxy_provider_url_collected() {
        let profile_yaml = r#"
proxy-providers:
  pvd0:
    type: http
    url: https://example.com/sub1.yaml
    interval: 3600
  bak:
    type: http
    url: https://hajimi.nvimy.com/file/bak.yaml
    interval: 3600
proxy-groups:
  - name: "Entry"
    type: select
    use:
      - pvd0
  - name: "Special"
    type: select
    use:
      - bak
"#;

        let mut providers = BTreeMap::new();
        providers.insert(
            "pvd0".to_string(),
            "https://example.com/sub1.yaml".to_string(),
        );
        let mut groups = ProxyProviderGroups::new();
        groups.insert("pvd".to_string(), providers);

        let urls = collect_proxy_provider_urls(profile_yaml, &groups);

        // Group provider (pvd0) should be included
        assert!(
            urls.iter().any(|(name, _)| name == "pvd0"),
            "pvd0 from groups should be collected"
        );
        // Standalone provider (bak) should also be collected
        assert!(
            urls.iter().any(|(name, _)| name == "bak"),
            "bak standalone provider should be collected"
        );

        // pvd0 should appear only once (not duplicated from extract)
        let pvd0_count = urls.iter().filter(|(name, _)| name == "pvd0").count();
        assert_eq!(pvd0_count, 1, "pvd0 should not be duplicated");
    }

    #[test]
    fn standalone_proxy_provider_bak_specifically_collected() {
        // Realistic generated profile mimicking the user's setup:
        // two group-expanded providers (hajimi, mojie) + standalone bak
        let profile_yaml = r#"
proxy-providers:
  hajimi:
    type: http
    url: https://hajimi.nvimy.com/file/clash.yaml
  mojie:
    type: http
    url: https://hajimi.nvimy.com/file/clash_mojie.yaml
  bak:
    type: http
    url: https://hajimi.nvimy.com/file/mojie_johan.yaml
    override:
      additional-prefix: '[bak]'
proxy-groups:
  - name: "Entry"
    type: select
    use:
      - hajimi
      - mojie
  - name: "Special"
    type: select
    use:
      - bak
"#;

        let mut pvd_providers = BTreeMap::new();
        pvd_providers.insert(
            "hajimi".to_string(),
            "https://hajimi.nvimy.com/file/clash.yaml".to_string(),
        );
        pvd_providers.insert(
            "mojie".to_string(),
            "https://hajimi.nvimy.com/file/clash_mojie.yaml".to_string(),
        );
        let mut groups = ProxyProviderGroups::new();
        groups.insert("pvd".to_string(), pvd_providers);

        let urls = collect_proxy_provider_urls(profile_yaml, &groups);

        assert_eq!(
            urls.len(),
            3,
            "should collect 3 unique URLs: hajimi, mojie, bak"
        );

        assert!(urls.iter().any(|(name, _)| name == "hajimi"));
        assert!(urls.iter().any(|(name, _)| name == "mojie"));
        assert!(
            urls.iter().any(|(name, _)| name == "bak"),
            "bak MUST be collected as standalone provider"
        );

        // Verify bak's URL is correct
        let bak_url = urls
            .iter()
            .find(|(name, _)| name == "bak")
            .map(|(_, url)| url);
        assert_eq!(
            bak_url,
            Some(&"https://hajimi.nvimy.com/file/mojie_johan.yaml".to_string())
        );
    }

    #[test]
    fn empty_groups_standalone_still_collected() {
        // When groups are empty (no tpl_param providers), standalone ones still work
        let profile_yaml = r#"
proxy-providers:
  bak:
    type: http
    url: https://example.com/standalone.yaml
proxy-groups:
  - name: "Entry"
    type: select
    use:
      - bak
"#;
        let groups = ProxyProviderGroups::new();

        let urls = collect_proxy_provider_urls(profile_yaml, &groups);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].0, "bak");
        assert!(urls[0].1.contains("standalone.yaml"));
    }

    #[test]
    fn no_standalone_when_all_in_groups() {
        let profile_yaml = r#"
proxy-providers:
  pvd0:
    type: http
    url: https://example.com/sub1.yaml
  pvd1:
    type: http
    url: https://example.com/sub2.yaml
proxy-groups:
  - name: "Entry"
    type: select
    use:
      - pvd0
      - pvd1
"#;
        let mut providers = BTreeMap::new();
        providers.insert(
            "pvd0".to_string(),
            "https://example.com/sub1.yaml".to_string(),
        );
        providers.insert(
            "pvd1".to_string(),
            "https://example.com/sub2.yaml".to_string(),
        );
        let mut groups = ProxyProviderGroups::new();
        groups.insert("pvd".to_string(), providers);

        let urls = collect_proxy_provider_urls(profile_yaml, &groups);
        assert_eq!(urls.len(), 2, "only the two group providers, no duplicates");
    }
}
