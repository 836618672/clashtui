use super::utils::raw_mode;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) static SUSPENDED: AtomicBool = AtomicBool::new(false);

pub fn setup() -> anyhow::Result<()> {
    raw_mode::setup()?;
    raw_mode::set_panic_hook();
    Ok(())
}

pub fn teardown() {
    let _ = raw_mode::restore();
}

pub fn hold(on: bool) -> anyhow::Result<()> {
    if !super::ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    if on {
        SUSPENDED.store(true, Ordering::Release);
        if let Err(error) = raw_mode::restore() {
            SUSPENDED.store(false, Ordering::Release);
            return Err(error.into());
        }
    } else {
        raw_mode::setup()?;
        SUSPENDED.store(false, Ordering::Release);
    }
    super::app::FULL_RENDER.notify_one();
    Ok(())
}
