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

    assert!(cloud.path().join(".kknow/manifest.json").is_file());
    assert!(cloud.path().join("Проекты/kknow.md").is_file());

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
    assert!(!cloud.path().join("Черновик.md").exists());
}

#[test]
fn the_first_sync_joins_a_vault_and_a_storage_that_other_programs_filled() {
    let cloud = tempfile::tempdir().unwrap();
    fs::create_dir_all(cloud.path().join("research/.cache")).unwrap();
    fs::write(cloud.path().join("research/htmx.md"), "# htmx\n").unwrap();
    fs::write(cloud.path().join("research/data.json"), "{}").unwrap();
    fs::write(cloud.path().join("research/.cache/x.md"), "кэш").unwrap();

    let mut device = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    device.edit("Идеи", "# Идеи\n");

    let report = device.sync();
    assert_eq!(report.downloaded, [NoteId::parse("research/htmx").unwrap()]);
    assert_eq!(report.uploaded, [NoteId::parse("Идеи").unwrap()]);
    assert_eq!(device.read("research/htmx"), "# htmx\n");
    assert!(!device.has("research/.cache/x"));
    assert_eq!(
        fs::read_to_string(cloud.path().join("Идеи.md")).unwrap(),
        "# Идеи\n"
    );

    assert!(device.sync().is_empty(), "второй синк ничего не делает");
}

#[test]
fn a_file_deleted_in_the_storage_goes_to_the_trash() {
    let cloud = tempfile::tempdir().unwrap();
    fs::write(cloud.path().join("Черновик.md"), "черновик").unwrap();

    let mut device = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    device.sync();
    assert!(device.has("Черновик"));

    fs::remove_file(cloud.path().join("Черновик.md")).unwrap();
    let report = device.sync();
    assert_eq!(report.removed_local.len(), 1);
    assert!(!device.has("Черновик"));
}

#[test]
fn the_same_path_with_different_text_keeps_both() {
    let cloud = tempfile::tempdir().unwrap();
    fs::write(cloud.path().join("План.md"), "из облака").unwrap();

    let mut device = Device::new(cloud.path(), ConflictPolicy::KeepBoth);
    device.edit("План", "локальный");

    let report = device.sync();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(device.read("План"), "локальный");
    assert_eq!(device.read(report.conflicts[0].copy.as_str()), "из облака");
}
