use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::settings::SettingsState;
use crate::state::{Error, Result};

pub const EVENT: &str = "update-status";

const FIRST_CHECK: Duration = Duration::from_secs(15);
const TICK: Duration = Duration::from_secs(10 * 60);
const PERIOD: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum UpdateStatus {
    Unavailable,
    Managed { manager: String },
    Idle,
    Checking,
    Downloading { version: String },
    Ready { version: String },
    Failed { message: String },
}

pub fn is_release(version: &str, debug: bool) -> bool {
    !debug && version != "0.0.0"
}

fn manager_of(dir: &Path) -> Option<&'static str> {
    dir.join("scoop-manifest.json").is_file().then_some("Scoop")
}

pub fn initial_status(version: &str, debug: bool, exe: Option<&Path>) -> UpdateStatus {
    if !is_release(version, debug) {
        return UpdateStatus::Unavailable;
    }
    match exe.and_then(Path::parent).and_then(manager_of) {
        Some(manager) => UpdateStatus::Managed {
            manager: manager.to_owned(),
        },
        None => UpdateStatus::Idle,
    }
}

fn due(last: Option<SystemTime>) -> bool {
    last.is_none_or(|at| !matches!(at.elapsed(), Ok(passed) if passed < PERIOD))
}

pub struct Updates {
    release: bool,
    inner: Mutex<Inner>,
}

struct Inner {
    status: UpdateStatus,
    ready: Option<(Update, Vec<u8>)>,
}

impl Updates {
    pub fn new(status: UpdateStatus) -> Self {
        Updates {
            release: status == UpdateStatus::Idle,
            inner: Mutex::new(Inner {
                status,
                ready: None,
            }),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.lock().status.clone()
    }

    fn begin(&self) -> bool {
        let mut inner = self.lock();
        if !self.release
            || matches!(
                inner.status,
                UpdateStatus::Checking
                    | UpdateStatus::Downloading { .. }
                    | UpdateStatus::Ready { .. }
            )
        {
            return false;
        }
        inner.status = UpdateStatus::Checking;
        true
    }

    fn set(&self, status: UpdateStatus) {
        self.lock().status = status;
    }

    fn keep(&self, update: Update, bytes: Vec<u8>) {
        let mut inner = self.lock();
        inner.status = UpdateStatus::Ready {
            version: update.version.clone(),
        };
        inner.ready = Some((update, bytes));
    }

    fn take(&self) -> Option<(Update, Vec<u8>)> {
        self.lock().ready.take()
    }

    fn put_back(&self, ready: (Update, Vec<u8>)) {
        self.lock().ready = Some(ready);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("the update state is poisoned")
    }
}

fn publish<R: Runtime>(app: &AppHandle<R>, status: UpdateStatus) -> UpdateStatus {
    app.state::<Updates>().set(status.clone());
    let _ = app.emit(EVENT, status.clone());
    status
}

pub async fn check<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let updates = app.state::<Updates>();
    if !updates.begin() {
        return updates.status();
    }
    let _ = app.emit(EVENT, UpdateStatus::Checking);

    match fetch(app).await {
        Ok(Some((update, bytes))) => {
            updates.keep(update, bytes);
            let status = updates.status();
            let _ = app.emit(EVENT, status.clone());
            status
        }
        Ok(None) => publish(app, UpdateStatus::Idle),
        Err(error) => {
            eprintln!("cannot check for updates: {error}");
            publish(
                app,
                UpdateStatus::Failed {
                    message: error.to_string(),
                },
            )
        }
    }
}

async fn fetch<R: Runtime>(
    app: &AppHandle<R>,
) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
    let Some(update) = app.updater()?.check().await? else {
        return Ok(None);
    };
    publish(
        app,
        UpdateStatus::Downloading {
            version: update.version.clone(),
        },
    );
    let bytes = update.download(|_, _| {}, || {}).await?;
    Ok(Some((update, bytes)))
}

pub fn schedule<R: Runtime>(app: AppHandle<R>) {
    if !app.state::<Updates>().release {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut last = None;
        loop {
            if due(last) && app.state::<SettingsState>().get().auto_update {
                tauri::async_runtime::block_on(check(&app));
                last = Some(SystemTime::now());
            }
            std::thread::sleep(TICK);
        }
    });
}

pub fn install_on_exit<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(windows) {
        return;
    }
    if let Some((update, bytes)) = app.state::<Updates>().take() {
        if let Err(error) = update.install(bytes) {
            eprintln!("cannot install the update: {error}");
        }
    }
}

#[tauri::command]
pub fn update_status(updates: State<'_, Updates>) -> UpdateStatus {
    updates.status()
}

#[tauri::command]
pub async fn check_update<R: Runtime>(app: AppHandle<R>) -> UpdateStatus {
    check(&app).await
}

#[tauri::command(async)]
pub fn restart_to_update<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    let updates = app.state::<Updates>();
    let Some(ready) = updates.take() else {
        return Err(Error::UpdateNotReady);
    };

    if crate::sync_status::CLOSING.start() {
        crate::sync_status::sync_on_close(&app, crate::CLOSE_WAIT);
    }
    if let Err(error) =
        install_or_reopen(&crate::sync_status::CLOSING, || ready.0.install(&ready.1))
    {
        updates.put_back(ready);
        return Err(Error::Update(error.to_string()));
    }
    app.restart()
}

fn install_or_reopen<E>(
    latch: &crate::sync_status::CloseLatch,
    install: impl FnOnce() -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    let installed = install();
    if installed.is_err() {
        latch.reopen();
    }
    installed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_status::CloseLatch;

    #[test]
    fn a_failed_install_opens_the_close_latch_again() {
        let latch = CloseLatch::new();
        assert!(latch.start());

        let result = install_or_reopen(&latch, || Err("no disk"));

        assert_eq!(result, Err("no disk"));
        assert!(latch.start());
    }

    #[test]
    fn a_successful_install_keeps_the_close_latch_closed() {
        let latch = CloseLatch::new();
        assert!(latch.start());

        assert_eq!(install_or_reopen(&latch, || Ok::<(), ()>(())), Ok(()));
        assert!(!latch.start());
    }

    #[test]
    fn a_developer_build_does_not_check() {
        assert!(!is_release("0.0.0", false), "pnpm install:app");
        assert!(!is_release("0.0.0", true), "pnpm dev");
        assert!(!is_release("0.1.7", true), "отладочная сборка с версией");
        assert!(is_release("0.1.7", false), "сборка из релиза");
    }

    #[test]
    fn a_copy_of_a_package_manager_does_not_update_itself() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("kknow-desktop.exe");

        assert_eq!(
            initial_status("0.7.2", false, Some(&exe)),
            UpdateStatus::Idle,
            "установка из MSI обновляет себя сама"
        );

        std::fs::write(dir.path().join("scoop-manifest.json"), "{}").unwrap();
        assert_eq!(
            initial_status("0.7.2", false, Some(&exe)),
            UpdateStatus::Managed {
                manager: "Scoop".into()
            }
        );

        assert_eq!(
            initial_status("0.0.0", false, Some(&exe)),
            UpdateStatus::Unavailable,
            "сборка разработчика важнее: она и так не проверяет"
        );
        assert_eq!(initial_status("0.7.2", false, None), UpdateStatus::Idle);
    }

    #[test]
    fn a_managed_copy_says_so_and_never_starts() {
        let updates = Updates::new(UpdateStatus::Managed {
            manager: "Scoop".into(),
        });

        assert!(!updates.begin());
        assert_eq!(
            updates.status(),
            UpdateStatus::Managed {
                manager: "Scoop".into()
            }
        );
    }

    #[test]
    fn a_developer_build_says_so_and_never_starts() {
        let updates = Updates::new(UpdateStatus::Unavailable);

        assert_eq!(updates.status(), UpdateStatus::Unavailable);
        assert!(!updates.begin());
        assert_eq!(updates.status(), UpdateStatus::Unavailable);
    }

    #[test]
    fn a_second_check_waits_for_the_first() {
        let updates = Updates::new(UpdateStatus::Idle);

        assert!(updates.begin());
        assert_eq!(updates.status(), UpdateStatus::Checking);
        assert!(!updates.begin(), "таймер и кнопка не качают дважды");

        updates.set(UpdateStatus::Downloading {
            version: "0.2.0".into(),
        });
        assert!(!updates.begin());
    }

    #[test]
    fn a_downloaded_version_is_not_downloaded_again() {
        let updates = Updates::new(UpdateStatus::Idle);
        updates.set(UpdateStatus::Ready {
            version: "0.2.0".into(),
        });

        assert!(!updates.begin());
        assert_eq!(
            updates.status(),
            UpdateStatus::Ready {
                version: "0.2.0".into()
            }
        );
    }

    #[test]
    fn a_failure_or_nothing_new_allows_the_next_check() {
        let updates = Updates::new(UpdateStatus::Idle);

        updates.set(UpdateStatus::Failed {
            message: "нет сети".into(),
        });
        assert!(updates.begin());

        updates.set(UpdateStatus::Idle);
        assert!(updates.begin());
    }

    #[test]
    fn nothing_to_install_without_a_download() {
        assert!(Updates::new(UpdateStatus::Idle).take().is_none());
    }

    #[test]
    fn the_scheduler_checks_at_start_and_then_after_the_period() {
        let now = SystemTime::now();

        assert!(due(None), "первая проверка");
        assert!(!due(Some(now)), "только что проверили");
        assert!(due(Some(now - PERIOD)), "прошло шесть часов");
        assert!(
            due(Some(now + Duration::from_secs(3600))),
            "часы ушли назад — проверяем"
        );
    }

    #[test]
    fn the_status_is_tagged_for_the_window() {
        let ready = serde_json::to_value(UpdateStatus::Ready {
            version: "0.2.0".into(),
        })
        .unwrap();
        assert_eq!(
            ready,
            serde_json::json!({"state": "ready", "version": "0.2.0"})
        );

        let off = serde_json::to_value(UpdateStatus::Unavailable).unwrap();
        assert_eq!(off, serde_json::json!({"state": "unavailable"}));
    }
}
