use anyhow::{Context, Result, ensure};
use std::{io::Read, path::Path};
use tiny_http::Header;

pub(super) fn load_token(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path).context("Read management token metadata")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Management token must be a regular file, not a symlink"
    );
    let mut file = std::fs::File::open(path).context("Read management token file")?;
    let metadata = file.metadata()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "Management token file must not be accessible by group or others; use chmod 600"
        );
    }
    ensure!(metadata.len() <= 4096, "Management token file is too large");
    let mut token = String::new();
    (&mut file).take(4097).read_to_string(&mut token)?;
    ensure!(token.len() <= 4096, "Management token file is too large");
    let token = token.trim();
    ensure!(
        token.len() >= 24 && !token.chars().any(char::is_whitespace),
        "Use a management token with at least 24 characters and no whitespace"
    );
    Ok(token.to_owned())
}

pub(super) fn authorized(headers: &[Header], token: &str) -> bool {
    let expected = format!("Bearer {token}");
    headers.iter().any(|header| {
        if !header.field.equiv("Authorization") {
            return false;
        }
        let actual = header.value.as_str().as_bytes();
        if actual.len() != expected.len() {
            return false;
        }
        actual
            .iter()
            .zip(expected.as_bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_requires_the_exact_independent_credential() {
        let header = |value| vec![Header::from_bytes("Authorization", value).unwrap()];
        assert!(authorized(
            &header("Bearer management-token"),
            "management-token"
        ));
        for value in [
            "Bearer core-secret",
            "Bearer management-tokeN",
            "Bearer management-token ",
            "management-token",
        ] {
            assert!(!authorized(&header(value), "management-token"));
        }
        assert!(!authorized(&[], "management-token"));
    }
}

#[cfg(all(test, unix))]
mod file_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn token_files_require_private_permissions_regular_content_and_a_valid_credential() {
        let root = std::env::temp_dir().join(format!("clashtui-token-{}", fastrand::u64(..)));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("token");
        assert!(load_token(&path).is_err());
        std::fs::write(&path, "management-token-1234567890\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(load_token(&path).unwrap(), "management-token-1234567890");
        for mode in [0o644, 0o640, 0o606] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(load_token(&path).is_err());
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        for text in [
            "short".to_owned(),
            "management token-1234567890".to_owned(),
            "x".repeat(4097),
        ] {
            std::fs::write(&path, text).unwrap();
            assert!(load_token(&path).is_err());
        }
        symlink(&path, root.join("link")).unwrap();
        assert!(load_token(&root.join("link")).is_err());
        assert!(load_token(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
