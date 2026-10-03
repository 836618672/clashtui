//! Process-level workflows use temporary data, fake service status and HTTP
//! subscriptions. No real core, installation or service operation is performed.
#![cfg(target_os = "linux")]
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("clashtui-process-{}", fastrand::u64(..)));
        for directory in ["bin", "mihomo/profiles", "mihomo/templates"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        let fake = root.join("bin/systemctl");
        std::fs::write(&fake, "#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = format!(
            "mihomo:\n  core:\n    config_dir: {0}/mihomo\n    config_path: {0}/mihomo/config.yaml\n    bin_path: {0}/bin/fake-core\n  core_service:\n    service_name: mock-core\n    is_user: true\n    service_controller: systemd\ntimeout: 2\n",
            root.display()
        );
        std::fs::write(root.join("config.yaml"), config).unwrap();
        Self(root)
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_clashtui"));
        command
            .arg(format!("--config-dir={}", self.0.display()))
            .args(args)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.0.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("CLASHTUI_MOCK_CALLS", self.0.join("service-calls"));
        command
    }
    fn run(&self, args: &[&str]) -> Value {
        let output = self.command(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn file(&self, name: &str, content: &str) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, content).unwrap();
        path.to_str().unwrap().to_owned()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn legacy_database_migrates_without_touching_removed_core_files() {
    let fixture = Fixture::new();
    fixture.file("mihomo/profiles/main.yaml", "proxies: []\n");
    let original = "core_type: singbox\nmihomo:\n  cur_profile: main\n  profiles:\n    main: File\nsingbox:\n  profiles:\n    old: Singbox\n";
    fixture.file("clashtui.db", original);
    let mut config = std::fs::read_to_string(fixture.0.join("config.yaml")).unwrap();
    config.push_str("singbox:\n  core:\n    bin_path: /removed/backend\n");
    fixture.file("config.yaml", &config);

    let state = fixture.run(&["manage", "state"]);
    assert_eq!(state["core"], "mihomo");
    assert!(!fixture.0.join("sing-box").exists());
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("clashtui.db.before-mihomo-only")).unwrap(),
        original
    );
    fixture.run(&[
        "manage",
        "rename",
        "--name",
        "main",
        "--new-name",
        "retained",
    ]);
    let database = std::fs::read_to_string(fixture.0.join("clashtui.db")).unwrap();
    assert!(!database.contains("singbox"));
    assert!(fixture.0.join("mihomo/profiles/retained.yaml").exists());
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("clashtui.db.before-mihomo-only")).unwrap(),
        original
    );
    assert!(!fixture.0.join("sing-box").exists());
}

#[test]
fn shared_management_rejects_stale_documents_and_preserves_profile_files() {
    let fixture = Fixture::new();
    let file = fixture.file("input.yaml", "proxies: []\n");
    fixture.run(&["manage", "import", "--name", "sample", "--input", &file]);
    let document = fixture.run(&["manage", "read", "--kind", "profile", "--name", "sample"]);
    let revision = document["revision"].as_str().unwrap();
    let changed = fixture.file("changed.yaml", "proxies: []\nmode: direct\n");
    fixture.run(&[
        "manage",
        "save",
        "--kind",
        "profile",
        "--name",
        "sample",
        "--input",
        &changed,
        "--revision",
        revision,
    ]);
    let output = fixture
        .command(&[
            "manage",
            "save",
            "--kind",
            "profile",
            "--name",
            "sample",
            "--input",
            &file,
            "--revision",
            revision,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Revision conflict"));
    fixture.run(&[
        "manage",
        "rename",
        "--name",
        "sample",
        "--new-name",
        "renamed",
    ]);
    assert!(fixture.0.join("mihomo/profiles/renamed.yaml").exists());
    assert!(!fixture.0.join("mihomo/profiles/sample.yaml").exists());
    fixture.run(&["manage", "delete", "--name", "renamed", "--yes"]);
    assert!(!fixture.0.join("mihomo/profiles/renamed.yaml").exists());
}

#[test]
fn legacy_cli_download_holds_the_same_process_lock_as_management_rename() {
    let fixture = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/subscription", listener.local_addr().unwrap());
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            if index == 1 {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            let body = if index == 0 {
                "proxies: []\n"
            } else {
                "proxies: []\nmode: direct\n"
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    fixture.run(&["manage", "create", "--name", "sample", "--url", &url]);
    let updating = fixture
        .command(&["profile", "update", "--name", "sample"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut renaming = fixture
        .command(&[
            "manage",
            "rename",
            "--name",
            "sample",
            "--new-name",
            "renamed",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        renaming.try_wait().unwrap().is_none(),
        "Rename must wait for the complete download transaction"
    );
    release_tx.send(()).unwrap();
    let assert_success = |output: Output| {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        )
    };
    assert_success(updating.wait_with_output().unwrap());
    assert_success(renaming.wait_with_output().unwrap());
    server.join().unwrap();
    assert!(!fixture.0.join("mihomo/profiles/sample.yaml").exists());
    assert!(
        std::fs::read_to_string(fixture.0.join("mihomo/profiles/renamed.yaml"))
            .unwrap()
            .contains("direct")
    );
}

struct WebProcess(std::process::Child);
impl Drop for WebProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start_web(fixture: &Fixture, address: &str, token_file: &str) -> WebProcess {
    let child = fixture
        .command(&["web", "--listen", address, "--token-file", token_file])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let process = WebProcess(child);
    for _ in 0..100 {
        if minreq::get(format!("http://{address}/"))
            .with_timeout(1)
            .send()
            .is_ok()
        {
            return process;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("Mock management server did not become ready");
}

#[test]
fn web_busy_reads_do_not_block_job_polling_and_completed_results_survive_restart() {
    let fixture = Fixture::new();
    fixture.run(&["manage", "state"]);
    let token = "mock-management-token-1234567890";
    let token_file = fixture.file("management-token", token);
    let legacy_record = fixture.file(".web-task.json", "{}");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(legacy_record, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap().to_string();
    drop(reservation);
    let process = start_web(&fixture, &address, &token_file);
    let get = |path: &str| {
        minreq::get(format!("http://{address}{path}"))
            .with_header("Authorization", format!("Bearer {token}"))
            .with_timeout(1)
            .send()
            .unwrap()
    };
    assert_eq!(
        minreq::get(format!("http://{address}/api/state"))
            .with_timeout(1)
            .send()
            .unwrap()
            .status_code,
        401
    );
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.0.join(".management.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert_eq!(get("/api/state").status_code, 503);
    assert_eq!(get("/api/job").status_code, 200);
    drop(lock);
    let state = get("/api/state").json::<Value>().unwrap();
    let response = minreq::post(format!("http://{address}/api/action"))
        .with_header("Authorization", format!("Bearer {token}"))
        .with_body(serde_json::json!({"action":"import","name":"from-web","content":"proxies: []\n","revision":state["revision"],"core":state["core"]}).to_string())
        .with_timeout(1).send().unwrap();
    assert_eq!(response.status_code, 200);
    let id = response.json::<Value>().unwrap()["job"].clone();
    let mut completed = None;
    for _ in 0..100 {
        let job = get("/api/job").json::<Value>().unwrap();
        if job["pending"] == false {
            completed = Some(job);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let completed = completed.expect("Mock import task did not finish");
    assert_eq!(completed["id"], id);
    assert_eq!(completed["result"]["ok"], true, "{completed}");
    drop(process);
    let _restarted = start_web(&fixture, &address, &token_file);
    assert_eq!(get("/api/job").json::<Value>().unwrap(), completed);
    assert!(fixture.0.join("mihomo/profiles/from-web.yaml").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(fixture.0.join(".web-task.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
}

#[test]
fn service_commands_only_target_mihomo_and_removed_selectors_are_rejected() {
    let fixture = Fixture::new();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    fixture.file(
        "mihomo/core_override_config.yaml",
        &format!("external-controller: http://{address}\nmixed-port: 27890\n"),
    );
    fixture.file(
        "bin/systemctl",
        r#"#!/bin/sh
case "$1" in
 is-active) echo inactive;;
 *) printf '%s\n' "$*" >> "$CLASHTUI_MOCK_CALLS";;
esac
exit 0
"#,
    );
    for action in ["start", "stop", "restart"] {
        let output = fixture.command(&["service", action]).output().unwrap();
        if action == "stop" {
            assert!(output.status.success());
        } else {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("not ready"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    assert_eq!(fixture.run(&["service", "status"])["core"], "mihomo");
    let calls = std::fs::read_to_string(fixture.0.join("service-calls")).unwrap();
    assert_eq!(calls.lines().count(), 3);
    assert!(calls.lines().all(|line| line.ends_with("mock-core")));
    for args in [
        vec!["service", "switch", "sing-box"],
        vec!["--core", "sing-box", "manage", "state"],
        vec!["manage", "select_core"],
    ] {
        assert!(!fixture.command(&args).output().unwrap().status.success());
    }
}

#[test]
fn template_generation_validates_before_replacement_and_preserves_saved_options() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let validator = fixture.file("bin/fake-core", "#!/bin/sh\nexit 0\n");
    std::fs::set_permissions(&validator, std::fs::Permissions::from_mode(0o700)).unwrap();
    let template_input = fixture.file(
        "template-input.yaml",
        "proxies: []\nproxy-providers: {}\nproxy-groups: []\nrules: ['MATCH,DIRECT']\n",
    );
    let empty = fixture.run(&[
        "manage",
        "read",
        "--kind",
        "template",
        "--name",
        "sample.yaml",
    ]);
    fixture.run(&[
        "manage",
        "save",
        "--kind",
        "template",
        "--name",
        "sample.yaml",
        "--input",
        &template_input,
        "--revision",
        empty["revision"].as_str().unwrap(),
    ]);
    fixture.run(&[
        "manage",
        "generate",
        "--template",
        "sample.yaml",
        "--name",
        "generated",
    ]);
    fixture.run(&["manage", "with_proxy", "--name", "generated"]);
    fixture.run(&[
        "manage",
        "generate",
        "--template",
        "sample.yaml",
        "--name",
        "generated",
        "--yes",
    ]);
    let state = fixture.run(&["manage", "state"]);
    let generated = state["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["name"] == "generated")
        .unwrap();
    assert_eq!(generated["update_with_proxy"], true);
    let path = fixture.0.join("mihomo/profiles/generated.yaml");
    let before = std::fs::read(&path).unwrap();
    fixture.file("bin/fake-core", "#!/bin/sh\nexit 1\n");
    let result = fixture
        .command(&[
            "manage",
            "generate",
            "--template",
            "sample.yaml",
            "--name",
            "generated",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("core validation"));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn readonly_commands_preserve_database_bytes_and_explicit_empty_groups() {
    let fixture = Fixture::new();
    let input = fixture.file("input.yaml", "proxies: []\n");
    for name in ["a", "b", "c", "d"] {
        fixture.run(&["manage", "import", "--name", name, "--input", &input]);
    }
    let before = std::fs::read(fixture.0.join("clashtui.db")).unwrap();
    let state = fixture.run(&["manage", "state"]);
    for _ in 0..10 {
        fixture.run(&["manage", "read", "--name", "a"]);
        assert_eq!(
            fixture.run(&["manage", "state"])["revision"],
            state["revision"]
        );
        assert_eq!(
            std::fs::read(fixture.0.join("clashtui.db")).unwrap(),
            before
        );
    }
    fixture.file(
        "mihomo/template_proxy_providers.yaml",
        "legacy: {old: 'http://localhost/old'}\n",
    );
    fixture.file(
        "mihomo/templates/empty.yaml",
        "clashtui: {proxy_provider_groups: {}}\n",
    );
    assert_eq!(
        fixture.run(&["manage", "template_providers", "--name", "empty.yaml"])["groups"],
        serde_json::json!({})
    );
    fixture.file("mihomo/templates/absent.yaml", "proxies: []\n");
    assert_eq!(
        fixture.run(&["manage", "template_providers", "--name", "absent.yaml"])["groups"]["legacy"]
            ["old"],
        "http://localhost/old"
    );
}
