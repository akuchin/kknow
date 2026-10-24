//! Sync status of each vault and the auto-sync schedule.
//!
//! Each vault syncs separately and does not block the others, so each has its
//! own status: off, idle, syncing (N of M), conflicts, error. The window shows
//! it in the status bar and never opens a modal dialog for it. Another vault
//! only gets a chip.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::{AppState, VaultSlot};

/// Event name for the frontend.
pub const EVENT: &str = "sync-status";

/// How often the scheduler wakes up to check if a sync is due.
const TICK: Duration = Duration::from_secs(1);

/// The maximum number of times the pause doubles after failures.
const BACKOFF_LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    /// Sync is not configured.
    Off,
    /// Configured and not active now.
    Idle,
    Syncing,
    /// Sync finished, but the folder has conflict copies.
    Conflict,
    /// The last attempt failed.
    Error,
}

/// What to show for one vault.
///
/// The whole struct is comparable. Windows redraw on each event, so sending an
/// unchanged status makes them redraw for nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub vault: String,
    pub state: SyncState,
    /// Sync progress: how many actions of the total are done.
    pub done: usize,
    pub total: usize,
    /// End of the last successful sync, in ms since the epoch. The window
    /// formats the time ("2 min ago") because the format depends on the
    /// language.
    pub at: Option<u64>,
    /// What went wrong: a code with values, like all other errors.
    pub message: Option<crate::errors::ErrorView>,
    /// Number of conflict copies in the folder.
    pub conflicts: usize,
    /// Sync target: a bucket or a folder.
    pub location: Option<String>,
    /// Auto-sync interval in minutes; `0` means manual only.
    pub auto_sync_minutes: u32,
}

impl SyncStatus {
    fn new(vault: &str) -> Self {
        SyncStatus {
            vault: vault.to_string(),
            state: SyncState::Off,
            done: 0,
            total: 0,
            at: None,
            message: None,
            conflicts: 0,
            location: None,
            auto_sync_minutes: 0,
        }
    }
}

/// Statuses of all vaults. They live apart from `AppState`: that one is about
/// folders, this one is about sync, and they do not need each other.
#[derive(Default)]
pub struct SyncStatuses {
    inner: Mutex<HashMap<String, SyncStatus>>,
}

impl SyncStatuses {
    /// Status of one vault. In the app, the `sync_status` command returns it
    /// and also recalculates what is visible without a sync. Tests need direct
    /// reads.
    #[cfg(test)]
    pub fn get(&self, vault: &str) -> SyncStatus {
        self.lock()
            .get(vault)
            .cloned()
            .unwrap_or_else(|| SyncStatus::new(vault))
    }

    pub fn all(&self) -> Vec<SyncStatus> {
        let mut all: Vec<SyncStatus> = self.lock().values().cloned().collect();
        all.sort_by(|a, b| a.vault.cmp(&b.vault));
        all
    }

    /// Changes the status and sends it to the windows if it changed.
    ///
    /// Silence on no change is required, not an optimisation. On each event the
    /// settings window reads the statuses again with `sync_statuses`, and that
    /// command recalculates them. A "nothing changed" event closes the loop,
    /// and the vaults section redraws without end.
    pub fn update<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        vault: &str,
        change: impl FnOnce(&mut SyncStatus),
    ) -> SyncStatus {
        let mut guard = self.lock();
        let status = guard
            .entry(vault.to_string())
            .or_insert_with(|| SyncStatus::new(vault));
        let before = status.clone();
        change(status);

        let updated = status.clone();
        drop(guard);

        if updated != before {
            let _ = app.emit(EVENT, updated.clone());
        }
        updated
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, SyncStatus>> {
        self.inner.lock().expect("состояние обмена испорчено")
    }
}

/// Current time in ms since the epoch, the format `Date` in the window uses.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

/// Decides when to sync automatically.
///
/// It is separate from the thread, so tests check the rule without real-time
/// waits. After a failure the pause doubles: if the network is down, there is
/// no point to retry each minute. A successful sync resets the counter.
#[derive(Debug)]
pub struct Cadence {
    interval: Duration,
    failures: u32,
    last: Option<Instant>,
}

impl Cadence {
    pub fn new(interval: Duration) -> Self {
        Cadence {
            interval,
            failures: 0,
            last: None,
        }
    }

    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// How long to wait until the next attempt.
    pub fn delay(&self) -> Duration {
        self.interval * 2u32.pow(self.failures.min(BACKOFF_LIMIT))
    }

    pub fn due(&self, now: Instant) -> bool {
        match self.last {
            // The first sync is not delayed: the app just opened, and notes
            // must be fresh at once.
            None => true,
            Some(last) => now.duration_since(last) >= self.delay(),
        }
    }

    pub fn succeeded(&mut self, now: Instant) {
        self.failures = 0;
        self.last = Some(now);
    }

    pub fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        self.last = Some(now);
    }
}

/// Starts auto-sync for one vault.
///
/// The thread lives while the vault is open. On each tick it reads the setting
/// again: the interval can change, and a thread restart is not necessary. A
/// tick is skipped if a sync is already running.
pub fn schedule<R: Runtime>(app: AppHandle<R>, slot: Arc<VaultSlot>) {
    std::thread::spawn(move || {
        let mut cadence = Cadence::new(Duration::ZERO);

        while !slot.is_closed() {
            std::thread::sleep(TICK);
            if slot.is_closed() {
                break;
            }

            let minutes = crate::sync::auto_sync_minutes(&slot);
            if minutes == 0 {
                continue;
            }
            cadence.set_interval(Duration::from_secs(u64::from(minutes) * 60));

            if !cadence.due(Instant::now()) {
                continue;
            }

            match crate::sync::exchange(&app, &slot) {
                Ok(_) => cadence.succeeded(Instant::now()),
                // The status bar already shows the error. Here we only make
                // the next attempt later.
                Err(_) => cadence.failed(Instant::now()),
            }
        }
    });
}

/// The last sync before the window closes.
///
/// The wait is short. Sync is idempotent, so the next start finishes an
/// incomplete one. The user must not look at a frozen window.
pub fn sync_on_close<R: Runtime>(app: &AppHandle<R>, wait: Duration) {
    let state = app.state::<AppState>();
    let slots: Vec<Arc<VaultSlot>> = state
        .entries()
        .iter()
        .filter_map(|entry| state.opened(&entry.id))
        .filter(|slot| crate::sync::auto_sync_minutes(slot) > 0)
        .collect();

    if slots.is_empty() {
        return;
    }

    let (done, wait_for) = std::sync::mpsc::channel();
    for slot in slots {
        let app = app.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            let _ = crate::sync::exchange(&app, &slot);
            let _ = done.send(());
        });
    }
    drop(done);

    let deadline = Instant::now() + wait;
    while Instant::now() < deadline && wait_for.recv_timeout(TICK).is_ok() {}
}

/// The window is already closing: do not start the sync a second time.
pub static CLOSING: AtomicBool = AtomicBool::new(false);

pub fn start_closing() -> bool {
    !CLOSING.swap(true, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recalculation with no change must not wake the windows. They read
    /// the statuses again in response, and the event comes back.
    #[test]
    fn an_unchanged_status_is_not_broadcast() {
        use std::sync::atomic::AtomicUsize;
        use tauri::Listener;
        use tauri::test::{mock_builder, mock_context, noop_assets};

        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let heard = Arc::new(AtomicUsize::new(0));
        let counter = heard.clone();
        app.listen(EVENT, move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let statuses = SyncStatuses::default();
        let handle = app.handle().clone();

        statuses.update(&handle, "vault", |status| status.state = SyncState::Idle);
        assert_eq!(heard.load(Ordering::SeqCst), 1, "новое состояние разослано");

        statuses.update(&handle, "vault", |status| status.state = SyncState::Idle);
        assert_eq!(
            heard.load(Ordering::SeqCst),
            1,
            "то же самое состояние второй раз не рассылается"
        );

        statuses.update(&handle, "vault", |status| status.conflicts = 1);
        assert_eq!(heard.load(Ordering::SeqCst), 2, "изменение снова разослано");
    }

    #[test]
    fn the_first_tick_happens_at_once() {
        let cadence = Cadence::new(Duration::from_secs(300));

        assert!(cadence.due(Instant::now()), "первый обмен не откладывается");
    }

    #[test]
    fn a_successful_exchange_waits_out_the_interval() {
        let mut cadence = Cadence::new(Duration::from_secs(60));
        let start = Instant::now();
        cadence.succeeded(start);

        assert!(!cadence.due(start + Duration::from_secs(59)));
        assert!(cadence.due(start + Duration::from_secs(60)));
    }

    #[test]
    fn failures_stretch_the_pause_but_not_forever() {
        let mut cadence = Cadence::new(Duration::from_secs(60));
        let start = Instant::now();

        cadence.failed(start);
        assert_eq!(cadence.delay(), Duration::from_secs(120));
        cadence.failed(start);
        assert_eq!(cadence.delay(), Duration::from_secs(240));

        for _ in 0..10 {
            cadence.failed(start);
        }
        assert_eq!(
            cadence.delay(),
            Duration::from_secs(60 * 8),
            "пауза упирается в потолок, а не растёт до суток"
        );

        // A successful sync restores the normal interval.
        cadence.succeeded(start);
        assert_eq!(cadence.delay(), Duration::from_secs(60));
    }
}
