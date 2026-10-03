use super::platform::stringify_output;
use anyhow::Result;
use std::process::{Command, Stdio};

pub fn exec(pgm: &str, args: Vec<&str>) -> Result<String> {
    log::debug!("IPC: {} {:?}", pgm, args);
    let output = Command::new(pgm).args(args).output()?;
    anyhow::ensure!(
        output.status.success(),
        "Command {pgm} failed: {}",
        stringify_output(output.clone())
    );
    Ok(stringify_output(output))
}

pub fn exec_sudo(pgm: &str, args: Vec<&str>) -> Result<String> {
    if super::BACKGROUND.with(|background| background.get()) {
        let output = Command::new("sudo")
            .arg("-n")
            .arg(pgm)
            .args(args)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "Service command failed; configure non-interactive service permissions: {}",
            stringify_output(output.clone())
        );
        return Ok(stringify_output(output));
    }
    log::debug!("IPC: sudo -S {:?}", args);
    #[cfg(feature = "tui")]
    crate::tui::hold(true)?;
    let mut child = Command::new("sudo")
        .arg(pgm)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut childstderr = child.stderr.take().unwrap();
    let mut stderr = std::io::stderr();
    let mut output_copy = Vec::new();
    let mut buffer = [0; 1024];

    // Read in a loop until the pipe closes
    loop {
        use std::io::{Read, Write};
        let n = childstderr.read(&mut buffer)?;
        if n == 0 {
            break; // EOF
        }
        // Save to memory (keep a copy)
        output_copy.extend_from_slice(&buffer[..n]);
        // Write to terminal
        stderr.write_all(&buffer[..n])?;
        stderr.flush()?;
    }
    eprintln!();
    let mut output = child.wait_with_output()?;
    output.stderr = output_copy;

    #[cfg(feature = "tui")]
    crate::tui::hold(false)?;
    anyhow::ensure!(
        output.status.success(),
        "Service command failed: {}",
        stringify_output(output.clone())
    );
    Ok(stringify_output(output))
}

// #[cfg(unix)]
// fn check_sudo_password_required() -> Result<bool> {
//     Command::new("sudo")
//         .args(["-n", "true"])
//         .stdout(Stdio::null())
//         .stderr(Stdio::null())
//         .status()
//         .map(|staus| staus.success())
//         .map_err(|e| e.into())
// }

pub fn spawn(pgm: &str, args: Vec<&str>) -> Result<()> {
    log::debug!("SPW: {} {:?}", pgm, args);
    Command::new(pgm)
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .args(args)
        .spawn()?;
    Ok(())
}

fn sanitize_windows_path(path: &str) -> String {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    path.replace('\\', "/")
}

pub fn shell_spawn(cmd_template: &str, path: &str) -> Result<()> {
    if cmd_template.is_empty() {
        if cfg!(windows) {
            let path = sanitize_windows_path(path);
            spawn("explorer", vec![&path])
        } else if cfg!(target_os = "macos") {
            spawn("open", vec![path])
        } else {
            spawn("xdg-open", vec![path])
        }
    } else if cfg!(windows) {
        // cmd.exe reparses arguments. A custom program plus one path argument
        // avoids exposing document names to that parser.
        let program = cmd_template
            .strip_suffix(" %s")
            .unwrap_or(cmd_template)
            .trim_matches('"');
        anyhow::ensure!(
            !program.contains('%') && !program.contains(['&', '|', '<', '>']),
            "Windows editor command must be a program path, optionally followed by %s"
        );
        spawn(program, vec![&sanitize_windows_path(path)])
    } else {
        let cmd = positional_template(cmd_template)?;
        spawn("sh", vec!["-c", &cmd, "clashtui-editor", path])
    }
}

#[cfg(all(unix, feature = "tui"))]
pub fn edit_terminal(template: &str, path: &str) -> Result<()> {
    use std::sync::atomic::Ordering;
    let command = positional_template(template)?;
    anyhow::ensure!(
        crate::tui::EXT_PROC
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok(),
        "Another external process is running"
    );
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = crate::tui::hold(false);
            crate::tui::EXT_PROC.store(false, Ordering::SeqCst);
        }
    }
    let _restore = Restore;
    crate::tui::hold(true)?;
    let status = Command::new("sh")
        .args(["-c", &command, "clashtui-editor", path])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    anyhow::ensure!(status.success(), "Editor exited unsuccessfully: {status}");
    Ok(())
}

fn positional_template(template: &str) -> Result<String> {
    // Custom shell syntax is trusted configuration; document paths are data.
    anyhow::ensure!(
        !template.contains('`') && !template.contains("$("),
        "Editor templates with command substitution are unsupported; use a wrapper script"
    );
    let mut result = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if !escaped && ch == '%' && chars.peek() == Some(&'s') {
            chars.next();
            result.push_str(match quote {
                Some('\'') => "'\"$1\"'",
                Some('"') => "$1",
                _ => "\"$1\"",
            });
            continue;
        }
        result.push(ch);
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
        } else if Some(ch) == quote {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        }
    }
    anyhow::ensure!(
        quote.is_none() && !escaped,
        "Unclosed editor template quote or escape"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn editor_paths_are_one_literal_argument_in_each_quote_context() {
        let path = "space ' quote \" ; $(touch /tmp/clashtui-should-not-exist) & *";
        for template in ["printf '%s' %s", "printf '%s' '%s'", "printf '%s' \"%s\""] {
            // The printf format itself has the placeholder syntax, so use a
            // shell builtin with a literal output format assembled separately.
            let argument = template.strip_prefix("printf '%s' ").unwrap();
            let command = format!("printf '%s' {}", positional_template(argument).unwrap());
            let output = Command::new("sh")
                .args(["-c", &command, "test-editor", path])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(String::from_utf8(output.stdout).unwrap(), path);
        }
        assert!(positional_template("editor $(echo %s)").is_err());
        assert!(positional_template("editor '%s").is_err());
    }

    #[test]
    fn sanitize_unc_prefix_stripped() {
        assert_eq!(sanitize_windows_path(r"\\?\C:\Users\foo"), "C:/Users/foo");
    }

    #[test]
    fn sanitize_non_unc_untouched() {
        assert_eq!(sanitize_windows_path(r"C:\Users\foo"), "C:/Users/foo");
    }

    #[test]
    fn sanitize_forward_slashes_unchanged() {
        assert_eq!(sanitize_windows_path("C:/Users/foo"), "C:/Users/foo");
    }

    #[test]
    fn sanitize_mixed_slashes_converted() {
        assert_eq!(sanitize_windows_path(r"C:\foo/bar\baz"), "C:/foo/bar/baz");
    }

    #[test]
    fn sanitize_unc_with_mixed_slashes() {
        assert_eq!(
            sanitize_windows_path(r"\\?\C:\foo/bar\baz"),
            "C:/foo/bar/baz"
        );
    }

    #[test]
    fn sanitize_empty_string() {
        assert_eq!(sanitize_windows_path(""), "");
    }

    #[test]
    fn sanitize_path_without_backslashes() {
        assert_eq!(sanitize_windows_path("C:/foo/bar/baz"), "C:/foo/bar/baz");
    }
}
