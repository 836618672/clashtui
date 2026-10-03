#[cfg_attr(target_os = "linux", path = "command/linux.rs")]
#[cfg_attr(target_os = "macos", path = "command/macos.rs")]
#[cfg_attr(target_os = "windows", path = "command/windows.rs")]
mod platform;
mod utils;

use crate::config::CONFIG;
use crate::config::{CoreType, ServiceController};
use anyhow::Result;
use std::{path::Path, process::Command};

pub use platform::*;
use utils::*;

thread_local! { pub static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

pub fn check_config(profile_path: &Path) -> anyhow::Result<()> {
    match CONFIG.core_type() {
        CoreType::Mihomo => {
            let cfg = &CONFIG.cfg_file.mihomo.core;
            let output = Command::new(&cfg.bin_path)
                .args(["-t", "-d", &cfg.config_dir, "-f"])
                .arg(profile_path)
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to run mihomo -t: {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "mihomo -t failed:\n{}",
                    stringify_output(output)
                ))
            }
        }
    }
}

/// Preserve the core's diagnostic output for all three clients.
pub fn configuration_test(profile_path: &Path) -> anyhow::Result<serde_json::Value> {
    let cfg = &CONFIG.cfg_file.mihomo.core;
    let output = Command::new(&cfg.bin_path)
        .args(["-t", "-d", &cfg.config_dir, "-f"])
        .arg(profile_path)
        .output()?;
    Ok(
        serde_json::json!({"valid":output.status.success(),"exit_code":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr),"activated":false}),
    )
}

pub fn is_core_service_running() -> Option<bool> {
    core_service_running(CONFIG.core_type())
}

fn core_service_running(ct: CoreType) -> Option<bool> {
    let (service_name, is_user, csc) = match ct {
        CoreType::Mihomo => (
            &CONFIG.cfg_file.mihomo.core_service.service_name,
            CONFIG.cfg_file.mihomo.core_service.is_user,
            &CONFIG.cfg_file.mihomo.core_service,
        ),
    };
    let host = &ServiceController::from_config(csc);

    if service_name.is_empty() {
        return None;
    }

    #[cfg(target_os = "windows")]
    if matches!(host, ServiceController::Nssm) {
        let s = nssm_status(service_name);
        return Some(s == "active");
    }

    if matches!(host, ServiceController::OpenRc) {
        let mut args = vec![];
        if is_user {
            args.push("--user");
        }
        args.push(service_name.as_str());
        args.push("status");
        return std::process::Command::new(host.bin_name())
            .args(&args)
            .output()
            .map(|o| Some(String::from_utf8_lossy(&o.stdout).contains("started")))
            .unwrap_or(None);
    }

    if matches!(host, ServiceController::Systemd) {
        let mut args = vec!["is-active"];
        if is_user {
            args.push("--user");
        }
        args.push(service_name);
        return std::process::Command::new(host.bin_name())
            .args(&args)
            .output()
            .map(|o| Some(String::from_utf8_lossy(&o.stdout).trim() == "active"))
            .unwrap_or(None);
    }

    #[cfg(target_os = "macos")]
    if matches!(host, ServiceController::Launchd) && is_user {
        let uid = unsafe { libc::getuid() };
        return std::process::Command::new("launchctl")
            .args(["print", &format!("gui/{uid}/{service_name}")])
            .output()
            .map(|o| Some(String::from_utf8_lossy(&o.stdout).contains("state = running")))
            .unwrap_or(None);
    }

    None
}

fn svc_operation(op: &str, core_type: Option<CoreType>) -> Result<String> {
    let _write = crate::functions::file::coordination::WriteGuard::acquire()?;
    let ct = core_type.unwrap_or(CONFIG.core_type());
    let controller = match ct {
        CoreType::Mihomo => &CONFIG.external_controller,
    };
    anyhow::ensure!(
        crate::functions::management::local_controller(controller),
        "Remote core endpoints cannot control local services"
    );

    let (service_name, is_user, csc) = match ct {
        CoreType::Mihomo => (
            &CONFIG.cfg_file.mihomo.core_service.service_name,
            CONFIG.cfg_file.mihomo.core_service.is_user,
            &CONFIG.cfg_file.mihomo.core_service,
        ),
    };
    let host = &ServiceController::from_config(csc);

    if matches!(host, ServiceController::Launchd) {
        return launchd_operation(op, service_name, is_user);
    }

    #[cfg(target_os = "windows")]
    if matches!(host, ServiceController::Nssm) {
        return nssm_svc_operation(op, service_name, ct);
    }

    let svc_args = host.args(op, service_name, is_user);
    if is_user {
        exec(host.bin_name(), svc_args)
    } else {
        exec_sudo(host.bin_name(), svc_args)
    }
}

#[cfg(target_os = "windows")]
fn nssm_svc_operation(op: &str, service_name: &str, ct: CoreType) -> Result<String> {
    match op {
        "start" | "stop" | "restart" | "reload" => {
            let op = if op == "reload" { "restart" } else { op };
            let args = [op, service_name];
            platform::nssm_runas_or_direct(service_name, &args)
        }
        "install" => {
            let bin_path = match ct {
                CoreType::Mihomo => &CONFIG.cfg_file.mihomo.core.bin_path,
            };
            let launch_args = platform::nssm_launch_args(ct);
            let launch_strs: Vec<&str> = launch_args.iter().map(|s| s.as_str()).collect();
            platform::nssm_install(service_name, bin_path, &launch_strs)
        }
        "remove" => platform::nssm_uninstall(service_name),
        _ => Err(anyhow::anyhow!("Unknown nssm operation: {op}")),
    }
}

fn launchd_plist_path(service_name: &str, is_user: bool) -> String {
    if is_user {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Library/LaunchAgents/{service_name}.plist")
    } else {
        format!("/Library/LaunchDaemons/{service_name}.plist")
    }
}

fn launchd_operation(op: &str, service_name: &str, is_user: bool) -> Result<String> {
    let plist = launchd_plist_path(service_name, is_user);

    let do_exec = |args: Vec<&str>| -> Result<String> {
        if is_user {
            exec("launchctl", args)
        } else {
            exec_sudo("launchctl", args)
        }
    };

    match op {
        "start" => do_exec(vec!["load", &plist]),
        "stop" => do_exec(vec!["unload", &plist]),
        "restart" | "reload" => {
            // Best-effort unload, then load
            let _ = do_exec(vec!["unload", &plist]);
            do_exec(vec!["load", &plist])
        }
        _ => Err(anyhow::anyhow!("Unknown launchd operation: {op}")),
    }
}

pub fn stop_core_service(core_type: CoreType) -> Result<String> {
    svc_operation("stop", Some(core_type))
}

pub fn start_core_service(core_type: CoreType) -> Result<String> {
    let output = svc_operation("start", Some(core_type))?;
    crate::functions::restful::config::wait_until_ready()
        .map_err(|error| anyhow::anyhow!("Service start was accepted, but Mihomo is not ready: {error}. Check service status before retrying."))?;
    Ok(output)
}

#[cfg(windows)]
pub fn install_core_service(core_type: CoreType) -> Result<String> {
    svc_operation("install", Some(core_type))
}

#[cfg(windows)]
pub fn uninstall_core_service(core_type: CoreType) -> Result<String> {
    svc_operation("remove", Some(core_type))
}

pub fn restart_service() -> Result<String> {
    let output = svc_operation("restart", None)?;
    crate::functions::restful::config::wait_until_ready()
        .map_err(|error| anyhow::anyhow!("Service restart was accepted, but Mihomo is not ready: {error}. Check service status before retrying."))?;
    Ok(output)
}

pub fn stop_service() -> Result<String> {
    svc_operation("stop", None)
}

pub fn stop_all_services() -> Result<String> {
    let mut outputs = Vec::new();
    let mut failures = Vec::new();
    let core_types = [CoreType::Mihomo];
    for ct in &core_types {
        if core_service_running(*ct) == Some(false) {
            outputs.push(format!("{ct}: already stopped"));
            continue;
        }
        match stop_core_service(*ct) {
            Ok(out) => outputs.push(out),
            Err(e) => {
                log::warn!("Failed to stop {:?} service: {e}", ct);
                failures.push(format!("{ct}: {e}"));
            }
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "Some services could not be stopped: {}",
        failures.join("; ")
    );
    Ok(outputs.join("\n"))
}

pub fn edit(path: &str) -> Result<()> {
    let tpl = CONFIG.cfg_file.extra.edit_cmd.as_deref().unwrap_or("");
    log::debug!("edit: path={path} template={tpl}");
    #[cfg(all(unix, feature = "tui"))]
    if !tpl.is_empty() && crate::tui::is_active() && !BACKGROUND.with(|background| background.get())
    {
        return edit_terminal(tpl, path);
    }
    shell_spawn(tpl, path)
}

pub fn open_dir(path: &str) -> Result<()> {
    let tpl = CONFIG.cfg_file.extra.open_dir_cmd.as_deref().unwrap_or("");
    log::debug!("open_dir: path={path} template={tpl}");
    shell_spawn(tpl, path)
}

pub fn open_panel() -> Result<()> {
    let url = format!("{}/ui/", CONFIG.controller_for_core().trim_end_matches('/'));
    anyhow::ensure!(
        url.starts_with("http://") || url.starts_with("https://"),
        "Panel endpoint must use HTTP or HTTPS"
    );
    #[cfg(target_os = "linux")]
    let (bin, args) = ("xdg-open", vec![url.as_str()]);
    #[cfg(target_os = "macos")]
    let (bin, args) = ("open", vec![url.as_str()]);
    #[cfg(target_os = "windows")]
    let (bin, args) = (
        "rundll32",
        vec!["url.dll,FileProtocolHandler", url.as_str()],
    );
    Command::new(bin).args(args).spawn()?;
    Ok(())
}

#[cfg(feature = "tui")]
pub fn copy_text(text: &str) -> Result<()> {
    use std::io::Write;
    #[cfg(target_os = "linux")]
    let candidates: &[(&str, &[&str])] =
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])];
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &[&str])] = &[(
        "powershell",
        &["-NoProfile", "-Command", "$input | Set-Clipboard"],
    )];
    let mut failures = Vec::new();
    for (program, args) in candidates {
        let result = (|| -> Result<()> {
            let mut child = Command::new(program)
                .args(*args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            let write = child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("Clipboard input unavailable"))?
                .write_all(text.as_bytes());
            let status = child.wait()?;
            write?;
            anyhow::ensure!(status.success(), "Clipboard command failed");
            Ok(())
        })();
        if result.is_ok() {
            return Ok(());
        }
        failures.push(program.to_string());
    }
    anyhow::bail!("Clipboard unavailable; check {}", failures.join(" or "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_launchd_plist_path_user() {
        let path = launchd_plist_path("com.example.service", true);
        assert!(path.contains("Library/LaunchAgents"));
        assert!(path.contains("com.example.service.plist"));
        assert!(
            !path.starts_with("/Library/"),
            "user path should use HOME, not system /Library: {path}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_launchd_plist_path_system() {
        let path = launchd_plist_path("com.example.service", false);
        assert_eq!(path, "/Library/LaunchDaemons/com.example.service.plist");
    }

    #[test]
    fn test_service_controller_args_launchd() {
        let args = ServiceController::Launchd.args("start", "my_service", false);
        assert!(
            args.is_empty(),
            "Launchd args should be empty (handled inline)"
        );
    }

    #[test]
    fn test_service_controller_args_launchd_user() {
        let args = ServiceController::Launchd.args("stop", "my_service", true);
        assert!(args.is_empty(), "Launchd user args should also be empty");
    }

    #[test]
    fn test_service_controller_bin_name_launchd() {
        assert_eq!(ServiceController::Launchd.bin_name(), "launchctl");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_service_controller_default_is_launchd_on_macos() {
        assert_eq!(ServiceController::default(), ServiceController::Launchd);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_service_controller_default_not_macos() {
        // On non-macOS, the default should NOT be Launchd
        assert_ne!(ServiceController::default(), ServiceController::Launchd);
    }
}
