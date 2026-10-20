//! End-to-end check for stage 3: a file edit on disk reaches search by
//! itself, without a full vault reload.

use std::fs;
use std::time::{Duration, Instant};

use kknow_app::{Changes, Workspace};
use kknow_core::NoteId;

/// Waits for the watcher to deliver changes and applies them.
fn settle(workspace: &mut Workspace, watcher: &kknow_app::watch::VaultWatcher) -> Changes {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut changes = Changes::default();

    while Instant::now() < deadline {
        if let Some(events) = watcher.recv_timeout(Duration::from_millis(200)) {
            let batch = workspace.apply(&events).expect("изменения применены");
            changes.updated.extend(batch.updated);
            changes.removed.extend(batch.removed);
            // One file edit causes several notifications; let them settle.
            std::thread::sleep(Duration::from_millis(300));
            if let Some(tail) = watcher.recv_timeout(Duration::from_millis(200)) {
                let batch = workspace.apply(&tail).expect("изменения применены");
                changes.updated.extend(batch.updated);
                changes.removed.extend(batch.removed);
            }
            break;
        }
    }

    changes
}

fn found(workspace: &Workspace, query: &str) -> Vec<String> {
    workspace
        .search(query, 10)
        .expect("поиск отработал")
        .into_iter()
        .map(|hit| hit.id.as_str().to_string())
        .collect()
}

#[test]
fn external_edit_reaches_search_and_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();

    let mut workspace = Workspace::open(dir.path()).unwrap();
    let watcher = workspace.watch().unwrap();

    // A note that a different editor creates.
    fs::write(
        dir.path().join("Извне.md"),
        "# Извне\n\nПишу про полнотекстовый поиск и ссылаюсь на [[Index]].\n",
    )
    .unwrap();

    let changes = settle(&mut workspace, &watcher);
    let created = NoteId::parse("Извне").unwrap();

    assert!(
        changes.updated.contains(&created),
        "watcher не заметил новый файл: {changes:?}"
    );
    assert_eq!(found(&workspace, "полнотекстовый"), ["Извне"]);
    assert_eq!(
        workspace.backlinks(&NoteId::parse("Index").unwrap()).len(),
        1,
        "ссылка из новой заметки должна появиться в графе"
    );

    // Now the file is deleted outside the app.
    fs::remove_file(dir.path().join("Извне.md")).unwrap();
    let changes = settle(&mut workspace, &watcher);

    assert!(
        changes.removed.contains(&created),
        "watcher не заметил удаление: {changes:?}"
    );
    assert!(found(&workspace, "полнотекстовый").is_empty());
    assert!(
        workspace
            .backlinks(&NoteId::parse("Index").unwrap())
            .is_empty()
    );
}
