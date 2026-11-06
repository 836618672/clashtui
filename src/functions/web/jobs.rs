//! Single-operation lifecycle and durable recovery; never replay operations.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Running,
    #[serde(alias = "completed")]
    Succeeded,
    Failed,
    Interrupted,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Job {
    pub id: String,
    action: String,
    pub state: State,
    #[serde(default)]
    started_at: Option<u64>,
    #[serde(default)]
    finished_at: Option<u64>,
    pub result: Option<Value>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl Job {
    pub fn running(action: &str) -> Self {
        Self {
            id: fastrand::u64(..).to_string(),
            action: action.to_owned(),
            state: State::Running,
            started_at: Some(now()),
            finished_at: None,
            result: None,
        }
    }
    pub fn pending(&self) -> bool {
        self.state == State::Running
    }
    pub fn snapshot(&self) -> Value {
        let mut value =
            serde_json::to_value(self).expect("Job serialization contains only JSON values");
        value["pending"] = json!(self.pending());
        value
    }
    pub fn finish(&mut self, result: Value) {
        self.state = if result["ok"] == true {
            State::Succeeded
        } else {
            State::Failed
        };
        self.finished_at = Some(now());
        self.result = Some(result);
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Task recovery record must be a regular file"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            }
        }
        crate::functions::file::activation::atomic_write(path, &serde_json::to_vec(self)?)
    }
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(metadata) => ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Task recovery record must be a regular file"
            ),
        }
        let bytes = std::fs::read(path)?;
        let parsed = serde_json::from_slice::<Self>(&bytes).ok().filter(|job| {
            job.id.parse::<u64>().is_ok()
                && match job.state {
                    State::Running => job.result.is_none(),
                    State::Succeeded => job.result.as_ref().is_some_and(|r| r["ok"] == true),
                    State::Failed | State::Interrupted => job
                        .result
                        .as_ref()
                        .is_some_and(|r| r["ok"] == false && r["error"].is_string()),
                }
        });
        let Some(mut job) = parsed else {
            // Preserve damaged bytes for diagnosis, report uncertainty to clients.
            let mut job = Self::running("recovery");
            job.state = State::Interrupted;
            job.finished_at = Some(now());
            job.result = Some(
                json!({"ok":false,"code":"recovery_record_invalid","error":"Task recovery record is invalid; refresh and check the current state before retrying"}),
            );
            return Ok(Some(job));
        };
        if job.pending() {
            job.state = State::Interrupted;
            job.finished_at = Some(now());
            job.result = Some(
                json!({"ok":false,"code":"task_interrupted","error":"Management server restarted during a task; refresh and check the current state before retrying"}),
            );
            job.save(path)?;
        }
        Ok(Some(job))
    }
}

pub(super) fn execute(work: impl FnOnce() -> Result<Value>, token: &str) -> Value {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => {
            json!({"ok":false,"code":"operation_failed","error":format!("{error:#}").replace(token,"[redacted]")})
        }
        Err(_) => {
            json!({"ok":false,"code":"task_panicked","error":"Management task failed unexpectedly; refresh before retrying"})
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_and_errors_become_failed_results_without_leaking_credentials() {
        let result = execute(|| panic!("test panic"), "private-token");
        assert_eq!(result["ok"], false);
        assert_eq!(result["code"], "task_panicked");
        let result = execute(|| anyhow::bail!("failure private-token"), "private-token");
        assert!(!result.to_string().contains("private-token"));
        let mut job = Job::running("test");
        job.finish(result);
        assert!(!job.pending());
        assert_eq!(job.state, State::Failed);
        assert!(job.finished_at >= job.started_at);
        assert_eq!(execute(|| Ok(json!(1)), "token")["value"], 1);
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn recovery_preserves_outcomes_and_persists_interruption_without_replay() {
        let dir = Fixture(std::env::temp_dir().join(format!("clashtui-job-{}", fastrand::u64(..))));
        std::fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join("task.json");
        assert!(Job::load(&path).unwrap().is_none());
        let running = Job::running("activate");
        running.save(&path).unwrap();
        let recovered = Job::load(&path).unwrap().unwrap();
        assert_eq!(recovered.id, running.id);
        assert_eq!(recovered.state, State::Interrupted);
        assert_eq!(
            recovered.result.as_ref().unwrap()["code"],
            "task_interrupted"
        );
        assert_eq!(
            Job::load(&path).unwrap().unwrap().snapshot(),
            recovered.snapshot()
        );
        for result in [
            json!({"ok":true,"value":{"revision":"new"}}),
            json!({"ok":false,"error":"failed"}),
        ] {
            let mut job = Job::running("save");
            job.finish(result.clone());
            job.save(&path).unwrap();
            assert_eq!(
                Job::load(&path).unwrap().unwrap().snapshot(),
                job.snapshot()
            );
        }
        std::fs::write(
            &path,
            r#"{"id":"12","action":"save","state":"completed","result":{"ok":true,"value":{}}}"#,
        )
        .unwrap();
        assert_eq!(Job::load(&path).unwrap().unwrap().state, State::Succeeded);
    }
    #[test]
    fn malformed_or_inconsistent_records_report_uncertainty_and_preserve_bytes() {
        let dir = Fixture(std::env::temp_dir().join(format!("clashtui-job-{}", fastrand::u64(..))));
        std::fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join("task.json");
        for content in [
            "{broken",
            "{}",
            r#"{"id":"1","action":"save","state":"succeeded","result":{"ok":false,"error":"bad"}}"#,
        ] {
            std::fs::write(&path, content).unwrap();
            let recovered = Job::load(&path).unwrap().unwrap();
            assert_eq!(recovered.state, State::Interrupted);
            assert_eq!(recovered.result.unwrap()["code"], "recovery_record_invalid");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        }
        #[cfg(unix)]
        {
            std::fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(dir.0.join("other"), &path).unwrap();
            assert!(Job::load(&path).is_err());
            assert!(Job::running("save").save(&path).is_err());
        }
    }
}
