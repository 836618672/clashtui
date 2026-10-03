use super::dev::*;
use crate::config::CONFIG;
use ratatui::{
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Clear, ListItem, Paragraph},
};
use strum::VariantArray;

newtype_tab!(SettingsTab(Tab<SettingsContent>));

mod_agent!(
    SettingsKey,
    [
        ([KeyCode::Enter], SettingsKey::Execute, "Apply"),
        ([KeyCode::Esc], SettingsKey::Esc, "Back"),
        ([KeyCode::Up], SettingsKey::MoveUp, "Move up"),
        ([KeyCode::Down], SettingsKey::MoveDown, "Move down"),
        ([KeyCode::Char('k')], SettingsKey::MoveUp, "Move up"),
        ([KeyCode::Char('j')], SettingsKey::MoveDown, "Move down"),
        (
            [KeyCode::Char('c')],
            SettingsKey::CloseOnMode,
            "Toggle closing connections after mode change"
        ),
        (
            [KeyCode::Char('e')],
            SettingsKey::EditRuntime,
            "Edit temporary runtime fields"
        ),
        (
            [KeyCode::Char('p')],
            SettingsKey::PersistRuntime,
            "Save current runtime fields to override"
        ),
    ]
);

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(crate) enum SettingsKey {
    Execute,
    MoveUp,
    MoveDown,
    Esc,
    EditRuntime,
    PersistRuntime,
    CloseOnMode,
}

impl TryFrom<&crate::tui::Key> for SettingsKey {
    type Error = ();

    fn try_from(ev: &crate::tui::Key) -> Result<Self, Self::Error> {
        let agent = agent();
        if !agent.is_empty() {
            return agent.get(ev).copied().ok_or(());
        }
        Ok(match ev.code {
            KeyCode::Enter => Self::Execute,
            KeyCode::Esc => Self::Esc,
            KeyCode::Up | KeyCode::Char('k') => Self::MoveUp,
            KeyCode::Down | KeyCode::Char('j') => Self::MoveDown,
            _ => return Err(()),
        })
    }
}

use crate::config::CoreType;
use crate::functions::restful::config_struct::TunStack;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsOp {
    SwitchMode,
    AllowLan,
    TunEnable,
    TunStackOp,
    FlushFakeIP,
    FlushDNSCache,
    UpdateGeo,
}

impl SettingsOp {
    fn all(core_type: CoreType) -> Vec<Self> {
        let mut ops = vec![Self::SwitchMode, Self::AllowLan];
        if core_type == CoreType::Mihomo {
            ops.push(Self::TunEnable);
            ops.push(Self::TunStackOp);
            ops.push(Self::FlushFakeIP);
            ops.push(Self::FlushDNSCache);
            ops.push(Self::UpdateGeo);
        }
        ops
    }
}

#[derive(Default)]
struct SettingsContent {
    core_type: CoreType,
    ops: Vec<SettingsOp>,
    current_mode: String,
    allow_lan: bool,
    tun_enable: bool,
    tun_stack: String,
    mode_selector_state: ListState,
    mode_selector_visible: bool,
    tun_selector_state: ListState,
    tun_selector_visible: bool,
    modes: Vec<String>,
    tun_stacks: Vec<TunStack>,
    paused: bool,
    refresh_in_flight: std::cell::Cell<bool>,
    error: Option<String>,
    close_on_mode: bool,
}

impl SettingsContent {
    fn apply_config(&mut self, config: crate::functions::restful::config_struct::ClashConfig) {
        self.current_mode = config.mode.to_string();
        self.modes = config
            .mode_list
            .clone()
            .filter(|modes| !modes.is_empty())
            .unwrap_or_else(|| vec!["Rule".to_owned(), "Global".to_owned(), "Direct".to_owned()]);
        if !self.modes.contains(&self.current_mode) {
            self.modes.push(self.current_mode.clone());
        }
        self.allow_lan = config.allow_lan.unwrap_or(false);
        self.tun_enable = config.tun.as_ref().map(|tun| tun.enable).unwrap_or(false);
        self.tun_stack = config
            .tun
            .as_ref()
            .map(|tun| tun.stack.to_string())
            .unwrap_or_default();
        self.ops = SettingsOp::all(self.core_type);
        // Missing optional fields are not evidence of a disabled setting.
        self.ops.retain(|op| match op {
            SettingsOp::AllowLan => {
                self.core_type == CoreType::Mihomo && config.allow_lan.is_some()
            }
            SettingsOp::TunEnable | SettingsOp::TunStackOp => config.tun.is_some(),
            _ => true,
        });
        self.error = None;
    }

    fn refresh(&self, task_set: &mut FutureSet<Self>, delay: std::time::Duration) {
        if self.paused || self.refresh_in_flight.replace(true) {
            return;
        }
        async move {
            tokio::time::sleep(delay).await;
            let result = crate::functions::restful::session::spawn_blocking(
                crate::functions::restful::config::fetch,
            )
            .await
            .unwrap();
            wrapper(move |content: &mut Self| {
                content.refresh_in_flight.set(false);
                match result {
                    Ok(config) => content.apply_config(config),
                    Err(error) => content.error = Some(format!("State may be stale: {error}")),
                }
            })
        }
        .spawn_at(task_set);
    }
}

impl BasicTabContent for SettingsContent {
    type Key = SettingsKey;

    type State = ListState;

    const TITLE: &str = "Settings";

    fn all_shortcuts() -> &'static [(KeyCombo, Self::Key, &'static str)] {
        agent::all_shortcuts()
    }

    fn on_enter(&mut self, task_set: &mut FutureSet<Self>, _state: &mut Self::State) {
        self.paused = false;
        self.refresh(task_set, std::time::Duration::ZERO);
    }

    fn on_leave(&mut self, _task_set: &mut FutureSet<Self>, _state: &mut Self::State) {
        self.paused = true;
    }

    fn after_sync(&self, task_set: &mut FutureSet<Self>) {
        let interval = if self.error.is_some() { 5 } else { 2 };
        self.refresh(task_set, std::time::Duration::from_secs(interval));
    }
}

impl TabContent for SettingsContent {
    fn init(&mut self, _task_set: &mut FutureSet<Self>, state: &mut Self::State) {
        self.core_type = CONFIG.core_type();
        self.paused = true;
        self.current_mode = "Loading...".to_owned();
        self.ops = vec![SettingsOp::SwitchMode];
        self.modes = vec!["Rule".to_owned(), "Global".to_owned(), "Direct".to_owned()];
        self.tun_stacks = TunStack::VARIANTS.to_vec();
        self.mode_selector_state.select(Some(0));
        self.tun_selector_state.select(Some(0));
        if !self.ops.is_empty() {
            state.select(Some(0));
        }
    }

    fn handle_key_event(
        &mut self,
        key: SettingsKey,
        task_set: &mut FutureSet<Self>,
        state: &mut Self::State,
    ) {
        if matches!(key, SettingsKey::CloseOnMode) {
            self.close_on_mode = !self.close_on_mode;
            return;
        }
        if matches!(key, SettingsKey::EditRuntime | SettingsKey::PersistRuntime) {
            async move {
                let result = if matches!(key, SettingsKey::PersistRuntime) {
                    if crate::tui::widget::popmsg::Confirm::title("Save current settings to the override configuration?".to_owned())
                        .with_prompt("Future profile activation will use these settings. Enter confirms; Esc cancels.".to_owned())
                        .build_and_send().await.is_err() { return do_nothing(); }
                    crate::functions::restful::session::spawn_blocking(crate::functions::management::persist_runtime).await
                } else {
                    let fields = crate::functions::restful::session::spawn_blocking(crate::functions::restful::config::fetch_raw).await;
                    let fields = match fields { Ok(Ok(fields)) => fields, _ => { crate::tui::widget::popmsg::Confirm::err("Cannot read runtime configuration"); return do_nothing(); } };
                    let available = crate::functions::restful::config::writable_fields(&fields, CONFIG.core_type());
                    let Ok(patch) = Input::new().with_value("{}".to_owned())
                        .with_title("Temporary runtime patch (JSON)".to_owned())
                        .with_prompt(format!("Fields: {}. Example: {{\"log-level\":\"debug\"}}", available.join(", ")))
                        .build_and_send().await else { return do_nothing(); };
                    crate::functions::restful::session::spawn_blocking(move || {
                        let patch = serde_json::from_str(&patch)?;
                        crate::functions::restful::config::patch_checked(patch)?;
                        Ok(serde_json::Value::Null)
                    }).await
                };
                match result { Ok(Ok(_)) => {}, Ok(Err(error)) => crate::tui::widget::popmsg::Confirm::err(error), Err(error) => crate::tui::widget::popmsg::Confirm::err(error) }
                do_nothing()
            }.spawn_at(task_set);
            return;
        }
        if self.mode_selector_visible {
            match key {
                SettingsKey::MoveUp => {
                    let i = self.mode_selector_state.selected().unwrap_or(0);
                    self.mode_selector_state.select(Some(i.saturating_sub(1)));
                }
                SettingsKey::MoveDown => {
                    let i = self.mode_selector_state.selected().unwrap_or(0);
                    if i + 1 < self.modes.len() {
                        self.mode_selector_state.select(Some(i + 1));
                    }
                }
                SettingsKey::Esc => {
                    self.mode_selector_visible = false;
                }
                SettingsKey::EditRuntime
                | SettingsKey::PersistRuntime
                | SettingsKey::CloseOnMode => {}
                SettingsKey::Execute => {
                    let idx = self.mode_selector_state.selected().unwrap_or(0);
                    if let Some(mode) = self.modes.get(idx) {
                        let mode = mode.clone();
                        let close = self.close_on_mode;
                        self.mode_selector_visible = false;
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let payload = serde_json::json!({"mode": crate::functions::restful::config_struct::Mode::from(mode)}).to_string();
                            let result =
                                crate::functions::restful::session::spawn_blocking(move || {
                                    crate::functions::restful::config::patch_and_fetch(payload)
                                })
                                .await
                                .unwrap();
                            match result {
                                Ok(config) => {
                                    if close && let Err(error) = crate::functions::restful::session::spawn_blocking(crate::functions::restful::connection::terminate_all_connections).await.unwrap() { crate::tui::widget::popmsg::Confirm::err(format!("Mode changed, but closing connections failed: {error}")); }
                                    wrapper(move |c: &mut SettingsContent| c.apply_config(config))
                                }
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    do_nothing()
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                }
            }
            return;
        }

        if self.tun_selector_visible {
            match key {
                SettingsKey::MoveUp => {
                    let i = self.tun_selector_state.selected().unwrap_or(0);
                    self.tun_selector_state.select(Some(i.saturating_sub(1)));
                }
                SettingsKey::MoveDown => {
                    let i = self.tun_selector_state.selected().unwrap_or(0);
                    if i + 1 < self.tun_stacks.len() {
                        self.tun_selector_state.select(Some(i + 1));
                    }
                }
                SettingsKey::Esc => {
                    self.tun_selector_visible = false;
                }
                SettingsKey::EditRuntime
                | SettingsKey::PersistRuntime
                | SettingsKey::CloseOnMode => {}
                SettingsKey::Execute => {
                    let idx = self.tun_selector_state.selected().unwrap_or(0);
                    if let Some(stack) = self.tun_stacks.get(idx) {
                        let stack = *stack;
                        self.tun_selector_visible = false;
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let payload = serde_json::json!({"tun": {"stack": stack.to_string()}})
                                .to_string();
                            let result =
                                crate::functions::restful::session::spawn_blocking(move || {
                                    crate::functions::restful::config::patch_and_fetch(payload)
                                })
                                .await
                                .unwrap();
                            match result {
                                Ok(config) => {
                                    wrapper(move |c: &mut SettingsContent| c.apply_config(config))
                                }
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    do_nothing()
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                }
            }
            return;
        }

        match key {
            SettingsKey::EditRuntime | SettingsKey::PersistRuntime | SettingsKey::CloseOnMode => {}
            SettingsKey::MoveUp => {
                let i = state.selected().unwrap_or(0);
                state.select(Some(i.saturating_sub(1)));
            }
            SettingsKey::MoveDown => {
                let i = state.selected().unwrap_or(0);
                if i + 1 < self.ops.len() {
                    state.select(Some(i + 1));
                }
            }
            SettingsKey::Execute => {
                let Some(idx) = state.selected() else { return };
                let Some(op) = self.ops.get(idx) else { return };
                match op {
                    SettingsOp::SwitchMode => {
                        self.mode_selector_visible = true;
                    }
                    SettingsOp::AllowLan => {
                        let new_val = !self.allow_lan;
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let payload = serde_json::json!({"allow-lan": new_val}).to_string();
                            let result =
                                crate::functions::restful::session::spawn_blocking(move || {
                                    crate::functions::restful::config::patch_and_fetch(payload)
                                })
                                .await
                                .unwrap();
                            match result {
                                Ok(config) => {
                                    wrapper(move |c: &mut SettingsContent| c.apply_config(config))
                                }
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    wrapper(move |c: &mut SettingsContent| {
                                        c.error = Some(
                                            "Setting was not confirmed; refreshing".to_owned(),
                                        );
                                    })
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                    SettingsOp::TunEnable => {
                        let new_val = !self.tun_enable;
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let payload =
                                serde_json::json!({"tun": {"enable": new_val}}).to_string();
                            let result =
                                crate::functions::restful::session::spawn_blocking(move || {
                                    crate::functions::restful::config::patch_and_fetch(payload)
                                })
                                .await
                                .unwrap();
                            match result {
                                Ok(config) => {
                                    wrapper(move |c: &mut SettingsContent| c.apply_config(config))
                                }
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    wrapper(move |c: &mut SettingsContent| {
                                        c.error = Some(
                                            "Setting was not confirmed; refreshing".to_owned(),
                                        );
                                    })
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                    SettingsOp::TunStackOp => {
                        self.tun_selector_visible = true;
                    }
                    SettingsOp::FlushFakeIP => {
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let result = crate::functions::restful::session::spawn_blocking(|| {
                                crate::functions::restful::cache::flush_fakeip()
                            })
                            .await
                            .unwrap();
                            match result {
                                Ok(_) => do_nothing(),
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    do_nothing()
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                    SettingsOp::FlushDNSCache => {
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let result = crate::functions::restful::session::spawn_blocking(|| {
                                crate::functions::restful::cache::flush_dns()
                            })
                            .await
                            .unwrap();
                            match result {
                                Ok(_) => do_nothing(),
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    do_nothing()
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                    SettingsOp::UpdateGeo => {
                        async move {
                            if crate::config::is_core_mismatch() {
                                return do_nothing();
                            }
                            let result = crate::functions::restful::session::spawn_blocking(|| {
                                crate::functions::restful::geo::upgrade_geo()
                            })
                            .await
                            .unwrap();
                            match result {
                                Ok(_) => do_nothing(),
                                Err(e) => {
                                    crate::tui::widget::popmsg::Confirm::err(e);
                                    do_nothing()
                                }
                            }
                        }
                        .spawn_at(task_set);
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, state: &mut Self::State) {
        let title = self
            .error
            .as_ref()
            .map(|error| format!("Settings — {error}"))
            .unwrap_or_else(|| Self::TITLE.to_owned());
        let block = Block::bordered()
            .border_style(Theme::get().section("settings").border)
            .title(title)
            .title_bottom(format!(
                "temporary by default · e edits · p persists · c close on mode: {}",
                if self.close_on_mode { "yes" } else { "no" }
            ));

        if crate::config::is_core_mismatch() {
            let widget = Paragraph::new("API data mismatch with configured core").block(block);
            f.render_widget(widget, area);
            return;
        }

        let value_style = Theme::get().section("settings").muted;

        let items: Vec<ListItem> = self
            .ops
            .iter()
            .map(|op| {
                let (name, current) = match op {
                    SettingsOp::SwitchMode => ("Mode", self.current_mode.as_str()),
                    SettingsOp::AllowLan => {
                        let val = if self.allow_lan { "Yes" } else { "No" };
                        ("Allow LAN", val)
                    }
                    SettingsOp::TunEnable => {
                        let val = if self.tun_enable { "Yes" } else { "No" };
                        ("TUN", val)
                    }
                    SettingsOp::TunStackOp => ("TUN Stack", self.tun_stack.as_str()),
                    SettingsOp::FlushFakeIP => ("Flush Fake-IP", ""),
                    SettingsOp::FlushDNSCache => ("Flush DNS Cache", ""),
                    SettingsOp::UpdateGeo => ("Update GEO", ""),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("  {:<14}", name)),
                    Span::raw(current).style(value_style),
                ]))
            })
            .collect();

        let highlight_style = Theme::get().section("settings").highlight;
        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        f.render_stateful_widget(list, area, state);

        if self.mode_selector_visible {
            let select_area = centered_rect(60, 30, area);
            let mode_items: Vec<ListItem> = self
                .modes
                .iter()
                .map(|m| ListItem::new(format!("  {}", m)))
                .collect();
            let mode_list = List::new(mode_items)
                .block(
                    Block::bordered()
                        .border_style(Theme::get().section("settings").border)
                        .title("Mode"),
                )
                .highlight_style(highlight_style);
            f.render_widget(Clear, select_area);
            f.render_stateful_widget(
                mode_list,
                select_area,
                &mut self.mode_selector_state.clone(),
            );
        }

        if self.tun_selector_visible {
            let select_area = centered_rect(60, 30, area);
            let tun_items: Vec<ListItem> = self
                .tun_stacks
                .iter()
                .map(|s| ListItem::new(format!("  {}", s)))
                .collect();
            let tun_list = List::new(tun_items)
                .block(
                    Block::bordered()
                        .border_style(Theme::get().section("settings").border)
                        .title("TUN Stack"),
                )
                .highlight_style(highlight_style);
            f.render_widget(Clear, select_area);
            f.render_stateful_widget(tun_list, select_area, &mut self.tun_selector_state.clone());
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_settings_refresh_replaces_previous_values() {
        let mut content = SettingsContent::default();
        content.apply_config(serde_json::from_str(r#"{"mode":"rule","allow-lan":false}"#).unwrap());
        assert_eq!(content.current_mode, "Rule");
        assert!(!content.allow_lan);
        content.error = Some("disconnected".to_owned());
        content
            .apply_config(serde_json::from_str(r#"{"mode":"global","allow-lan":true}"#).unwrap());
        assert_eq!(content.current_mode, "Global");
        assert!(content.allow_lan);
        assert!(content.error.is_none());
    }

    #[test]
    fn absent_fields_do_not_offer_lan_or_tun_mutations() {
        let mut content = SettingsContent::default();
        content.apply_config(serde_json::from_str(r#"{"mode":"rule"}"#).unwrap());
        assert!(!content.ops.contains(&SettingsOp::AllowLan));
        assert!(!content.ops.contains(&SettingsOp::TunEnable));
        assert!(!content.ops.contains(&SettingsOp::TunStackOp));
        assert!(content.ops.contains(&SettingsOp::SwitchMode));
    }

    fn mk_key(code: KeyCode) -> crate::tui::Key {
        crate::tui::Key {
            code,
            shift: matches!(code, KeyCode::Char(c) if c.is_ascii_uppercase()),
            ctrl: false,
            alt: false,
            super_: false,
        }
    }

    #[test]
    fn settings_key_agent_contains_expected() {
        let a = agent();
        assert!(a.contains_key(&mk_key(KeyCode::Enter)));
        assert!(a.contains_key(&mk_key(KeyCode::Esc)));
        assert!(a.contains_key(&mk_key(KeyCode::Up)));
        assert!(a.contains_key(&mk_key(KeyCode::Down)));
        assert!(a.contains_key(&mk_key(KeyCode::Char('k'))));
        assert!(a.contains_key(&mk_key(KeyCode::Char('j'))));
    }

    #[test]
    fn settings_key_try_from_correct_actions() {
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Enter)),
            Ok(SettingsKey::Execute)
        ));
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Esc)),
            Ok(SettingsKey::Esc)
        ));
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Up)),
            Ok(SettingsKey::MoveUp)
        ));
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Down)),
            Ok(SettingsKey::MoveDown)
        ));
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Char('k'))),
            Ok(SettingsKey::MoveUp)
        ));
        assert!(matches!(
            SettingsKey::try_from(&mk_key(KeyCode::Char('j'))),
            Ok(SettingsKey::MoveDown)
        ));
    }

    #[test]
    fn settings_key_try_from_unknown_key_is_err() {
        assert!(SettingsKey::try_from(&mk_key(KeyCode::Char('x'))).is_err());
        assert!(SettingsKey::try_from(&mk_key(KeyCode::Backspace)).is_err());
    }

    #[test]
    fn settings_op_all_is_non_empty() {
        let ops = SettingsOp::all(CoreType::Mihomo);
        assert!(!ops.is_empty());
        assert!(ops.contains(&SettingsOp::SwitchMode));
        assert!(ops.contains(&SettingsOp::AllowLan));
    }

    #[test]
    fn settings_content_default_has_empty_ops() {
        let c = SettingsContent::default();
        assert!(c.ops.is_empty());
        assert!(!c.mode_selector_visible);
        assert!(!c.tun_selector_visible);
    }
}
