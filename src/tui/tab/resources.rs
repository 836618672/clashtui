use super::dev::*;
use crate::functions::restful::resources::{self, ResourceItem, ResourceKind};
use ratatui::widgets::Paragraph;
use std::cell::Cell;
use std::time::Duration;

newtype_tab!(RulesTab(Tab<ResourceContent<0>>), "Rules");
newtype_tab!(ProvidersTab(Tab<ResourceContent<1>>), "Providers");

mod_agent!(
    Key,
    [
        ([KeyCode::Char('j')], Key::Down, "Down"),
        ([KeyCode::Down], Key::Down, "Down"),
        ([KeyCode::Char('k')], Key::Up, "Up"),
        ([KeyCode::Up], Key::Up, "Up"),
        ([KeyCode::Char('/')], Key::Search, "Filter"),
        ([KeyCode::Esc], Key::Clear, "Clear filter"),
        ([KeyCode::Enter], Key::Details, "Details"),
        ([KeyCode::Char('r')], Key::Refresh, "Refresh"),
        (
            [KeyCode::Char('u')],
            Key::Update,
            "Update provider / toggle rule"
        ),
        ([KeyCode::Char('U')], Key::UpdateAll, "Update all providers"),
        ([KeyCode::Char('h')], Key::Health, "Provider health check"),
        ([KeyCode::Char('d')], Key::NodeDelay, "Test a provider node"),
        ([KeyCode::Char('t')], Key::Switch, "Switch provider type"),
        ([KeyCode::Char('e')], Key::Export, "Export displayed rows"),
    ]
);

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Key {
    Up,
    Down,
    Search,
    Clear,
    Details,
    Refresh,
    Update,
    UpdateAll,
    Health,
    NodeDelay,
    Switch,
    Export,
}
impl TryFrom<&crate::tui::Key> for Key {
    type Error = ();
    fn try_from(key: &crate::tui::Key) -> Result<Self, ()> {
        agent().get(key).copied().ok_or(())
    }
}

pub struct ResourceContent<const KIND: u8> {
    kind: ResourceKind,
    items: Vec<ResourceItem>,
    filter: String,
    error: Option<String>,
    notice: Option<String>,
    paused: bool,
    polling: Cell<bool>,
    generation: Cell<u64>,
}
impl<const KIND: u8> Default for ResourceContent<KIND> {
    fn default() -> Self {
        Self {
            kind: if KIND == 0 {
                ResourceKind::Rules
            } else {
                ResourceKind::ProxyProviders
            },
            items: vec![],
            filter: String::new(),
            error: None,
            notice: None,
            paused: true,
            polling: Cell::new(false),
            generation: Cell::new(0),
        }
    }
}
impl<const KIND: u8> ResourceContent<KIND> {
    fn visible(&self) -> Vec<&ResourceItem> {
        let filter = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|row| {
                row.label.to_lowercase().contains(&filter)
                    || row.value.to_string().to_lowercase().contains(&filter)
            })
            .collect()
    }
    fn refresh(&self, tasks: &mut FutureSet<Self>, delay: Duration) {
        if self.paused || self.polling.replace(true) {
            return;
        }
        let kind = self.kind;
        let generation = self.generation.get();
        async move {
            tokio::time::sleep(delay).await;
            let result =
                crate::functions::restful::session::spawn_blocking(move || resources::fetch(kind))
                    .await;
            wrapper(move |content: &mut Self| {
                if content.generation.get() != generation {
                    return;
                }
                content.polling.set(false);
                if content.kind != kind {
                    return;
                }
                match result {
                    Ok(Ok(rows)) => {
                        content.items = rows;
                        content.error = None;
                    }
                    Ok(Err(error)) => content.error = Some(error.to_string()),
                    Err(error) => content.error = Some(error.to_string()),
                }
            })
        }
        .spawn_at(tasks);
    }
}
impl<const KIND: u8> BasicTabContent for ResourceContent<KIND> {
    type Key = Key;
    type State = ListState;
    const TITLE: &'static str = if KIND == 0 { "Rules" } else { "Providers" };
    fn all_shortcuts() -> &'static [(KeyCombo, Key, &'static str)] {
        agent::all_shortcuts()
    }
    fn on_enter(&mut self, tasks: &mut FutureSet<Self>, state: &mut ListState) {
        self.paused = false;
        if state.selected().is_none() {
            state.select(Some(0));
        }
        self.refresh(tasks, Duration::ZERO);
    }
    fn on_leave(&mut self, _tasks: &mut FutureSet<Self>, _state: &mut ListState) {
        self.paused = true;
        self.generation.set(self.generation.get().wrapping_add(1));
        self.polling.set(false);
    }
    fn after_sync(&self, tasks: &mut FutureSet<Self>) {
        self.refresh(
            tasks,
            Duration::from_secs(if self.error.is_some() { 5 } else { 2 }),
        );
    }
}
impl<const KIND: u8> TabContent for ResourceContent<KIND> {
    fn init(&mut self, _tasks: &mut FutureSet<Self>, _state: &mut ListState) {}
    fn handle_key_event(&mut self, key: Key, tasks: &mut FutureSet<Self>, state: &mut ListState) {
        let rows = self.visible();
        let selected = rows
            .get(state.selected().unwrap_or(0))
            .map(|row| (*row).clone());
        let count = rows.len();
        match key {
            Key::Up => state.select(
                count
                    .checked_sub(1)
                    .map(|max| state.selected().unwrap_or(0).min(max).saturating_sub(1)),
            ),
            Key::Down => state.select(
                count
                    .checked_sub(1)
                    .map(|max| (state.selected().unwrap_or(0) + 1).min(max)),
            ),
            Key::Clear => {
                self.filter.clear();
                state.select(Some(0));
            }
            Key::Search => {
                let value = self.filter.clone();
                state.select(Some(0));
                async move {
                    let filter = Input::new()
                        .with_value(value)
                        .with_title("Filter resources".to_owned())
                        .build_and_send()
                        .await;
                    wrapper(move |c: &mut Self| {
                        if let Ok(filter) = filter {
                            c.filter = filter;
                        }
                    })
                }
                .spawn_at(tasks);
            }
            Key::Details => {
                if let Some(row) = selected {
                    crate::tui::widget::popmsg::Confirm::dismiss_any(row.id)
                        .with_prompt(serde_json::to_string_pretty(&row.value).unwrap_or_default())
                        .build_and_send();
                }
            }
            Key::Refresh => {
                self.refresh(tasks, Duration::ZERO);
            }
            Key::NodeDelay if self.kind == ResourceKind::ProxyProviders => {
                if let Some(row) = selected {
                    let test_url = row.value["testUrl"].as_str().map(str::to_owned);
                    async move {
                        let nodes: Vec<_> = row.value["proxies"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|node| node["name"].as_str())
                            .collect();
                        let Ok(name) = Input::new()
                            .with_title(format!("Provider node name ({})", nodes.join(", ")))
                            .build_and_send()
                            .await
                        else {
                            return do_nothing();
                        };
                        if !nodes.contains(&name.as_str()) {
                            crate::tui::widget::popmsg::Confirm::err(
                                "Node is not in the selected provider",
                            );
                            return do_nothing();
                        }
                        let result =
                            crate::functions::restful::session::spawn_blocking(move || {
                                crate::functions::restful::proxies::test_provider_node_delay(
                                    &row.id,
                                    &name,
                                    test_url.as_deref(),
                                    crate::config::CONFIG.cfg_file.timeout.unwrap_or(5).max(1)
                                        * 1000,
                                )
                            })
                            .await;
                        wrapper(move |content: &mut Self| {
                            content.notice = Some(match result {
                                Ok(Ok(Some(delay))) => format!("Provider node delay: {delay} ms"),
                                Ok(Ok(None)) => {
                                    "Provider node returned no successful delay".to_owned()
                                }
                                Ok(Err(error)) => error.to_string(),
                                Err(error) => error.to_string(),
                            });
                        })
                    }
                    .spawn_at(tasks);
                }
            }
            Key::Switch if KIND == 1 => {
                // An old delayed poll must not block or overwrite the newly
                // selected provider type. Invalidate it and fetch immediately.
                self.generation.set(self.generation.get().wrapping_add(1));
                self.polling.set(false);
                self.kind = if self.kind == ResourceKind::ProxyProviders {
                    ResourceKind::RuleProviders
                } else {
                    ResourceKind::ProxyProviders
                };
                self.items.clear();
                self.filter.clear();
                self.error = None;
                self.notice = None;
                state.select(Some(0));
                self.refresh(tasks, Duration::ZERO);
            }
            Key::Update | Key::UpdateAll | Key::Health => {
                if self.error.is_some() {
                    return;
                }
                let kind = self.kind;
                if matches!(key, Key::Health) && kind != ResourceKind::ProxyProviders {
                    return;
                }
                if matches!(key, Key::UpdateAll) && kind == ResourceKind::Rules {
                    return;
                }
                let items = if matches!(key, Key::UpdateAll) {
                    self.items.clone()
                } else {
                    selected.into_iter().collect()
                };
                if items.is_empty() {
                    return;
                }
                async move {
                    if crate::tui::widget::popmsg::Confirm::title(format!(
                        "Apply to {} {}?",
                        items.len(),
                        kind.title()
                    ))
                    .with_prompt("Enter applies; Esc cancels".to_owned())
                    .build_and_send()
                    .await
                    .is_err()
                    {
                        return do_nothing();
                    }
                    let result = crate::functions::restful::session::spawn_blocking(move || {
                        let mut errors = vec![];
                        for row in items {
                            let result = if matches!(key, Key::Health) {
                                resources::healthcheck(&row.id)
                            } else {
                                resources::update(kind, &row.id, row.value["disabled"].as_bool())
                            };
                            if let Err(error) = result {
                                errors.push(format!("{}: {error}", row.id));
                            }
                        }
                        let rows = resources::fetch(kind);
                        (errors, rows)
                    })
                    .await;
                    wrapper(move |c: &mut Self| {
                        if c.kind != kind {
                            return;
                        }
                        match result {
                            Ok((errors, rows)) => {
                                c.notice = Some(if errors.is_empty() {
                                    "Operation completed".to_owned()
                                } else {
                                    errors.join("; ")
                                });
                                match rows {
                                    Ok(rows) => {
                                        c.items = rows;
                                        c.error = None;
                                    }
                                    Err(error) => c.error = Some(error.to_string()),
                                }
                            }
                            Err(error) => c.error = Some(error.to_string()),
                        }
                    })
                }
                .spawn_at(tasks);
            }
            Key::Export => {
                let rows: Vec<_> = rows.iter().map(|row| row.value.clone()).collect();
                async move {
                    let Ok(path) = Input::new()
                        .with_title("Export JSON path".to_owned())
                        .build_and_send()
                        .await
                    else {
                        return do_nothing();
                    };
                    let result = crate::functions::restful::session::spawn_blocking(move || {
                        crate::functions::file::export_json(std::path::Path::new(&path), &rows)
                    })
                    .await;
                    wrapper(move |c: &mut Self| {
                        c.notice = Some(match result {
                            Ok(Ok(())) => "Exported".to_owned(),
                            Ok(Err(e)) => e.to_string(),
                            Err(e) => e.to_string(),
                        })
                    })
                }
                .spawn_at(tasks);
            }
            _ => {}
        }
    }
    fn render(&self, f: &mut Frame, area: Rect, state: &mut ListState) {
        let title = format!(
            "{} | {} | {}",
            self.kind.title(),
            self.filter,
            self.notice.as_deref().unwrap_or("")
        );
        let block = Block::bordered().title(title);
        if let Some(error) = &self.error {
            f.render_widget(
                Paragraph::new(format!(
                    "State unavailable: {error}\nRetrying automatically; r refreshes."
                ))
                .block(block),
                area,
            );
            return;
        }
        let rows = self.visible();
        if rows.is_empty() {
            f.render_widget(Paragraph::new("No matching resources").block(block), area);
            return;
        }
        state.select(Some(state.selected().unwrap_or(0).min(rows.len() - 1)));
        let items = rows.iter().map(|row| row.label.clone());
        f.render_stateful_widget(
            List::new(items).block(block).highlight_symbol("▶ "),
            area,
            state,
        );
    }
}
