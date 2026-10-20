//! The same algorithm on real S3.
//!
//! The tests are `#[ignore]`: they cannot run without MinIO, and `cargo test`
//! must stay a command with no requirements.
//!
//! ```sh
//! docker compose up -d
//! cargo test -p kknow-sync --test minio -- --ignored
//! ```
//!
//! The address and keys come from the environment (`KKNOW_S3_*`). The defaults
//! match `docker-compose.yml` in the repository root.

#![cfg(feature = "s3")]

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use kknow_core::{NoteId, Vault};
use kknow_sync::{S3Config, S3Provider, SyncEngine, SyncProvider};

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// A separate prefix for each run: tests must not see notes of other tests,
/// and the bucket must survive a repeated run.
fn config(test: &str) -> S3Config {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("часы отстают от 1970 года")
        .as_nanos();

    S3Config {
        bucket: env_or("KKNOW_S3_BUCKET", "kknow"),
        endpoint: Some(env_or("KKNOW_S3_ENDPOINT", "http://localhost:9000")),
        region: Some(env_or("KKNOW_S3_REGION", "us-east-1")),
        root: Some(format!("tests/{test}-{stamp}/")),
        access_key_id: Some(env_or("KKNOW_S3_ACCESS_KEY", "kknowkknow")),
        secret_access_key: Some(env_or("KKNOW_S3_SECRET_KEY", "kknowkknow")),
    }
}

fn engine(config: &S3Config) -> SyncEngine<S3Provider> {
    SyncEngine::new(S3Provider::connect(config).expect("MinIO не отвечает — docker compose up -d"))
}

fn vault(notes: &[(&str, &str)]) -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    for (id, content) in notes {
        let path = dir.path().join(format!("{id}.md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let vault = Vault::open(dir.path()).unwrap();
    (dir, vault)
}

fn read(vault: &Vault, id: &str) -> String {
    vault.read(&NoteId::parse(id).unwrap()).unwrap()
}

#[test]
#[ignore = "needs MinIO: docker compose up -d"]
fn a_vault_travels_through_s3() {
    let config = config("travels");
    let sender = engine(&config);
    let receiver = engine(&config);

    let (_here, mut here) = vault(&[
        ("Index", "# Оглавление\n[[Projects/kknow]]"),
        ("Projects/kknow", "# kknow\n"),
    ]);
    let (_there, mut there) = vault(&[]);

    let sent = sender.run(&mut here).unwrap();
    let received = receiver.run(&mut there).unwrap();

    assert_eq!(sent.uploaded.len(), 2);
    assert_eq!(received.downloaded.len(), 2);
    assert_eq!(read(&there, "Projects/kknow"), "# kknow\n");

    // The bucket has the notes and the manifest, and nothing else.
    let mut keys = sender.remote_keys().unwrap();
    keys.sort();
    assert_eq!(keys, ["notes/Index.md", "notes/Projects/kknow.md"]);
    assert!(
        sender
            .provider()
            .get(kknow_sync::MANIFEST_KEY)
            .unwrap()
            .is_some()
    );

    // A repeated sync must not touch anything.
    assert!(sender.run(&mut here).unwrap().is_empty());
    assert!(receiver.run(&mut there).unwrap().is_empty());
}

#[test]
#[ignore = "needs MinIO: docker compose up -d"]
fn edits_and_deletes_reach_the_other_side() {
    let config = config("edits");
    let first = engine(&config);
    let second = engine(&config);

    let (here_dir, mut here) = vault(&[("Заметка", "первая версия"), ("Черновик", "выбросить")]);
    let (_there, mut there) = vault(&[]);
    first.run(&mut here).unwrap();
    second.run(&mut there).unwrap();

    fs::write(here_dir.path().join("Заметка.md"), "вторая версия").unwrap();
    fs::remove_file(here_dir.path().join("Черновик.md")).unwrap();
    here.rescan().unwrap();
    first.run(&mut here).unwrap();

    let report = second.run(&mut there).unwrap();

    assert_eq!(report.downloaded.len(), 1);
    assert_eq!(report.removed_local.len(), 1);
    assert_eq!(read(&there, "Заметка"), "вторая версия");
    assert!(!there.contains(&NoteId::parse("Черновик").unwrap()));
    assert_eq!(first.remote_keys().unwrap(), ["notes/Заметка.md"]);
}

#[test]
#[ignore = "needs MinIO: docker compose up -d"]
fn a_conflict_keeps_both_versions() {
    let config = config("conflict");
    let first = engine(&config);
    let second = engine(&config);

    let (here_dir, mut here) = vault(&[("Заметка", "общее начало")]);
    let (there_dir, mut there) = vault(&[]);
    first.run(&mut here).unwrap();
    second.run(&mut there).unwrap();

    fs::write(here_dir.path().join("Заметка.md"), "правка на первом").unwrap();
    fs::write(there_dir.path().join("Заметка.md"), "правка на втором").unwrap();
    here.rescan().unwrap();
    there.rescan().unwrap();

    first.run(&mut here).unwrap();
    let report = second.run(&mut there).unwrap();

    assert_eq!(report.conflicts.len(), 1);
    let copy = &report.conflicts[0].copy;
    there.rescan().unwrap();
    assert_eq!(read(&there, "Заметка"), "правка на втором");
    assert_eq!(there.read(copy).unwrap(), "правка на первом");

    // The first device gets both versions, and then everything is stable.
    first.run(&mut here).unwrap();
    here.rescan().unwrap();
    assert_eq!(read(&here, "Заметка"), "правка на втором");
    assert_eq!(here.read(copy).unwrap(), "правка на первом");
    assert!(first.run(&mut here).unwrap().is_empty());
    assert!(second.run(&mut there).unwrap().is_empty());
}

#[test]
#[ignore = "needs MinIO: docker compose up -d"]
fn the_provider_speaks_plain_objects() {
    let config = config("provider");
    let provider = S3Provider::connect(&config).expect("MinIO не отвечает");

    assert!(provider.get("notes/нет.md").unwrap().is_none());
    assert!(provider.delete("notes/нет.md").is_ok());

    provider
        .put("notes/Заметка.md", "текст".as_bytes())
        .unwrap();

    assert_eq!(
        provider.get("notes/Заметка.md").unwrap().unwrap(),
        "текст".as_bytes()
    );
    assert_eq!(provider.list("notes/").unwrap(), ["notes/Заметка.md"]);

    provider.delete("notes/Заметка.md").unwrap();

    assert!(provider.get("notes/Заметка.md").unwrap().is_none());
    assert!(provider.list("notes/").unwrap().is_empty());
}
