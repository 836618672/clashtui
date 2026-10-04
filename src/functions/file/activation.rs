//! Prepare, validate, activate, and restore the previous configuration on error.
use anyhow::{Context, Result};
use std::path::Path;

pub struct TemporaryFile(pub std::path::PathBuf);
impl TemporaryFile {
    pub fn create_beside(path: &Path, bytes: &[u8]) -> Result<Self> {
        use std::io::Write;
        let parent = path.parent().context("File has no parent directory")?;
        let staged = parent.join(format!(".clashtui-check-{:016x}.json", fastrand::u64(..)));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged)?;
        let temporary = Self(staged);
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(temporary)
    }
}
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("File has no parent directory")?;
    std::fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".clashtui-{:016x}.tmp", fastrand::u64(..)));
    let result = (|| {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged)?;
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        #[cfg(unix)]
        if !path.exists()
            && crate::config::initialized()
            && !crate::config::CONFIG.cfg_file.mihomo.core_service.is_user
            && let (Ok(parent), Ok(core)) = (
                parent.canonicalize(),
                std::path::Path::new(&crate::config::CONFIG.cfg_file.mihomo.core.config_dir)
                    .canonicalize(),
            )
            && parent.starts_with(core)
        {
            use std::os::unix::fs::PermissionsExt;
            // System-service deployments share core files with the configured
            // group. Keep application credentials/database outside that scope
            // private, and preserve the permissions of every existing file.
            file.set_permissions(std::fs::Permissions::from_mode(0o660))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&staged, path)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(staged);
    result
}

pub fn activate(
    path: &Path,
    bytes: &[u8],
    validate: impl FnOnce(&Path) -> Result<()>,
    mut reload: impl FnMut(&Path) -> Result<()>,
    commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let _write = if crate::config::initialized() {
        Some(super::coordination::WriteGuard::acquire()?)
    } else {
        None
    };
    if let Some(session) = crate::functions::restful::session::CoreSession::try_current() {
        anyhow::ensure!(
            session.is_current(),
            "Core changed before activation; refresh and retry"
        );
    }
    let old = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let staged = path.with_file_name(format!(
        ".clashtui-check-{:016x}.{}",
        fastrand::u64(..),
        path.extension().and_then(|e| e.to_str()).unwrap_or("yaml")
    ));
    atomic_write(&staged, bytes)?;
    let validation = validate(&staged);
    let _ = std::fs::remove_file(&staged);
    validation.context("Generated configuration failed validation; active file was preserved")?;
    atomic_write(path, bytes)?;
    if let Err(error) = reload(path).and_then(|_| commit()) {
        let recovery = match old {
            Some(bytes) => atomic_write(path, &bytes).and_then(|_| reload(path)),
            None => Err(anyhow::anyhow!(
                "No previous configuration was available; the new file was kept and the running core must be checked"
            )),
        };
        return match recovery {
            Ok(()) => Err(anyhow::anyhow!(
                "Activation failed; previous file restored: {error:#}"
            )),
            Err(recovery) => Err(anyhow::anyhow!(
                "Activation failed: {error:#}; recovery failed: {recovery:#}. Check the running core before retrying"
            )),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_temporary_files_are_unique_and_cleaned_on_error() {
        let dir = std::env::temp_dir().join(format!("clashtui-validation-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join("config.json");
        let existing = dir.join("config.raw.json");
        std::fs::write(&existing, b"keep").unwrap();
        let first = TemporaryFile::create_beside(&original, b"one").unwrap();
        let second = TemporaryFile::create_beside(&original, b"two").unwrap();
        assert_ne!(first.0, second.0);
        let first_path = first.0.clone();
        drop(first);
        assert!(!first_path.exists());
        assert_eq!(std::fs::read(&second.0).unwrap(), b"two");
        drop(second);
        assert_eq!(std::fs::read(existing).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn first_activation_cannot_claim_rollback_after_commit_failure() {
        let dir =
            std::env::temp_dir().join(format!("clashtui-first-activation-{}", fastrand::u64(..)));
        let path = dir.join("config.yaml");
        let error = activate(
            &path,
            b"new",
            |_| Ok(()),
            |_| Ok(()),
            || anyhow::bail!("database failed"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("No previous configuration"));
        assert!(error.to_string().contains("recovery failed"));
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_config_preserves_file_and_failed_reload_restores_previous_bytes() {
        let dir = std::env::temp_dir().join(format!("clashtui-activation-{}", fastrand::u64(..)));
        let path = dir.join("config.yaml");
        atomic_write(&path, b"old").unwrap();
        assert!(
            activate(
                &path,
                b"invalid",
                |_| anyhow::bail!("invalid"),
                |_| panic!("must not reload"),
                || panic!("must not commit")
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        let mut calls = 0;
        let error = activate(
            &path,
            b"new",
            |_| Ok(()),
            |p| {
                calls += 1;
                if calls == 1 {
                    anyhow::bail!("reload failed");
                }
                assert_eq!(std::fs::read(p).unwrap(), b"old");
                Ok(())
            },
            || panic!("must not commit"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("restored"));
        assert!(error.to_string().contains("reload failed"));
        assert_eq!(calls, 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
