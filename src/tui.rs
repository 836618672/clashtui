use std::sync::atomic::AtomicBool;

mod agent;
mod app;
mod key;
mod popmsg;
mod signals;
mod tab;
mod term;
mod theme;
mod utils;
mod widget;

pub use app::App;
pub use key::Key;
pub use term::hold;
pub(crate) use theme::Theme;

pub static EXT_PROC: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_active() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Acquire)
}

trait TuiWidget {
    fn handle_key_event(&mut self, kv: &Key);
    fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect);
    fn sync(&mut self);
    fn on_enter(&mut self) {}
    fn on_leave(&mut self) {}
}

pub fn init() -> anyhow::Result<()> {
    agent::init()?;
    theme::Theme::load();
    term::setup()?;
    ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

pub fn restore() -> anyhow::Result<()> {
    ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    term::teardown();
    Ok(())
}
