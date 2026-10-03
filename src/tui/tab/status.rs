use ratatui::layout::{Constraint, Layout};
use ratatui::{text::Text, widgets::Paragraph};

use super::dev::*;

newtype_tab!(StatusTab(Tab<Status>));

mod_agent!(
    Key,
    [
        ([KeyCode::Char('w')], Key::OpenPanel, "Open MetaCubeXD"),
        (
            [KeyCode::Char('i')],
            Key::PreparePanel,
            "Prepare pinned MetaCubeXD panel"
        ),
    ]
);
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(crate) enum Key {
    OpenPanel,
    PreparePanel,
}

impl TryFrom<&crate::tui::Key> for Key {
    type Error = ();

    fn try_from(key: &crate::tui::Key) -> Result<Self, Self::Error> {
        agent().get(key).copied().ok_or(())
    }
}

use crate::config::CONFIG;
use crate::config::CoreType;
use crate::functions::restful::{self, config_struct::*};

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                crate::tui::widget::popmsg::Confirm::err(e);
                return do_nothing();
            }
        }
    };
    ($e:expr, or_set) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                return wrapper(move |content: &mut Self| {
                    content.error = Some(e.to_string());
                });
            }
        }
    };
}

#[derive(Default)]
struct Status {
    config: Option<ClashConfig>,
    version: Option<String>,
    detected_core_type: Option<CoreType>,
    error: Option<String>,
    paused: bool,
    metrics: restful::metrics::Metrics,
    metrics_error: Option<String>,
    memory: Option<u64>,
    memory_sampled: Option<std::time::Instant>,
    memory_error: Option<String>,
}

impl BasicTabContent for Status {
    type Key = Key;

    type State = ();

    const TITLE: &str = "Status";
    fn all_shortcuts() -> &'static [(KeyCombo, Key, &'static str)] {
        agent::all_shortcuts()
    }

    fn after_sync(&self, task_set: &mut FutureSet<Self>) {
        if self.paused || !task_set.is_empty() {
            return;
        }
        let sample_memory = self
            .memory_sampled
            .is_none_or(|time| time.elapsed() >= std::time::Duration::from_secs(30));
        async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let version = tri!(
                crate::functions::restful::session::spawn_blocking(restful::control::version)
                    .await
                    .unwrap(),
                or_set
            );
            let config = tri!(
                crate::functions::restful::session::spawn_blocking(restful::config::fetch)
                    .await
                    .unwrap(),
                or_set
            );
            let detected = tri!(
                crate::functions::restful::session::spawn_blocking(
                    restful::core_detect::detect_core_type
                )
                .await
                .unwrap(),
                or_set
            );

            let metrics = crate::functions::restful::session::spawn_blocking(
                restful::connection::get_connections,
            )
            .await;
            let sample_time = std::time::Instant::now();
            let memory = if sample_memory {
                Some(
                    crate::functions::restful::session::spawn_blocking(restful::stream::memory)
                        .await,
                )
            } else {
                None
            };

            wrapper(move |content: &mut Self| {
                let configured = CONFIG.core_type();
                content.detected_core_type = Some(detected);
                match metrics {
                    Ok(Ok(info)) => {
                        content.metrics.sample(&info, sample_time);
                        content.metrics_error = None;
                    }
                    Ok(Err(error)) => content.metrics_error = Some(error.to_string()),
                    Err(error) => content.metrics_error = Some(error.to_string()),
                }
                if let Some(memory) = memory {
                    content.memory_sampled = Some(std::time::Instant::now());
                    match memory {
                        Ok(Ok(bytes)) => {
                            content.memory = Some(bytes);
                            content.memory_error = None;
                        }
                        Ok(Err(error)) => content.memory_error = Some(error.to_string()),
                        Err(error) => content.memory_error = Some(error.to_string()),
                    }
                }
                if detected == configured {
                    content.version = Some(version);
                    content.config = Some(config);
                    content.error = None;
                    crate::config::set_core_mismatch(false);
                } else {
                    content.error = Some(format!(
                        "API returned {detected} data, but {configured} is configured"
                    ));
                    crate::config::set_core_mismatch(true);
                }
            })
        }
        .spawn_at(task_set);
    }

    fn on_enter(&mut self, task_set: &mut FutureSet<Self>, _state: &mut Self::State) {
        self.paused = false;

        let was_unknown = self.detected_core_type.is_none();
        if was_unknown {
            match crate::functions::command::is_core_service_running() {
                Some(false) => {
                    self.error = Some("Core service is not running".to_owned());
                    self.after_sync(task_set);
                    return;
                }
                _ => {
                    self.error = Some("Detecting...".to_owned());
                }
            }
        }

        async {
            let detected = tri!(
                crate::functions::restful::session::spawn_blocking(
                    restful::core_detect::detect_core_type
                )
                .await
                .unwrap(),
                or_set
            );
            let version = tri!(
                crate::functions::restful::session::spawn_blocking(restful::control::version)
                    .await
                    .unwrap(),
                or_set
            );
            let config = tri!(
                crate::functions::restful::session::spawn_blocking(restful::config::fetch)
                    .await
                    .unwrap(),
                or_set
            );

            wrapper(move |content: &mut Self| {
                let configured = CONFIG.core_type();
                content.detected_core_type = Some(detected);
                let mismatch = detected != configured;
                crate::config::set_core_mismatch(mismatch);
                if mismatch {
                    let msg =
                        format!("API returned {detected} data, but {configured} is configured");
                    content.error = Some(msg);
                } else {
                    content.version = Some(version);
                    content.config = Some(config);
                    content.error = None;
                }
            })
        }
        .spawn_at(task_set);
    }

    fn on_leave(&mut self, _task_set: &mut FutureSet<Self>, _state: &mut Self::State) {
        self.paused = true;
    }
}

impl TabContent for Status {
    fn init(&mut self, _task_set: &mut FutureSet<Self>, _state: &mut Self::State) {
        self.paused = true;
        self.error = Some("Waiting".to_owned());
    }

    fn handle_key_event(
        &mut self,
        key: Self::Key,
        task_set: &mut FutureSet<Self>,
        _state: &mut Self::State,
    ) {
        match key {
            Key::OpenPanel => {
                if let Err(error) = crate::functions::command::open_panel() {
                    crate::tui::widget::popmsg::Confirm::err(error);
                }
            }
            Key::PreparePanel => {
                async {
                    if crate::tui::widget::popmsg::Confirm::title("Prepare MetaCubeXD v1.273.1?".to_owned())
                        .with_prompt("Download the locked release, verify SHA-256, then replace the panel directory. Enter confirms; Esc cancels.".to_owned())
                        .build_and_send().await.is_err() { return do_nothing(); }
                    if let Err(error) = crate::functions::restful::session::spawn_blocking(crate::functions::file::panel::prepare).await.unwrap() { crate::tui::widget::popmsg::Confirm::err(error); }
                    do_nothing()
                }.spawn_at(task_set);
            }
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, _state: &mut Self::State) {
        let block = Block::bordered()
            .border_style(Theme::get().section("status").border)
            .title(Self::TITLE);
        let mut lines: Vec<String> = vec![];
        let configured = CONFIG.core_type();
        lines.push(format!(
            "MetaCubeXD target {} · {}/ui/ · w opens · i prepares",
            crate::functions::file::panel::VERSION,
            CONFIG.controller_for_core().trim_end_matches('/')
        ));
        let panel = crate::functions::file::panel::deployment_state();
        lines.push(if panel["verified_install_record"] == true {
            "Panel: pinned installation recorded (served assets require browser verification)"
                .to_owned()
        } else {
            "Panel: installation version unknown; prepare the pinned release to record it"
                .to_owned()
        });
        if let Some(detected) = self.detected_core_type {
            if detected == configured {
                lines.push(format!("core: {detected}"));
            } else {
                lines.push(format!(
                    "core: {detected} (configured: {configured}, MISMATCH)"
                ));
            }
        }
        let matched = self.detected_core_type.is_none_or(|d| d == configured);
        if matched {
            if let Some(memory) = self.memory {
                lines.push(format!(
                    "memory: {memory} B{}",
                    if self.memory_error.is_some() {
                        " (stale)"
                    } else {
                        ""
                    }
                ));
            } else if let Some(error) = &self.memory_error {
                lines.push(format!("memory unavailable: {error}"));
            }
            lines.push(format!(
                "connections: {} | download: {} B/s | upload: {} B/s",
                self.metrics.connections, self.metrics.download_speed, self.metrics.upload_speed
            ));
            lines.push(format!(
                "observed session: ↓ {} B / ↑ {} B",
                self.metrics.download, self.metrics.upload
            ));
            if let Some(error) = &self.metrics_error {
                lines.push(format!("metrics unavailable: {error}"));
            }
            if let Some(ref ver) = self.version {
                lines.push(format!("version: {ver}"));
            }
            if let Some(cfg) = self.config.as_ref() {
                lines.extend(cfg.build());
            }
        }
        if lines.is_empty() {
            lines.push(
                self.error
                    .as_deref()
                    .map(|s| s.to_owned())
                    .expect("if there is not content, there should be an error"),
            );
        }
        let widget = Paragraph::new(Text::from_iter(lines)).block(block);
        let areas = Layout::vertical([Constraint::Min(4), Constraint::Length(4)]).split(area);
        f.render_widget(widget, areas[0]);
        let history: Vec<u64> = self.metrics.history.iter().copied().collect();
        f.render_widget(
            ratatui::widgets::Sparkline::default()
                .block(Block::bordered().title("Combined rate · last 120 samples · B/s"))
                .data(&history),
            areas[1],
        );
    }
}
