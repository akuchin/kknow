//! Sync as the user sees it: two vaults and one shared storage.
//!
//! In-memory storage lets us test the whole algorithm without a network. The
//! same scenario on real S3 is in `minio.rs`.

use std::fs;
use std::path::Path;

use kknow_core::{NoteId, Vault};
use kknow_sync::{ConflictPolicy, MemoryProvider, SyncEngine};

struct Device {
    dir: tempfile::TempDir,
    vault: Vault,
    engine: SyncEngine<MemoryProvider>,
}

impl Device {
    fn new(cloud: &MemoryProvider) -> Self {
        Device::with_policy(cloud, ConflictPolicy::KeepBoth)
    }

    fn with_policy(cloud: &MemoryProvider, policy: ConflictPolicy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        Device {
            dir,
            vault,
            engine: SyncEngine::new(cloud.clone()).with_policy(policy),
        }
    }

    /// An edit outside the app, as in an external editor.
    fn edit(&mut self, id: &str, content: &str) {
        let path = self.dir.path().join(format!("{id}.md"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
        self.vault.rescan().unwrap();
    }

    fn delete(&mut self, id: &str) {
        fs::remove_file(self.dir.path().join(format!("{id}.md"))).unwrap();
        self.vault.rescan().unwrap();
    }

    fn sync(&mut self) -> kknow_sync::Report {
        let report = self.engine.run(&mut self.vault).unwrap();
        self.vault.rescan().unwrap();
        report
    }

    fn read(&self, id: &str) -> String {
        self.vault.read(&NoteId::parse(id).unwrap()).unwrap()
    }

    fn has(&self, id: &str) -> bool {
        self.vault.contains(&NoteId::parse(id).unwrap())
    }

    fn ids(&self) -> Vec<String> {
        self.vault.ids().map(|id| id.as_str().to_string()).collect()
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

#[test]
fn a_second_device_receives_the_whole_vault() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Index", "# Оглавление\n[[Projects/kknow]]");
    first.edit("Projects/kknow", "# kknow\n");
    let sent = first.sync();
    let received = second.sync();

    assert_eq!(sent.uploaded.len(), 2);
    assert_eq!(received.downloaded.len(), 2);
    assert_eq!(second.ids(), ["Index", "Projects/kknow"]);
    assert_eq!(second.read("Projects/kknow"), "# kknow\n");
}

#[test]
fn a_repeated_sync_does_nothing() {
    let cloud = MemoryProvider::new();
    let mut device = Device::new(&cloud);

    device.edit("Index", "# Оглавление");
    device.sync();

    assert!(device.sync().is_empty());
    assert!(device.sync().is_empty());
}

#[test]
fn an_edit_travels_to_the_other_device() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Index", "первая версия");
    first.sync();
    second.sync();

    first.edit("Index", "вторая версия");
    first.sync();
    let report = second.sync();

    assert_eq!(report.downloaded.len(), 1);
    assert_eq!(second.read("Index"), "вторая версия");
}

#[test]
fn a_delete_travels_too() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Черновик", "выбросить");
    first.sync();
    second.sync();
    assert!(second.has("Черновик"));

    first.delete("Черновик");
    let sent = first.sync();
    let received = second.sync();

    assert_eq!(sent.removed_remote.len(), 1);
    assert_eq!(received.removed_local.len(), 1);
    assert!(!second.has("Черновик"));
    // The delete can be a mistake, so the note waits in the trash.
    let trash = second.vault.trash().entries().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].path, "Черновик.md");
    assert!(cloud.keys().iter().all(|key| !key.contains("Черновик")));
}

#[test]
fn a_deleted_note_does_not_come_back() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Черновик", "выбросить");
    first.sync();
    second.sync();
    second.delete("Черновик");
    second.sync();

    // The first device still knows the note and must not put it back in the cloud.
    let report = first.sync();

    assert_eq!(report.removed_local.len(), 1);
    assert!(!first.has("Черновик"));
    assert!(first.sync().is_empty());
    assert!(second.sync().is_empty());
}

#[test]
fn both_sides_edited_keeps_both_versions() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Заметка", "общее начало");
    first.sync();
    second.sync();

    first.edit("Заметка", "правка на первом");
    second.edit("Заметка", "правка на втором");
    first.sync();
    let report = second.sync();

    assert_eq!(report.conflicts.len(), 1);
    let conflict = &report.conflicts[0];
    assert_eq!(conflict.id.as_str(), "Заметка");

    // The second device keeps its version, the other one goes next to it as a copy.
    assert_eq!(second.read("Заметка"), "правка на втором");
    assert_eq!(
        second.vault.read(&conflict.copy).unwrap(),
        "правка на первом"
    );

    // The first device gets both versions, and then everything is stable.
    let back = first.sync();
    assert_eq!(back.downloaded.len(), 2);
    assert_eq!(first.read("Заметка"), "правка на втором");
    assert_eq!(
        first.vault.read(&conflict.copy).unwrap(),
        "правка на первом"
    );
    assert!(first.sync().is_empty());
    assert!(second.sync().is_empty());
}

#[test]
fn a_conflict_settles_after_one_round() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Заметка", "общее начало");
    first.sync();
    second.sync();

    first.edit("Заметка", "правка на первом");
    second.edit("Заметка", "правка на втором");
    first.sync();
    second.sync();

    // After this, the devices must not make more copies for each other.
    for _ in 0..3 {
        first.sync();
        second.sync();
    }

    assert_eq!(first.ids().len(), 2, "{:?}", first.ids());
    assert_eq!(second.ids(), first.ids());
}

#[test]
fn a_chosen_side_wins_when_the_policy_says_so() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::with_policy(&cloud, ConflictPolicy::PreferLocal);

    first.edit("Заметка", "общее начало");
    first.sync();
    second.sync();

    first.edit("Заметка", "правка на первом");
    second.edit("Заметка", "правка на втором");
    first.sync();
    let report = second.sync();

    assert!(report.conflicts.is_empty());
    assert_eq!(report.uploaded.len(), 1);
    assert_eq!(second.ids(), ["Заметка"]);
    assert_eq!(first.sync().downloaded.len(), 1);
    assert_eq!(first.read("Заметка"), "правка на втором");
}

#[test]
fn sync_state_lives_in_a_hidden_folder() {
    let cloud = MemoryProvider::new();
    let mut device = Device::new(&cloud);

    device.edit("Index", "# Оглавление");
    device.sync();

    assert!(device.path().join(".kknow/sync.json").is_file());
    // The internal file must not become a note.
    assert_eq!(device.ids(), ["Index"]);
}

#[test]
fn a_lost_local_state_does_not_lose_notes() {
    let cloud = MemoryProvider::new();
    let mut device = Device::new(&cloud);

    device.edit("Index", "# Оглавление");
    device.sync();

    // The sync cache can be rebuilt: delete it and sync again.
    fs::remove_file(device.path().join(".kknow/sync.json")).unwrap();
    let report = device.sync();

    assert!(report.is_empty(), "{report:?}");
    assert!(device.has("Index"));
}

#[test]
fn a_concurrent_sync_is_refused_instead_of_overwriting() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    let mut second = Device::new(&cloud);

    first.edit("Первая", "текст");
    second.edit("Вторая", "текст");

    // Both devices prepared a sync from the same manifest.
    let stale = second.engine.stage(&second.vault).unwrap();
    first.sync();

    let error = second
        .engine
        .transfer(stale, &mut |_| {})
        .expect_err("устаревший манифест должен быть отвергнут");

    assert!(matches!(error, kknow_sync::Error::RemoteChanged), "{error}");
    // After a retry, both devices have both notes.
    second.sync();
    first.sync();
    assert_eq!(first.ids(), ["Вторая", "Первая"]);
    assert_eq!(second.ids(), ["Вторая", "Первая"]);
}

/// Sync has three parts: stage (vault locked), transfer (network, no vault),
/// and local commit. Check that progress counts actions and that the transfer
/// does not need the directory.
#[test]
fn a_split_exchange_reports_progress_and_needs_no_vault_while_transferring() {
    let cloud = MemoryProvider::new();
    let mut device = Device::new(&cloud);

    device.edit("Первая", "текст");
    device.edit("Вторая", "текст");
    device.edit("Третья", "текст");

    let staged = device.engine.stage(&device.vault).unwrap();
    assert_eq!(staged.len(), 3);

    let mut seen = Vec::new();
    // The transfer gets only the staged sync, not the directory.
    let fetched = device
        .engine
        .transfer(staged, &mut |progress| seen.push(progress.done))
        .unwrap();

    assert_eq!(seen, [0, 1, 2, 3], "прогресс идёт по возрастанию до конца");
    assert!(seen.iter().all(|done| *done <= 3));

    let report = device
        .engine
        .commit_local(&mut device.vault, fetched)
        .unwrap();
    assert_eq!(report.uploaded.len(), 3);
    assert!(device.engine.stage(&device.vault).unwrap().is_empty());
}

/// Notes from the storage are written only in the third part: before it,
/// they are not on disk.
#[test]
fn downloads_land_on_disk_only_when_the_exchange_is_committed() {
    let cloud = MemoryProvider::new();
    let mut first = Device::new(&cloud);
    first.edit("Заметка", "из первого\n");
    first.sync();

    let mut second = Device::new(&cloud);
    let staged = second.engine.stage(&second.vault).unwrap();
    let fetched = second.engine.transfer(staged, &mut |_| {}).unwrap();

    assert!(!second.has("Заметка"), "передача на диск ещё не писала");

    second
        .engine
        .commit_local(&mut second.vault, fetched)
        .unwrap();
    second.vault.rescan().unwrap();
    assert_eq!(second.read("Заметка"), "из первого\n");
}
