use anyhow::Result;
use anyhow::bail;

use super::*;

pub fn handle_cli(cmd: Cmds) -> Result<()> {
    let Some(command) = cmd.command else {
        return Ok(());
    };

    match command {
        ArgCommand::Panel => crate::functions::file::panel::prepare(),
        ArgCommand::Web { listen, token_file } => crate::functions::web::serve(listen, &token_file),
        command @ ArgCommand::Manage { .. } => super::operations::manage(command),
        ArgCommand::Core { command } => super::operations::core(command),
        ArgCommand::Profile { command } => handle_profile(command),
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        ArgCommand::Service { command } => handle_service(command),
        ArgCommand::Mode {
            mode,
            close_connections,
        } => handle_mode(mode, close_connections),
        ArgCommand::Update { ci, target } => handle_update(ci, target),
    }
}

// ── Profile ──────────────────────────────────────────────────────────

fn handle_profile(command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::Update {
            all,
            name,
            with_proxy,
            without_proxyprovider,
            r#type: type_filter,
        } => {
            let profiles: Vec<crate::config::database::Profile> = if all {
                crate::functions::file::profile::db::get_all()
            } else if let Some(name) = &name {
                match crate::functions::file::profile::db::get(name) {
                    Some(pf) => vec![pf],
                    None => {
                        eprintln!("Profile not found: {name}");
                        std::process::exit(1);
                    }
                }
            } else {
                anyhow::bail!("No profile selected! Use --all or --name <NAME>.");
            };

            let profiles: Vec<_> = if let Some(filter) = &type_filter {
                profiles
                    .into_iter()
                    .filter(|pf| filter.matches(&pf.dtype))
                    .collect()
            } else {
                profiles
            };

            if profiles.is_empty() {
                if type_filter.is_some() {
                    println!("No profiles match the given type filter.");
                } else if all {
                    println!("No profiles in database.");
                }
                return Ok(());
            }

            let rt = tokio::runtime::Runtime::new()?;
            let mut failures = Vec::new();
            for pf in &profiles {
                println!("Updating profile: {}", pf.name);
                if without_proxyprovider {
                    let mut pm = crate::functions::file::coordination::database();
                    pm.set_no_pp(&pf.name, true);
                    pm.to_file()?;
                }
                let pf = crate::functions::file::profile::db::get(&pf.name)
                    .ok_or_else(|| anyhow::anyhow!("Profile was removed; refresh and retry"))?;
                let name = pf.name.clone();
                let with_proxy = with_proxy.unwrap_or(pf.update_with_proxy);
                match rt.block_on(crate::functions::file::profile::update_profile(
                    pf, with_proxy,
                )) {
                    Ok(result) => {
                        println!(
                            "  Updated: {} ({} resources)",
                            result.name,
                            result.net_updates.len()
                        );
                        if !result.net_updates.is_empty() {
                            println!(
                                "{}",
                                crate::functions::file::net_resource::format_net_updates(
                                    &result.net_updates
                                )
                            );
                        }
                        if result.net_updates.iter().any(|resource| !resource.ok) {
                            failures.push(format!("{name}: some subscription resources failed"));
                        }
                    }
                    Err(e) => {
                        eprintln!("  Error: {e}");
                        failures.push(format!("{name}: {e}"));
                    }
                }
            }
            println!("Done.");
            anyhow::ensure!(
                failures.is_empty(),
                "Some profiles failed to update: {}",
                failures.join("; ")
            );
            Ok(())
        }
        ProfileCommand::Select { name } => {
            if let Some(name) = name {
                let Some(pf) = crate::functions::file::profile::db::get(&name) else {
                    eprintln!("Profile not found in database: {name}");
                    std::process::exit(1);
                };
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(crate::functions::file::profile::select(pf))?;
                println!("Profile selected: {name}");
            } else {
                let current = crate::functions::file::profile::db::get_current();
                println!("Current Profile: {}", current.name);
            }
            Ok(())
        }
        ProfileCommand::List {
            name_only,
            r#type: type_filter,
        } => {
            let pfs = crate::functions::file::profile::db::get_all();
            let mut pfs: Vec<_> = if let Some(filter) = &type_filter {
                pfs.into_iter()
                    .filter(|pf| filter.matches(&pf.dtype))
                    .collect()
            } else {
                pfs
            };
            pfs.sort_by(|a, b| a.name.cmp(&b.name));
            if pfs.is_empty() {
                if type_filter.is_some() {
                    println!("No profiles match the given type filter.");
                } else {
                    println!("No profiles found.");
                }
                return Ok(());
            }
            for pf in &pfs {
                if name_only {
                    println!("{}", pf.name);
                } else {
                    println!(
                        "{}: {}",
                        pf.name,
                        pf.dtype.get_domain().as_deref().unwrap_or("Unknown")
                    );
                }
            }
            Ok(())
        }
    }
}

// ── Service ──────────────────────────────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn handle_service(command: ServiceCommand) -> Result<()> {
    match command {
        ServiceCommand::Start => {
            println!(
                "{}",
                crate::functions::command::start_core_service(crate::config::CONFIG.core_type())?
            );
            Ok(())
        }
        ServiceCommand::Status => super::operations::print_json(
            &serde_json::json!({"core":crate::config::CONFIG.core_type().to_string(),"running":crate::functions::command::is_core_service_running()}),
        ),
        ServiceCommand::StopAll => {
            println!("{}", crate::functions::command::stop_all_services()?);
            Ok(())
        }
        #[cfg(windows)]
        ServiceCommand::Install => {
            println!(
                "{}",
                crate::functions::command::install_core_service(crate::config::CONFIG.core_type())?
            );
            Ok(())
        }
        #[cfg(windows)]
        ServiceCommand::Uninstall => {
            println!(
                "{}",
                crate::functions::command::uninstall_core_service(
                    crate::config::CONFIG.core_type()
                )?
            );
            Ok(())
        }
        #[cfg(windows)]
        ServiceCommand::SystemProxy => {
            let state = crate::functions::management::snapshot()?;
            super::operations::print_json(&crate::functions::management::execute(
                serde_json::json!({"action":"system_proxy","revision":state["revision"]}),
            )?)
        }
        ServiceCommand::Restart { soft } => {
            if soft {
                crate::functions::restful::control::restart(None)
                    .map_err(|e| anyhow::anyhow!("Soft restart failed: {e}"))?;
                println!("Core restarted (soft).");
            } else {
                let output = crate::functions::command::restart_service()?;
                println!("{output}");
            }
            Ok(())
        }
        ServiceCommand::Stop => {
            let output = crate::functions::command::stop_service()?;
            println!("{output}");
            Ok(())
        }
    }
}

// ── Mode ─────────────────────────────────────────────────────────────

fn handle_mode(mode: Option<ModeCommand>, close_connections: bool) -> Result<()> {
    match mode {
        None => {
            anyhow::ensure!(
                !close_connections,
                "Choose a mode before requesting connection closure"
            );
            let config = crate::functions::restful::config::fetch()
                .map_err(|e| anyhow::anyhow!("Failed to fetch config: {e}"))?;
            println!("{}", config.mode);
            Ok(())
        }
        Some(mode_cmd) => {
            let mode_str = match mode_cmd {
                ModeCommand::Rule => "rule".to_owned(),
                ModeCommand::Direct => "direct".to_owned(),
                ModeCommand::Global => "global".to_owned(),
                ModeCommand::Set { value } => value,
            };
            let payload = serde_json::json!({"mode": mode_str});
            let actual = crate::functions::restful::config::patch_checked(payload)
                .map_err(|e| anyhow::anyhow!("Failed to set mode: {e}"))?;
            println!("Mode set to: {}", actual.mode);
            if close_connections {
                crate::functions::restful::connection::terminate_all_connections().map_err(
                    |error| {
                        anyhow::anyhow!("Mode changed, but closing connections failed: {error}")
                    },
                )?;
            }
            Ok(())
        }
    }
}

// ── Update ───────────────────────────────────────────────────────────

fn handle_update(ci: bool, target: Target) -> Result<()> {
    match target {
        Target::Clashtui => update_clashtui(ci),
        Target::Mihomo => update_mihomo(ci),
    }
}

fn update_clashtui(ci: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let repo = "JohanChane/clashtui";
    let release = fetch_latest_release(repo, ci)?;
    let latest = release.tag_name.trim_start_matches('v');

    if latest == current {
        println!("Already up to date (v{current}).");
        return Ok(());
    }

    println!("New version available: v{latest} (current: v{current})");

    let asset = find_native_asset(&release.assets)?;
    println!("Downloading {}...", asset.name);
    download_and_replace(asset)?;
    println!("Updated to v{latest}.");
    Ok(())
}

fn update_mihomo(ci: bool) -> Result<()> {
    anyhow::ensure!(
        crate::functions::management::local_controller(crate::config::CONFIG.controller_for_core()),
        "Remote controllers cannot update a local core executable"
    );
    let current = crate::functions::restful::control::version()
        .map_err(|e| anyhow::anyhow!("Failed to fetch core version: {e}"))?;
    let version: serde_json::Value = serde_json::from_str(&current)?;
    let current = version["version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Core response is missing its version"))?;

    let repo = "MetaCubeX/mihomo";
    let release = fetch_latest_release(repo, ci)?;
    let latest = release.tag_name.trim_start_matches('v');

    if latest == current.trim_start_matches('v') {
        println!("Already up to date (v{latest}).");
        return Ok(());
    }

    println!("New version available: v{latest} (current: {current})");

    let _write = crate::functions::file::coordination::WriteGuard::acquire()?;
    let mihomo_path =
        std::path::PathBuf::from(&crate::config::CONFIG.cfg_file.mihomo.core.bin_path);
    anyhow::ensure!(
        mihomo_path.is_absolute(),
        "Mihomo update requires an absolute configured bin_path"
    );

    let asset = find_native_asset(&release.assets)?;
    println!("Downloading {}...", asset.name);
    download_to_path(asset, &mihomo_path)?;
    println!("Updated mihomo to v{latest}.");
    Ok(())
}

// ── GitHub helpers ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn fetch_latest_release(repo: &str, ci: bool) -> Result<GhRelease> {
    let url = if ci {
        format!("https://api.github.com/repos/{repo}/releases?per_page=1")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    };

    let mut releases: Vec<GhRelease> = if ci {
        minreq::get(url)
            .with_header("User-Agent", "clashtui")
            .with_timeout(10)
            .send()
            .map_err(|e| anyhow::anyhow!("Failed to fetch releases: {e}"))?
            .json()
            .map_err(|e| anyhow::anyhow!("Failed to parse releases: {e}"))?
    } else {
        vec![
            minreq::get(url)
                .with_header("User-Agent", "clashtui")
                .with_timeout(10)
                .send()
                .map_err(|e| anyhow::anyhow!("Failed to fetch latest release: {e}"))?
                .json()
                .map_err(|e| anyhow::anyhow!("Failed to parse release: {e}"))?,
        ]
    };

    if releases.is_empty() {
        bail!("No releases found");
    }
    Ok(releases.remove(0))
}

fn find_native_asset(assets: &[GhAsset]) -> Result<&GhAsset> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "armv7",
        other => other,
    };
    let prefix = format!("-{os}-{arch}-");
    let mut candidates: Vec<_> = assets
        .iter()
        .filter(|asset| {
            asset.name.contains(&prefix)
                && !asset.name.contains("musl")
                && !asset.name.contains("-v2-")
                && !asset.name.contains("-v3-")
                && !asset.name.contains("-v9-")
                && !asset.name.contains("-go1")
                && !asset.name.contains("android")
                && [".gz", ".tgz", ".zip", ".exe"]
                    .iter()
                    .any(|suffix| asset.name.ends_with(suffix))
        })
        .collect();
    let rank = |asset: &&GhAsset| {
        if asset.name.contains("-compatible-") {
            0
        } else if asset.name.contains("-amd64-v1-") {
            1
        } else {
            2
        }
    };
    candidates.sort_by_key(rank);
    if let Some(first) = candidates.first() {
        let preferred = rank(first);
        candidates.retain(|asset| rank(asset) == preferred);
    }
    anyhow::ensure!(
        candidates.len() == 1,
        "Expected one {os}/{arch} release asset, found {}; executable was preserved",
        candidates.len()
    );
    Ok(candidates[0])
}

fn download_and_replace(asset: &GhAsset) -> Result<()> {
    let exe = std::env::current_exe()?;
    let new_path = crate::functions::file::activation::TemporaryFile::create_beside(&exe, &[])?;
    download_to_path(asset, &new_path.0)?;
    self_replace::self_replace(&new_path.0)?;
    Ok(())
}

fn download_to_path(asset: &GhAsset, dest: &std::path::Path) -> Result<()> {
    let expected = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .ok_or_else(|| {
            anyhow::anyhow!("Release asset has no SHA-256 digest; executable was preserved")
        })?;
    let response = minreq::get(&asset.browser_download_url)
        .with_header("User-Agent", "clashtui")
        .with_timeout(300)
        .send_lazy()
        .map_err(|e| anyhow::anyhow!("Download failed: {e}"))?;
    anyhow::ensure!(
        (200..300).contains(&response.status_code),
        "Release download failed: HTTP {}; executable was preserved",
        response.status_code
    );

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use sha2::{Digest, Sha256};
    let downloaded = read_bounded(response)?;
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&downloaded)) == expected,
        "Release SHA-256 mismatch; executable was preserved"
    );
    let bytes = unpack_executable(&asset.name, &downloaded)?;
    verify_native_executable(&bytes)?;
    let staged = crate::functions::file::activation::TemporaryFile::create_beside(dest, &bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staged.0)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staged.0, perms)?;
    }
    std::fs::rename(&staged.0, dest)?;
    Ok(())
}

fn read_bounded(reader: impl std::io::Read) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader.take(128 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 128 * 1024 * 1024,
        "Expanded release exceeds 128 MiB"
    );
    Ok(bytes)
}

fn unpack_executable(name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let binary_name = if name.starts_with("clashtui-") {
        "clashtui"
    } else {
        "mihomo"
    };
    let candidate = |path: &std::path::Path| {
        path.file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                value == binary_name
                    || value == format!("{binary_name}.exe")
                    || (binary_name == "clashtui"
                        && value.starts_with("clashtui-")
                        && !value.ends_with(".txt"))
            })
    };
    let mut binaries = Vec::new();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let expanded = read_bounded(flate2::read::GzDecoder::new(bytes))?;
        let mut archive = tar::Archive::new(expanded.as_slice());
        for entry in archive.entries()? {
            let entry = entry?;
            if entry.header().entry_type().is_file() && candidate(&entry.path()?) {
                binaries.push(read_bounded(entry)?);
            }
        }
    } else if name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            if entry.is_file() && candidate(std::path::Path::new(entry.name())) {
                binaries.push(read_bounded(entry)?);
            }
        }
    } else if name.ends_with(".gz") {
        return read_bounded(flate2::read::GzDecoder::new(bytes));
    } else {
        return read_bounded(bytes);
    }
    anyhow::ensure!(
        binaries.len() == 1,
        "Archive must contain exactly one application executable"
    );
    Ok(binaries.remove(0))
}

fn verify_native_executable(bytes: &[u8]) -> Result<()> {
    let arch = std::env::consts::ARCH;
    let valid = match std::env::consts::OS {
        "linux" => {
            let machine = match arch {
                "x86_64" => 62,
                "aarch64" => 183,
                "arm" => 40,
                "x86" => 3,
                _ => 0,
            };
            bytes.len() >= 64
                && bytes.starts_with(b"\x7fELF")
                && bytes[5] == 1
                && u16::from_le_bytes([bytes[18], bytes[19]]) == machine
                && machine != 0
                && bytes[4]
                    == if matches!(arch, "x86_64" | "aarch64") {
                        2
                    } else {
                        1
                    }
        }
        "macos" => {
            let cpu = match arch {
                "x86_64" => 0x01000007,
                "aarch64" => 0x0100000c,
                _ => 0,
            };
            bytes.len() >= 32
                && bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
                && u32::from_le_bytes(bytes[4..8].try_into().unwrap()) == cpu
                && cpu != 0
        }
        "windows" => {
            let machine = match arch {
                "x86_64" => 0x8664,
                "aarch64" => 0xaa64,
                "x86" => 0x14c,
                _ => 0,
            };
            if bytes.len() >= 64 && bytes.starts_with(b"MZ") {
                let offset = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
                bytes
                    .get(offset..offset.saturating_add(6))
                    .is_some_and(|header| {
                        header.starts_with(b"PE\0\0")
                            && u16::from_le_bytes([header[4], header[5]]) == machine
                            && machine != 0
                    })
            } else {
                false
            }
        }
        _ => false,
    };
    anyhow::ensure!(
        valid,
        "Release executable format or architecture mismatch; executable was preserved"
    );
    Ok(())
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn no_matching_or_ambiguous_asset_never_falls_back_to_another_platform() {
        let asset = |name: &str| GhAsset {
            name: name.to_owned(),
            browser_download_url: "https://example.org".into(),
            digest: None,
        };
        assert!(find_native_asset(&[asset("source.tar.gz")]).is_err());
        let os = if cfg!(target_os = "macos") {
            "darwin"
        } else {
            std::env::consts::OS
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            "arm" => "armv7",
            other => other,
        };
        let matching = format!("clashtui-{os}-{arch}-v1.gz");
        let assets = [asset("source.tar.gz"), asset(&matching)];
        assert_eq!(find_native_asset(&assets).unwrap().name, matching);
        assert!(find_native_asset(&[asset(&matching), asset(&matching)]).is_err());
        if arch == "amd64" {
            let compatible = format!("mihomo-{os}-amd64-compatible-v1.gz");
            let variants = [
                asset(&format!("mihomo-{os}-amd64-v1-v1.gz")),
                asset(&format!("mihomo-{os}-amd64-v2-v1.gz")),
                asset(&format!("mihomo-{os}-amd64-v1-v1.deb")),
                asset(&format!("mihomo-{os}-amd64-v1-go120-v1.gz")),
                asset(&compatible),
            ];
            assert_eq!(find_native_asset(&variants).unwrap().name, compatible);
        }
    }
    #[test]
    fn gzip_and_zip_are_decoded_and_non_executables_are_rejected() {
        let payload = b"fake executable payload";
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        assert_eq!(
            unpack_executable("clashtui-linux-amd64-v1.gz", &encoder.finish().unwrap()).unwrap(),
            payload
        );
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file(
                "clashtui-windows-amd64.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(payload).unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert_eq!(
            unpack_executable("clashtui-windows-amd64-v1.zip", &bytes).unwrap(),
            payload
        );
        assert!(verify_native_executable(payload).is_err());
        assert!(unpack_executable("clashtui-linux-amd64-v1.gz", b"invalid gzip").is_err());
    }
    #[test]
    fn unverifiable_release_preserves_the_existing_executable_without_downloading() {
        let dir = std::env::temp_dir().join(format!("clashtui-release-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("program");
        std::fs::write(&target, b"old").unwrap();
        let asset = GhAsset {
            name: "clashtui.gz".into(),
            browser_download_url: "http://127.0.0.1:1".into(),
            digest: None,
        };
        assert!(
            download_to_path(&asset, &target)
                .unwrap_err()
                .to_string()
                .contains("SHA-256")
        );
        assert_eq!(std::fs::read(target).unwrap(), b"old");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
