//! under the data folder:
//! * [`BasicInfo`] mihomo/core_override_config.yaml
//! * [`ProfileManager`] clashtui.db
//! * [`log`] clashtui.log
//! * [`ConfigFile`] config.yaml
//! * `Folder` mihomo/profiles/
//! * `Folder` mihomo/templates/

use anyhow::{Context, Result, ensure};
use core::*;
use database::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Mutex, OnceLock},
};
use util::*;

mod core;
pub use core::{CoreType, ServiceController};
#[macro_use]
mod util;
pub mod database;

/// Load using [init]
pub const CONFIG: Wrapper = Wrapper;

static CORE_MISMATCH: AtomicBool = AtomicBool::new(false);
static CORE_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn core_generation() -> u64 {
    CORE_GENERATION.load(Ordering::Acquire)
}

pub fn initialized() -> bool {
    _CONFIG.get().is_some()
}

/// Set when StatusTab detects the API is serving data from a different core
/// than the configured one.
pub fn set_core_mismatch(mismatch: bool) {
    CORE_MISMATCH.store(mismatch, Ordering::Release);
}

/// True when the running core does not match the configured core type.
/// Tabs should skip displaying API data when this returns true.
pub fn is_core_mismatch() -> bool {
    CORE_MISMATCH.load(Ordering::Acquire)
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static CONFIG_ROOT: OnceLock<PathBuf> = OnceLock::new();
static _CONFIG: OnceLock<Config> = OnceLock::new();

/// Wrapper around [Config], only propose is be deref-ed as [Config]
///
/// Do Not use it directly
pub struct Wrapper;

impl std::ops::Deref for Wrapper {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        _CONFIG.get().expect("uninited")
    }
}

/// Do Not use it directly, use [CONFIG] instead
pub struct Config {
    pub cfg_file: ConfigFile,
    pub data: Mutex<ProfileManager>,
    pub external_controller: String,
    pub proxy_addr: String,
    pub secret: Option<String>,
    pub global_ua: Option<String>,
}

impl Config {
    fn load() -> Result<Self> {
        let mut cfg_file = ConfigFile::from_file()?;
        let basic_info = BasicInfo::from_file()?;
        let data: ProfileManager = ProfileManager::from_file()?;
        // Flush pending legacy Template migrations: write proxy_provider_groups from
        // old database entries into the corresponding template files.
        {
            let mut queue = database::PENDING_TEMPLATE_MIGRATIONS.lock().unwrap();
            for (template_name, groups) in queue.drain(..) {
                let tpl_path = template_path().join(&template_name);
                if tpl_path.exists() {
                    // Only write if the template file doesn't already have clashtui.proxy_provider_groups
                    let has_groups = std::fs::read_to_string(&tpl_path)
                        .ok()
                        .and_then(|text| serde_yml::from_str::<serde_yml::Value>(&text).ok())
                        .is_some_and(|value| {
                            value
                                .get("clashtui")
                                .and_then(|v| v.get("proxy_provider_groups"))
                                .is_some()
                        });
                    if !has_groups {
                        if let Err(e) = crate::functions::file::template::write_template_ppg(
                            &template_name,
                            &groups,
                        ) {
                            log::error!(
                                "Failed to migrate proxy_provider_groups to template '{template_name}': {e}"
                            );
                        } else {
                            log::info!(
                                "Migrated proxy_provider_groups from database to template '{template_name}'"
                            );
                        }
                    } else {
                        log::info!(
                            "Template '{template_name}' already has proxy_provider_groups, skipping migration"
                        );
                    }
                } else {
                    log::warn!(
                        "Template file '{template_name}' not found for migration — groups will be dropped on next save"
                    );
                }
            }
        }
        let data: Mutex<ProfileManager> = data.into();
        if !cfg_file.mihomo.core.config_path.is_empty() {
            cfg_file.mihomo.core.config_path =
                std::path::absolute(std::path::PathBuf::from(&cfg_file.mihomo.core.config_path))
                    .context("Failed to resolve mihomo config_path")?
                    .display()
                    .to_string();
        }
        if !cfg_file.mihomo.core.config_dir.is_empty() {
            cfg_file.mihomo.core.config_dir =
                std::path::absolute(std::path::PathBuf::from(&cfg_file.mihomo.core.config_dir))
                    .context("Failed to resolve mihomo config_dir")?
                    .display()
                    .to_string();
        }
        Ok(Self {
            cfg_file,
            data,
            external_controller: basic_info.get_external_controller(),
            proxy_addr: basic_info
                .get_proxy_addr()
                .context("Failed to determine proxy port")?,
            secret: basic_info.secret,
            global_ua: basic_info.global_ua,
        })
    }
    pub fn core_type(&self) -> CoreType {
        self.data.lock().unwrap().core_type
    }
    pub fn controller_for_core(&self) -> &str {
        match self.data.lock().unwrap().core_type {
            CoreType::Mihomo => &self.external_controller,
        }
    }
    pub fn secret_for_core(&self) -> Option<&str> {
        match self.data.lock().unwrap().core_type {
            CoreType::Mihomo => self.secret.as_deref(),
        }
    }
}

pub fn init(base_path: Option<PathBuf>) -> Result<()> {
    let config_root = {
        let path = if let Some(path) = base_path {
            path.to_path_buf()
        } else {
            load_home_dir()?
        };
        if !path.exists() {
            std::fs::create_dir_all(&path).with_context(|| {
                format!("Failed to create config directory: {}", path.display())
            })?;
        }
        ensure!(path.is_dir(), "{} is not a dir", path.display());

        let path = path
            .canonicalize()
            .context(format!("Failed to canonicalize path: {}", path.display()))?;
        std::path::absolute(&path).context(format!("{} is not an absolute path", path.display()))?
    };

    std::fs::create_dir_all(config_root.join("mihomo"))
        .context("Failed to create mihomo data directory")?;

    CONFIG_ROOT.set(config_root.clone()).ok();
    if DATA_DIR.set(config_root).is_err() {
        unreachable!("init twice")
    }

    let _write = crate::functions::file::coordination::WriteGuard::acquire()?;
    let is_first_run = !config_dir_path().join(defs::CONFIG_FILE).exists();

    if is_first_run {
        init_config()?;
    }

    // Fill in files that may be missing from old install scripts or partial setups
    {
        use std::fs;
        let path = DATA_DIR.get().unwrap();

        let mihomo_override = path.join("mihomo").join(defs::CORE_OVERRIDE_FILE);
        if !mihomo_override.exists() {
            fs::write(&mihomo_override, BasicInfo::DEFAULT)
                .with_context(|| format!("Failed to write {}", mihomo_override.display()))?;
        }
        let db = path.join(defs::DATA_FILE);
        if !db.exists() {
            ProfileManager::default().to_file()?;
        }
    }

    if _CONFIG.set(Config::load()?).is_err() {
        unreachable!("init twice")
    }

    Ok(())
}

pub fn init_config() -> Result<()> {
    use std::fs;

    let path = match DATA_DIR.get() {
        Some(path) => path,
        None => unreachable!(),
    };
    let mihomo = path.join("mihomo");

    fs::create_dir_all(&mihomo)?;

    fs::write(mihomo.join(defs::CORE_OVERRIDE_FILE), BasicInfo::DEFAULT)?;
    ConfigFile::default().to_file()?;
    ProfileManager::default().to_file()?;

    fs::create_dir(mihomo.join(defs::TEMPLATE_DIR))?;
    fs::create_dir(mihomo.join(defs::PROFILE_YAMLS_DIR))?;

    Ok(())
}

#[cfg(feature = "customized-theme")]
pub fn theme_path() -> PathBuf {
    DATA_DIR.get().unwrap().join(defs::THEME_FILE)
}
fn mihomo_dir() -> PathBuf {
    DATA_DIR.get().unwrap().join("mihomo")
}

pub fn config_dir_path() -> PathBuf {
    DATA_DIR.get().unwrap().clone()
}
pub fn core_data_dir(core_type: CoreType) -> PathBuf {
    match core_type {
        CoreType::Mihomo => mihomo_dir(),
    }
}
pub fn template_path() -> PathBuf {
    mihomo_dir().join(defs::TEMPLATE_DIR)
}

pub fn profile_yamls_path() -> PathBuf {
    mihomo_dir().join(defs::PROFILE_YAMLS_DIR)
}

pub fn load_basic() -> anyhow::Result<serde_yml::Mapping> {
    let fp = std::fs::File::open(mihomo_dir().join(defs::CORE_OVERRIDE_FILE))?;
    serde_yml::from_reader(fp).map_err(|e| e.into())
}

pub fn keymap_path() -> PathBuf {
    DATA_DIR.get().unwrap().join(defs::KEYMAP_FILE)
}

load_save!(BasicInfo, defs::CORE_OVERRIDE_FILE, no_save, "mihomo");
load_save!(ConfigFile, defs::CONFIG_FILE);
impl ProfileManager {
    pub fn from_file() -> Result<Self> {
        let path = DATA_DIR.get().unwrap().join(defs::DATA_FILE);
        let bytes =
            std::fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        Self::from_compatible_database(&path, &bytes)
    }

    fn from_compatible_database(path: &std::path::Path, bytes: &[u8]) -> Result<Self> {
        let raw: serde_yml::Mapping = serde_yml::from_slice(bytes)?;
        let database: Self = serde_yml::from_slice(bytes)?;
        let contains_legacy_data = raw
            .keys()
            .any(|key| !matches!(key.as_str(), Some("core_type" | "mihomo")))
            || raw
                .get("core_type")
                .and_then(serde_yml::Value::as_str)
                .is_some_and(|core| core != "mihomo");
        if contains_legacy_data {
            use std::io::Write;
            let backup = path.with_extension("db.before-mihomo-only");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&backup) {
                Ok(mut file) => {
                    file.write_all(bytes)?;
                    file.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("Could not preserve the legacy database"),
            }
        }
        Ok(database)
    }

    pub fn to_file(&self) -> Result<()> {
        let path = DATA_DIR.get().unwrap().join(defs::DATA_FILE);
        let bytes = serde_yml::to_string(self)?.into_bytes();
        if std::fs::read(&path).is_ok_and(|existing| existing == bytes) {
            return Ok(());
        }
        crate::functions::file::activation::atomic_write(&path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_selection_preserves_mihomo_profiles_and_original_database() {
        let root = std::env::temp_dir().join(format!("clashtui-legacy-{:x}", fastrand::u64(..)));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("clashtui.db");
        let bytes = b"core_type: singbox\nmihomo:\n  cur_profile: main\n  profiles:\n    main: File\nsingbox:\n  profiles:\n    old: Singbox\n";
        let database = ProfileManager::from_compatible_database(&path, bytes).unwrap();
        assert_eq!(database.core_type, CoreType::Mihomo);
        assert_eq!(database.get_current().unwrap().name, "main");
        assert_eq!(
            std::fs::read(path.with_extension("db.before-mihomo-only")).unwrap(),
            bytes
        );
        assert!(!serde_yml::to_string(&database).unwrap().contains("singbox"));
        assert!(ProfileManager::from_compatible_database(&path, bytes).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn core_mismatch_flag_defaults_false() {
        assert!(!is_core_mismatch());
    }

    #[test]
    fn core_mismatch_flag_toggle() {
        set_core_mismatch(true);
        assert!(is_core_mismatch());
        set_core_mismatch(false);
        assert!(!is_core_mismatch());
    }

    #[test]
    fn core_data_dir_returns_correct_subdir_per_core_type() {
        let tmp = std::env::temp_dir().join(format!("clashtui-test-{}", fastrand::u32(..)));
        std::fs::create_dir_all(&tmp).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(tmp.clone());

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            init(Some(tmp.clone())).unwrap()
        }));

        let mihomo_dir = core_data_dir(CoreType::Mihomo);
        assert!(
            mihomo_dir.ends_with("mihomo"),
            "expected path ending with 'mihomo', got: {mihomo_dir:?}"
        );
    }

    #[test]
    fn core_install_dir_is_parent_of_config_dir() {
        let config_dir = "/opt/clashtui/mihomo/config";
        let parent = std::path::Path::new(config_dir).parent().unwrap();
        assert_eq!(parent, std::path::Path::new("/opt/clashtui/mihomo"));

        let config_dir = "/opt/clashtui/mihomo/config";
        let parent = std::path::Path::new(config_dir).parent().unwrap();
        assert_eq!(parent, std::path::Path::new("/opt/clashtui/mihomo"));
    }
}
