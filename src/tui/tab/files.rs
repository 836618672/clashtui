use ratatui::{
    text::{Line, Span},
    widgets::ListItem,
};

use super::dev::*;

/// The Only reason why I use two functions to `sync` is that
/// I except modifying Self (what we do in `wrapper`) is
/// fast and infallable
///
/// Tasks should be done in async{} and left only values that
/// apply to Self
macro_rules! sync {
    ($ident: ty) => {{
        let (name, atime) = crate::functions::restful::session::spawn_blocking(
            super::profile::get_profiles_with_readable_atime,
        )
        .await
        .unwrap();
        wrapper(|(content, _): &mut $ident| super::profile::sync_helper(content, name, atime))
    }};
}

macro_rules! get_name {
    ($self:expr, $state:expr) => {
        if let Some(idx) = $state.selected() {
            if let Some(name) = visible_items(&$self.items, $self.filter.as_deref()).nth(idx) {
                name.clone()
            } else {
                return false;
            }
        } else {
            return false;
        }
    };
}

fn visible_items<'a>(
    items: &'a [String],
    filter: Option<&'a str>,
) -> impl Iterator<Item = &'a String> {
    items
        .iter()
        .filter(move |name| filter.is_none_or(|pat| name.contains(pat)))
}

fn clamp_selection(state: &mut ListState, len: usize) {
    state.select(if len == 0 {
        None
    } else {
        Some(state.selected().unwrap_or(0).min(len - 1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_actions_use_the_visible_row_not_the_unfiltered_index() {
        struct Content {
            items: Vec<String>,
            filter: Option<String>,
        }
        fn selected(content: &Content, state: &ListState, result: &mut Option<String>) -> bool {
            let name = get_name!(content, state);
            *result = Some(name);
            true
        }
        let content = Content {
            items: vec!["alpha".into(), "beta".into(), "beta-2".into()],
            filter: Some("beta".into()),
        };
        let mut state = ListState::default();
        clamp_selection(&mut state, 2);
        let mut result = None;
        assert!(selected(&content, &state, &mut result));
        assert_eq!(result.as_deref(), Some("beta"));
        state.select(Some(1));
        assert!(selected(&content, &state, &mut result));
        assert_eq!(result.as_deref(), Some("beta-2"));
        clamp_selection(&mut state, 0);
        assert!(!selected(&content, &state, &mut result));
    }
}

pub(crate) mod profile;
pub(crate) mod template;

newtype_tab!(
    /// This can only be [DualTab], because [Template] needs to update [Profile]
    ///
    /// [Template]: template::Template
    /// [Profile]: profile::Profile
    FileTab(DualTab<profile::Profile, template::Template>),
    "File"
);

pub fn agent_init(mut keymap: serde_yml::Mapping) -> anyhow::Result<()> {
    if let Some(val) = keymap.remove("profile") {
        match val {
            serde_yml::Value::Mapping(map) => {
                crate::tui::agent::check_duplicate_keys("file/profile", &map);
                let (keys, descs) = crate::tui::agent::extract_keymap_with_descs(map)?;
                profile::agent_init(keys);
                profile::init_descs(descs);
            }
            serde_yml::Value::Sequence(seq) => {
                let entries: Vec<crate::tui::agent::Entry> =
                    serde_yml::from_value(serde_yml::Value::Sequence(seq))?;
                crate::tui::agent::check_duplicate_keys_list("file/profile", &entries);
                let (keys, descs, chords) = crate::tui::agent::extract_keymap_list(entries)?;
                profile::agent_init(keys);
                profile::init_descs(descs);
                profile::init_chords(chords);
            }
            _ => anyhow::bail!("file/profile is neither Mapping nor Sequence"),
        }
    }
    if let Some(val) = keymap.remove("template") {
        match val {
            serde_yml::Value::Mapping(map) => {
                crate::tui::agent::check_duplicate_keys("file/template", &map);
                let (keys, descs) = crate::tui::agent::extract_keymap_with_descs(map)?;
                template::agent_init(keys);
                template::init_descs(descs);
            }
            serde_yml::Value::Sequence(seq) => {
                let entries: Vec<crate::tui::agent::Entry> =
                    serde_yml::from_value(serde_yml::Value::Sequence(seq))?;
                crate::tui::agent::check_duplicate_keys_list("file/template", &entries);
                let (keys, descs, chords) = crate::tui::agent::extract_keymap_list(entries)?;
                template::agent_init(keys);
                template::init_descs(descs);
                template::init_chords(chords);
            }
            _ => anyhow::bail!("file/template is neither Mapping nor Sequence"),
        }
    }
    Ok(())
}
