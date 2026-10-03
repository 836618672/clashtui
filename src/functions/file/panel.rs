//! Reproducible MetaCubeXD deployment for Mihomo.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
pub const VERSION: &str = "v1.273.1";
pub const SHA256: &str = "a178e00b67acabcda2dcef00afa90be6a7bb261e466a67dad58c8478d9553603";
const URL: &str =
    "https://github.com/MetaCubeX/metacubexd/releases/download/v1.273.1/compressed-dist.tgz";

fn target_directory() -> Result<std::path::PathBuf> {
    use crate::config::{CONFIG, CoreType};
    let (root, ui) = match CONFIG.core_type() {
        CoreType::Mihomo => {
            let overlay = crate::config::load_basic()?;
            let ui = overlay
                .get(serde_yml::Value::String("external-ui".to_owned()))
                .and_then(serde_yml::Value::as_str)
                .unwrap_or("uis/metacubexd")
                .to_owned();
            (CONFIG.cfg_file.mihomo.core.config_dir.clone(), ui)
        }
    };
    anyhow::ensure!(
        !ui.is_empty()
            && Path::new(&ui)
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "Panel directory must be a relative path within the core configuration directory"
    );
    Ok(Path::new(&root).join(ui))
}

pub fn deployment_state() -> serde_json::Value {
    let installed = target_directory().ok().and_then(|target| {
        if !target.join("index.html").is_file() {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(target.join("clashtui-panel.json")).ok()?,
        )
        .ok()
    });
    let recorded = installed
        .as_ref()
        .is_some_and(|value| value["version"] == VERSION && value["sha256"] == SHA256);
    serde_json::json!({"target":VERSION,"installed":installed,"verified_install_record":recorded})
}

pub fn prepare() -> Result<()> {
    let _write = super::coordination::WriteGuard::acquire()?;
    let target = target_directory()?;
    let response = minreq::get(URL).with_timeout(120).send_lazy()?;
    anyhow::ensure!(
        response.status_code == 200,
        "Panel download failed: HTTP {}",
        response.status_code
    );
    use std::io::Read;
    let mut downloaded = Vec::new();
    Read::take(response, 64 * 1024 * 1024 + 1).read_to_end(&mut downloaded)?;
    let bytes = downloaded.as_slice();
    anyhow::ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Panel archive exceeds 64 MiB"
    );
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(bytes)) == SHA256,
        "Panel archive SHA-256 mismatch; existing panel preserved"
    );
    let staging = target.with_file_name(format!(".metacubexd-staging-{:016x}", fastrand::u64(..)));
    let result = (|| {
        std::fs::create_dir_all(&staging)?;
        extract(bytes, &staging)?;
        anyhow::ensure!(
            staging.join("index.html").is_file(),
            "Panel archive has no index.html"
        );
        super::activation::atomic_write(
            &staging.join("clashtui-panel.json"),
            serde_json::json!({"version": VERSION, "sha256": SHA256})
                .to_string()
                .as_bytes(),
        )?;
        let backup =
            target.with_file_name(format!(".metacubexd-backup-{:016x}", fastrand::u64(..)));
        let existed = target.exists();
        if existed {
            std::fs::rename(&target, &backup)?;
        }
        if let Err(error) = std::fs::rename(&staging, &target) {
            if existed {
                std::fs::rename(&backup, &target).context(
                    "Panel replacement failed and the previous panel could not be restored",
                )?;
            }
            return Err(error.into());
        }
        if existed {
            let _ = std::fs::remove_dir_all(backup);
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(staging);
    result
}

fn extract(bytes: &[u8], directory: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            path.components().all(|part| matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )),
            "Unsafe panel archive path"
        );
        let kind = entry.header().entry_type();
        anyhow::ensure!(
            kind.is_file() || kind.is_dir(),
            "Panel archive contains a link or special file"
        );
        total = total
            .checked_add(entry.size())
            .context("Archive size overflow")?;
        anyhow::ensure!(total <= 256 * 1024 * 1024, "Expanded panel exceeds 256 MiB");
        anyhow::ensure!(
            entry.unpack_in(directory)?,
            "Archive entry escaped the panel directory"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_links_cannot_write_outside_the_panel() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_link_name("/tmp/outside-panel").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "link", std::io::empty())
            .unwrap();
        let archive = builder.into_inner().unwrap();
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write;
        gzip.write_all(&archive).unwrap();
        assert!(
            extract(&gzip.finish().unwrap(), Path::new("/tmp"))
                .unwrap_err()
                .to_string()
                .contains("link")
        );
    }
}
