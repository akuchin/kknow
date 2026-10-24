//! Auto-update: check for a new version, download it in the background and
//! install it on restart.
//!
//! The model is Zed's, not Sublime's: no dialog. The check and the download
//! are silent, and the status bar shows "Restart" when the new version is on
//! disk. The user picks the moment, so an update never interrupts typing. If
//! the user simply quits, the new version goes in on exit, and the next start
//! is the new version. Windows is the exception: there the update is an
//! installer with its own window, so it runs only on the button.
//!
//! The release workflow publishes `latest.json` and the signed archives. The
//! plugin checks the signature with the public key from `tauri.conf.json`, so
//! a changed archive does not install. All work is on the Rust side, as with
//! sync: the windows get the state through commands and the `update-status`
//! event. Thus the mock replaces three commands, and the windows need no
//! plugin permissions.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::settings::SettingsState;
use crate::state::{Error, Result};

/// Event name for the windows.
pub const EVENT: &str = "update-status";

/// The first check waits a little: at start the disk and the index come
/// first, not the network.
const FIRST_CHECK: Duration = Duration::from_secs(15);
/// How often the scheduler looks at the clock. A check after a toggle in the
/// settings or after a sleep of the laptop comes no later than this.
const TICK: Duration = Duration::from_secs(10 * 60);
/// Releases are rare: a check a few times a day is enough.
const PERIOD: Duration = Duration::from_secs(6 * 60 * 60);

/// What the windows show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    /// A developer build: it never checks.
    Unavailable,
    /// No check yet, or the last check found nothing new.
    Idle,
    Checking,
    Downloading {
        version: String,
    },
    /// The new version is on disk and waits for a restart.
    Ready {
        version: String,
    },
    Failed {
        message: String,
    },
}

/// A developer build does not check for updates.
///
/// Both `pnpm dev` and `pnpm install:app` give version 0.0.0: only the release
/// workflow writes the tag version into `Cargo.toml`. A check by
/// `debug_assertions` alone is not enough: `install:app` is a release build,
/// and it would see each published version as an update.
pub fn is_release(version: &str, debug: bool) -> bool {
    !debug && version != "0.0.0"
}

/// Whether the scheduler must check now. A clock that went back (`elapsed`
/// fails) also means "check": a wrong clock must not stop updates.
fn due(last: Option<SystemTime>) -> bool {
    last.is_none_or(|at| !matches!(at.elapsed(), Ok(passed) if passed < PERIOD))
}

/// Update state of the app.
pub struct Updates {
    release: bool,
    inner: Mutex<Inner>,
}

struct Inner {
    status: UpdateStatus,
    /// The downloaded archive. The plugin checked its signature already.
    ready: Option<(Update, Vec<u8>)>,
}

impl Updates {
    pub fn new(release: bool) -> Self {
        let status = if release {
            UpdateStatus::Idle
        } else {
            UpdateStatus::Unavailable
        };
        Updates {
            release,
            inner: Mutex::new(Inner {
                status,
                ready: None,
            }),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.lock().status.clone()
    }

    /// Starts a check if one is needed. `false`: this is a developer build, a
    /// check or a download runs already, or the new version is on disk. The
    /// current status is then the answer, and the timer and the button do not
    /// download the same archive twice.
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

    /// Puts back an archive that did not install, so the button still works.
    fn put_back(&self, ready: (Update, Vec<u8>)) {
        self.lock().ready = Some(ready);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("состояние обновлений испорчено")
    }
}

/// Saves the status and sends it to both windows.
fn publish<R: Runtime>(app: &AppHandle<R>, status: UpdateStatus) -> UpdateStatus {
    app.state::<Updates>().set(status.clone());
    let _ = app.emit(EVENT, status.clone());
    status
}

/// One check: ask for `latest.json` and, if there is a new version, download
/// it. A failure only goes into the status: the status bar does not show it,
/// so a laptop without network does not get an error each few hours.
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
            eprintln!("не удалось проверить обновления: {error}");
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

/// Background checks while the setting is on.
///
/// A thread with a clock, as the sync scheduler. It compares wall time, not a
/// sleep of six hours: a sleeping laptop stops the monotonic clock, and a
/// laptop that sleeps each night would then almost never check.
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

/// Installs the downloaded version when the app quits.
///
/// Not on Windows: there the installer opens its own window, and a window
/// after "Quit" is a surprise.
pub fn install_on_exit<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(windows) {
        return;
    }
    if let Some((update, bytes)) = app.state::<Updates>().take() {
        if let Err(error) = update.install(bytes) {
            eprintln!("не удалось установить обновление: {error}");
        }
    }
}

#[tauri::command]
pub fn update_status(updates: State<'_, Updates>) -> UpdateStatus {
    updates.status()
}

/// "Check now" in the settings. It works also with the setting off: the
/// setting is about the background, not about the button.
#[tauri::command]
pub async fn check_update<R: Runtime>(app: AppHandle<R>) -> UpdateStatus {
    check(&app).await
}

/// "Restart" in the status bar. The window saves the open note before this.
///
/// The last sync runs first, as on close: on Windows `install` quits the app
/// at once, and the sync must not be lost.
#[tauri::command(async)]
pub fn restart_to_update<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    let updates = app.state::<Updates>();
    let Some(ready) = updates.take() else {
        return Err(Error::Update("новая версия ещё не скачана".into()));
    };

    if crate::sync_status::start_closing() {
        crate::sync_status::sync_on_close(&app, crate::CLOSE_WAIT);
    }
    if let Err(error) = ready.0.install(&ready.1) {
        updates.put_back(ready);
        return Err(Error::Update(error.to_string()));
    }
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_developer_build_does_not_check() {
        assert!(!is_release("0.0.0", false), "pnpm install:app");
        assert!(!is_release("0.0.0", true), "pnpm dev");
        assert!(!is_release("0.1.7", true), "отладочная сборка с версией");
        assert!(is_release("0.1.7", false), "сборка из релиза");
    }

    #[test]
    fn a_developer_build_says_so_and_never_starts() {
        let updates = Updates::new(false);

        assert_eq!(updates.status(), UpdateStatus::Unavailable);
        assert!(!updates.begin());
        assert_eq!(updates.status(), UpdateStatus::Unavailable);
    }

    #[test]
    fn a_second_check_waits_for_the_first() {
        let updates = Updates::new(true);

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
        let updates = Updates::new(true);
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
        let updates = Updates::new(true);

        updates.set(UpdateStatus::Failed {
            message: "нет сети".into(),
        });
        assert!(updates.begin());

        updates.set(UpdateStatus::Idle);
        assert!(updates.begin());
    }

    #[test]
    fn nothing_to_install_without_a_download() {
        assert!(Updates::new(true).take().is_none());
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

    /// The windows read `state` and `version`, so the field names are part
    /// of the contract with `api.ts`.
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
