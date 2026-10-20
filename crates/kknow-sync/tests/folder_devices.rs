//! The same "two devices" scenario, but the shared storage is a folder.
//!
//! Cloud drive services work this way: a shared directory instead of a
//! bucket. No network is needed, so unlike the real S3 test, this test is
//! not `#[ignore]`.

use std::fs;

use kknow_core::{NoteId, Vault};
use kknow_sync::{ConflictPolicy, FolderProvider, SyncEngine};

struct Device {
    dir: tempfile::TempDir,
    vault: Vault,
    engine: SyncEngine<FolderProvider>,
}

impl Device {
    fn new(cloud: &std::path::Path, policy: ConflictPolicy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        Device {
            dir,
            vault,
            engine: SyncEngine::new(FolderProvider::open(cloud).unwrap()).with_policy(policy),
        }
    }

    fn edit(&mut self, id: &str, content: &str) {
        let path = self.dir.path().join(format!("{id}.md"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
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
}

#[test]
fn two_devices_meet_in_a_shared_folder() {
    let cloud = tempfile::tempdir().unwrap();
    let mut first = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    let mut second = Device::new(cloud.path(), ConflictPolicy::KeepBoth);

    first.edit("Проекты/kknow", "# kknow\n\nпроект\n");
    assert_eq!(first.sync().uploaded.len(), 1);

    let report = second.sync();
    assert_eq!(report.downloaded.len(), 1);
    assert_eq!(second.read("Проекты/kknow"), "# kknow\n\nпроект\n");

    // The folder keeps the same keys as a bucket.
    assert!(cloud.path().join("manifest.json").is_file());
    assert!(cloud.path().join("notes/Проекты/kknow.md").is_file());

    // The second run does nothing: sync is idempotent.
    assert!(second.sync().is_empty());
}

#[test]
fn a_conflict_in_a_folder_keeps_both_versions() {
    let cloud = tempfile::tempdir().unwrap();
    let mut first = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    let mut second = Device::new(cloud.path(), ConflictPolicy::KeepBoth);

    first.edit("Заметка", "исходная\n");
    first.sync();
    second.sync();

    // Both edit the same note without knowing about each other.
    first.edit("Заметка", "правка первого\n");
    second.edit("Заметка", "правка второго\n");
    first.sync();
    let report = second.sync();

    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(second.read("Заметка"), "правка второго\n");
    let copy = report.conflicts[0].copy.as_str().to_string();
    assert!(second.has(&copy), "чужая версия легла копией рядом");
}

#[test]
fn deleting_on_one_device_removes_it_from_the_folder() {
    let cloud = tempfile::tempdir().unwrap();
    let mut first = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    let mut second = Device::new(cloud.path(), ConflictPolicy::KeepBoth);

    first.edit("Черновик", "выбросить\n");
    first.sync();
    second.sync();
    assert!(second.has("Черновик"));

    fs::remove_file(first.dir.path().join("Черновик.md")).unwrap();
    first.vault.rescan().unwrap();
    first.sync();

    let report = second.sync();
    assert_eq!(report.removed_local.len(), 1);
    assert!(!second.has("Черновик"));
    assert!(!cloud.path().join("notes/Черновик.md").exists());
}
