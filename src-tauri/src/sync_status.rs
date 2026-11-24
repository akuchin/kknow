use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::{AppState, VaultSlot};

pub const EVENT: &str = "sync-status";

const SECONDS_PER_MINUTE: u64 = 60;

const TICK: Duration = Duration::from_secs(1);

const IDLE_POLL: Duration = Duration::from_millis(50);

const BACKOFF_LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SyncState {
    Off,
    Idle,
    Syncing,
    Conflict,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncStatus {
    pub vault: String,
    pub state: SyncState,
    pub done: usize,
    pub total: usize,
    pub at: Option<u64>,
    pub message: Option<crate::errors::ErrorView>,
    pub conflicts: usize,
    pub location: Option<String>,
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

#[derive(Default)]
pub struct SyncStatuses {
    inner: Mutex<HashMap<String, SyncStatus>>,
}

impl SyncStatuses {
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
        self.inner.lock().expect("the sync status is poisoned")
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

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

    pub fn delay(&self) -> Duration {
        self.interval * 2u32.pow(self.failures.min(BACKOFF_LIMIT))
    }

    pub fn due(&self, now: Instant) -> bool {
        match self.last {
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
            cadence.set_interval(Duration::from_secs(u64::from(minutes) * SECONDS_PER_MINUTE));

            if !cadence.due(Instant::now()) {
                continue;
            }

            match crate::sync::exchange(&app, &slot) {
                Ok(_) => cadence.succeeded(Instant::now()),
                Err(crate::state::Error::SyncBusy) => {}
                Err(_) => cadence.failed(Instant::now()),
            }
        }
    });
}

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

    let deadline = Instant::now() + wait;
    let started = slots.len();
    let (done, finished) = std::sync::mpsc::channel();
    for slot in slots {
        let app = app.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            sync_or_wait(&app, &slot, deadline);
            let _ = done.send(());
        });
    }
    drop(done);

    wait_for_syncs(&finished, started, deadline);
}

fn sync_or_wait<R: Runtime>(app: &AppHandle<R>, slot: &Arc<VaultSlot>, deadline: Instant) {
    if slot.is_syncing() {
        wait_until_idle(slot, deadline);
        return;
    }
    if let Err(crate::state::Error::SyncBusy) = crate::sync::exchange(app, slot) {
        wait_until_idle(slot, deadline);
    }
}

fn wait_until_idle(slot: &VaultSlot, deadline: Instant) {
    while slot.is_syncing() && Instant::now() < deadline {
        std::thread::sleep(IDLE_POLL);
    }
}

pub fn wait_for_syncs(finished: &Receiver<()>, started: usize, deadline: Instant) -> usize {
    let mut count = 0;
    while count < started {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match finished.recv_timeout(left) {
            Ok(()) => count += 1,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    count
}

#[derive(Default)]
pub struct CloseLatch {
    flag: AtomicBool,
}

impl CloseLatch {
    pub const fn new() -> Self {
        CloseLatch {
            flag: AtomicBool::new(false),
        }
    }

    pub fn start(&self) -> bool {
        !self.flag.swap(true, Ordering::SeqCst)
    }

    pub fn reopen(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

pub static CLOSING: CloseLatch = CloseLatch::new();

#[cfg(test)]
mod tests {
    use super::*;

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

        cadence.succeeded(start);
        assert_eq!(cadence.delay(), Duration::from_secs(60));
    }

    #[test]
    fn the_close_latch_starts_once_until_it_is_reopened() {
        let latch = CloseLatch::new();

        assert!(latch.start());
        assert!(!latch.start());
        latch.reopen();
        assert!(latch.start());
    }

    fn sender_after(delay: Duration, done: std::sync::mpsc::Sender<()>) {
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = done.send(());
        });
    }

    #[test]
    fn the_exit_wait_goes_past_one_tick() {
        let (done, finished) = std::sync::mpsc::channel();
        sender_after(Duration::from_millis(1500), done);
        let deadline = Instant::now() + Duration::from_secs(3);

        assert_eq!(wait_for_syncs(&finished, 1, deadline), 1);
    }

    #[test]
    fn the_exit_wait_ends_when_all_syncs_are_done() {
        let (done, finished) = std::sync::mpsc::channel();
        sender_after(Duration::ZERO, done.clone());
        sender_after(Duration::ZERO, done);
        let started = Instant::now();

        assert_eq!(
            wait_for_syncs(&finished, 2, started + Duration::from_secs(5)),
            2
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn the_exit_wait_stops_at_the_deadline() {
        let (_done, finished) = std::sync::mpsc::channel::<()>();
        let started = Instant::now();

        let count = wait_for_syncs(&finished, 1, started + Duration::from_millis(300));

        assert_eq!(count, 0);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
