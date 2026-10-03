macro_rules! pm {
    () => {
        crate::functions::file::coordination::database()
    };
}

pub mod activation;
pub mod coordination;
pub mod evidence;
pub mod net_resource;
pub mod panel;
pub mod profile;
pub mod template;

/// Explicit exports never replace an existing configuration or report.
pub fn export_json(path: &std::path::Path, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_all()?;
    Ok(())
}

use std::{path::PathBuf, sync::LazyLock};

pub static PROFILE_YAMLS_PATH: LazyLock<PathBuf> = LazyLock::new(crate::config::profile_yamls_path);
pub(crate) fn template_root() -> PathBuf {
    if !crate::config::initialized() {
        return crate::config::template_path();
    }
    match crate::config::CONFIG.core_type() {
        crate::config::CoreType::Mihomo => crate::config::template_path(),
    }
}
