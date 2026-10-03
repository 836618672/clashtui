//! Cooperating processes serialize local writes using an OS file lock.
use anyhow::Result;
use std::cell::Cell;
use std::fs::File;
thread_local! { static DEPTH: Cell<usize> = const { Cell::new(0) }; }

pub fn is_write_locked() -> bool {
    DEPTH.with(|depth| depth.get() > 0)
}

/// Keep the OS lock on one worker for the entire async file transaction.
/// Management's synchronous runtime already owns this lock on its thread.
pub async fn transaction<F, Fut, T>(task: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T>>,
    T: Send + 'static,
{
    if is_write_locked() {
        return task().await;
    }
    crate::functions::restful::session::spawn_blocking(move || {
        let _write = WriteGuard::acquire()?;
        if let Some(session) = crate::functions::restful::session::CoreSession::try_current() {
            anyhow::ensure!(
                session.is_current(),
                "Core changed before transaction; refresh and retry"
            );
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(task())
    })
    .await?
}

pub struct WriteGuard {
    _file: Option<File>,
    // A reentrant guard must be dropped on its originating thread.
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl WriteGuard {
    pub fn acquire() -> Result<Self> {
        Self::acquire_inner(false)
    }

    pub fn try_acquire() -> Result<Self> {
        Self::acquire_inner(true)
    }

    fn acquire_inner(nonblocking: bool) -> Result<Self> {
        let file = if DEPTH.with(|depth| depth.get() == 0) {
            let path = crate::config::config_dir_path().join(".management.lock");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path)?;
            if nonblocking {
                file.try_lock()
                    .map_err(|error| anyhow::anyhow!("Management state is busy: {error}"))?;
            } else {
                file.lock()?;
            }
            Some(file)
        } else {
            None
        };
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        Ok(Self {
            _file: file,
            _thread: std::marker::PhantomData,
        })
    }
}
impl Drop for WriteGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

pub struct DatabaseGuard {
    data: std::sync::MutexGuard<'static, crate::config::database::ProfileManager>,
    _write: WriteGuard,
    load_error: Option<String>,
}
impl DatabaseGuard {
    pub fn ensure_loaded(&self) -> Result<()> {
        if let Some(error) = &self.load_error {
            anyhow::bail!("Cannot use an unreadable database: {error}");
        }
        Ok(())
    }
    pub fn to_file(&self) -> Result<()> {
        self.ensure_loaded()?;
        self.data.to_file()
    }
}
impl std::ops::Deref for DatabaseGuard {
    type Target = crate::config::database::ProfileManager;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}
impl std::ops::DerefMut for DatabaseGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}
pub fn database() -> DatabaseGuard {
    let write = WriteGuard::acquire().expect("Cannot lock the profile database");
    let mut data = crate::config::CONFIG.data.lock().unwrap();
    let mut load_error = None;
    match crate::config::database::ProfileManager::from_file() {
        Ok(mut disk) => {
            disk.core_type = data.core_type;
            *data = disk;
        }
        Err(error) => {
            log::error!("Cannot refresh profile database: {error}");
            load_error = Some(error.to_string());
        }
    }
    DatabaseGuard {
        data,
        _write: write,
        load_error,
    }
}
