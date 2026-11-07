//! An encrypted storage: a [`SyncProvider`] on top of another one.
//!
//! The engine sees logical keys (`notes/a.md`, `.kknow/manifest.json`) and
//! plaintext bytes, as with any storage. Under them, [`Encrypted`] keeps
//! encrypted names and encrypted bytes (see [`crate::crypto`]). So the sync
//! algorithm does not know about encryption at all.
//!
//! The storage has two plaintext objects:
//!
//! - `.kknow/key.json`: the wrapped data key ([`KeyFile`]);
//! - `.kknow/manifest.json`: a decoy. An older build reads it, sees a
//!   manifest format that it does not know, and stops. Without the decoy it
//!   would see no manifest and upload all notes as plaintext. The real
//!   manifest has an encrypted name, so the two do not collide.
//!
//! A person or an agent can still drop a plaintext note into the storage.
//! [`SyncProvider::prepare`] encrypts it before the sync, and the sync then
//! sees it as a new remote note.

use crate::crypto::{self, Cipher, DataKey, KEY_FILE, KeyFile, RecoveryCode};
use crate::engine::{MANIFEST_KEY, note_of_key};
use crate::provider::{self, Operation, SyncProvider};
use crate::{Error, Result, shared};

/// The plaintext object at [`MANIFEST_KEY`] in an encrypted storage.
///
/// Format 2 is above the format of the real manifest, so an older build
/// refuses it with "manifest of version 2".
pub const DECOY: &[u8] = b"{\n  \"format\": 2,\n  \"encrypted\": true,\n  \"notes\": {}\n}\n";

/// True for the manifest of an encrypted storage, as a build without the
/// key sees it.
pub fn is_decoy(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .is_ok_and(|value| value.get("encrypted") == Some(&serde_json::Value::Bool(true)))
}

/// A storage whose names and contents are encrypted with one data key.
pub struct Encrypted<P> {
    inner: P,
    cipher: Cipher,
    key_check: Vec<u8>,
}

impl<P: SyncProvider> Encrypted<P> {
    pub fn new(inner: P, key: &DataKey) -> Self {
        Encrypted {
            inner,
            cipher: Cipher::new(key),
            key_check: crypto::key_check(key),
        }
    }

    /// The storage under the encryption, as other programs see it.
    pub fn inner(&self) -> &P {
        &self.inner
    }

    fn stored(&self, operation: Operation, key: &str) -> provider::Result<String> {
        self.cipher
            .encrypt_name(key)
            .map_err(|error| provider::Error::new(operation, key, error))
    }

    /// Encrypts a plaintext object and removes the plaintext.
    ///
    /// The encrypted copy goes first. If the sync stops between the two
    /// writes, the plaintext stays, and the next sync does the same work again.
    fn adopt(&self, key: &str) -> Result<()> {
        if let Some(bytes) = self.inner.get(key)? {
            self.put(key, &bytes)?;
        }
        self.inner.delete(key)?;
        Ok(())
    }

    /// Makes sure the plaintext manifest is the decoy.
    ///
    /// No manifest: a new storage, or a person deleted the decoy. A real
    /// plaintext manifest and no encrypted one: `init` stopped after it wrote
    /// the key file, so the manifest is moved under encryption now. Both: a
    /// device without encryption wrote into this storage, and the two
    /// manifests disagree. This needs a person.
    fn guard_manifest(&self, plain_keys: &[String]) -> Result<()> {
        let plain = if plain_keys.iter().any(|key| key == MANIFEST_KEY) {
            self.inner.get(MANIFEST_KEY)?
        } else {
            None
        };
        match plain {
            Some(bytes) if is_decoy(&bytes) => Ok(()),
            Some(bytes) => {
                if self.get(MANIFEST_KEY)?.is_some() {
                    return Err(Error::MixedStorage);
                }
                self.put(MANIFEST_KEY, &bytes)?;
                Ok(self.inner.put(MANIFEST_KEY, DECOY)?)
            }
            None => Ok(self.inner.put(MANIFEST_KEY, DECOY)?),
        }
    }
}

impl<P: SyncProvider> SyncProvider for Encrypted<P> {
    fn get(&self, key: &str) -> provider::Result<Option<Vec<u8>>> {
        let stored = self.stored(Operation::Get, key)?;
        let Some(bytes) = self.inner.get(&stored)? else {
            return Ok(None);
        };
        self.cipher
            .open(key, &bytes)
            .map(Some)
            .map_err(|error| provider::Error::new(Operation::Get, key, error))
    }

    fn put(&self, key: &str, bytes: &[u8]) -> provider::Result<()> {
        let stored = self.stored(Operation::Put, key)?;
        let sealed = self
            .cipher
            .seal(key, bytes)
            .map_err(|error| provider::Error::new(Operation::Put, key, error))?;
        self.inner.put(&stored, &sealed)
    }

    fn delete(&self, key: &str) -> provider::Result<()> {
        let stored = self.stored(Operation::Delete, key)?;
        self.inner.delete(&stored)
    }

    /// Only the objects that this key encrypted. Plaintext objects are not
    /// listed: [`SyncProvider::prepare`] encrypts them before each sync.
    fn list(&self, prefix: &str) -> provider::Result<Vec<String>> {
        let mut keys: Vec<String> = self
            .inner
            .list(crypto::NAME_PREFIX)?
            .iter()
            .filter_map(|stored| self.cipher.decrypt_name(stored))
            .filter(|key| key.starts_with(prefix))
            .collect();
        keys.sort();
        Ok(keys)
    }

    /// Keeps the decoy in place and encrypts plaintext notes and shared
    /// files. Gives their logical keys: the remote manifest may have an old
    /// hash for them.
    fn prepare(&self) -> Result<Vec<String>> {
        let file = key_file(&self.inner).map_err(|error| match error {
            Error::NotEncrypted => Error::WrongKey,
            other => other,
        })?;
        if !file.matches(&self.key_check) {
            return Err(Error::WrongKey);
        }

        let plain: Vec<String> = self
            .inner
            .list("")?
            .into_iter()
            .filter(|key| !key.starts_with(crypto::NAME_PREFIX))
            .collect();
        self.guard_manifest(&plain)?;

        let mut adopted = Vec::new();
        for key in plain {
            if note_of_key(&key).is_some() || shared::is_shared(&key) {
                self.adopt(&key)?;
                adopted.push(key);
            }
        }
        Ok(adopted)
    }
}

impl<P> std::fmt::Debug for Encrypted<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encrypted").finish_non_exhaustive()
    }
}

/// True if the storage has a key file.
pub fn is_encrypted(storage: &dyn SyncProvider) -> Result<bool> {
    Ok(storage.get(KEY_FILE)?.is_some())
}

/// Turns encryption on for a storage: a new key, then all objects that are
/// there go under encryption.
///
/// The key file goes first. From that moment, a device without the key
/// refuses the storage, so no new plaintext arrives during the work. If the
/// work stops, the next sync with the key finishes it ([`SyncProvider::prepare`]).
pub fn init(storage: &dyn SyncProvider, passphrase: &str) -> Result<(DataKey, RecoveryCode)> {
    let (file, key, code) = KeyFile::create(passphrase)?;
    init_with(storage, file, &key)?;
    Ok((key, code))
}

fn init_with(storage: &dyn SyncProvider, file: KeyFile, key: &DataKey) -> Result<()> {
    if is_encrypted(storage)? {
        return Err(Error::AlreadyEncrypted);
    }
    storage.put(KEY_FILE, &file.to_json())?;
    Encrypted::new(storage, key).prepare()?;
    Ok(())
}

/// The data key of the storage from the passphrase or the recovery code.
pub fn unlock(storage: &dyn SyncProvider, secret: &str) -> Result<DataKey> {
    key_file(storage)?.unlock(secret)
}

/// A new passphrase. `secret` is the current passphrase or the recovery code.
pub fn change_passphrase(storage: &dyn SyncProvider, secret: &str, new: &str) -> Result<DataKey> {
    let mut file = key_file(storage)?;
    let key = file.unlock(secret)?;
    file.change_passphrase(&key, new)?;
    storage.put(KEY_FILE, &file.to_json())?;
    Ok(key)
}

fn key_file(storage: &(impl SyncProvider + ?Sized)) -> Result<KeyFile> {
    let bytes = storage.get(KEY_FILE)?.ok_or(Error::NotEncrypted)?;
    KeyFile::from_json(&bytes)
}

#[cfg(test)]
mod tests {
    use kknow_core::{NoteId, Vault};

    use super::*;
    use crate::crypto::KdfParams;
    use crate::{MemoryProvider, SyncEngine};

    const WEAK: KdfParams = KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };
    const PASSPHRASE: &str = "correct horse";

    fn id(path: &str) -> NoteId {
        NoteId::parse(path).unwrap()
    }

    /// A storage with encryption on, and its key.
    fn encrypted_storage() -> (MemoryProvider, DataKey) {
        let storage = MemoryProvider::new();
        let (file, key, _) = KeyFile::create_with(PASSPHRASE, WEAK).unwrap();
        init_with(&storage, file, &key).unwrap();
        (storage, key)
    }

    fn device(files: &[(&str, &str)]) -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    fn engine(storage: &MemoryProvider, key: &DataKey) -> SyncEngine<Encrypted<MemoryProvider>> {
        SyncEngine::new(Encrypted::new(storage.clone(), key))
    }

    /// Each object of the storage as text, for checks that nothing leaks.
    fn everything(storage: &MemoryProvider) -> String {
        storage
            .keys()
            .into_iter()
            .map(|key| {
                let bytes = storage.get(&key).unwrap().unwrap();
                format!("{key}\n{}\n", String::from_utf8_lossy(&bytes))
            })
            .collect()
    }

    #[test]
    fn two_devices_sync_through_an_encrypted_storage() {
        let (storage, key) = encrypted_storage();
        let (_a, mut laptop) = device(&[("Health/Diagnosis.md", "# Secret\n\nflu")]);
        let (_b, mut desktop) = device(&[]);

        engine(&storage, &key).run(&mut laptop).unwrap();
        let report = engine(&storage, &key).run(&mut desktop).unwrap();

        assert_eq!(report.downloaded, [id("Health/Diagnosis")]);
        assert_eq!(
            desktop.read(&id("Health/Diagnosis")).unwrap(),
            "# Secret\n\nflu"
        );
        assert!(engine(&storage, &key).run(&mut laptop).unwrap().is_empty());
    }

    #[test]
    fn the_storage_shows_no_name_and_no_text() {
        let (storage, key) = encrypted_storage();
        let (_dir, mut vault) = device(&[
            ("Health/Diagnosis.md", "# Secret\n\nflu"),
            (".kknow/bookmarks.json", "{\"Health/Diagnosis\": []}"),
        ]);

        engine(&storage, &key).run(&mut vault).unwrap();

        let all = everything(&storage);
        for secret in ["Health", "Diagnosis", "Secret", "flu", "bookmarks"] {
            assert!(!all.contains(secret), "{secret} leaks:\n{all}");
        }
        let plain: Vec<String> = storage
            .keys()
            .into_iter()
            .filter(|key| !key.starts_with(crypto::NAME_PREFIX))
            .collect();
        assert_eq!(plain, [KEY_FILE, MANIFEST_KEY]);
    }

    #[test]
    fn a_plaintext_note_in_the_storage_is_imported_and_encrypted() {
        let (storage, key) = encrypted_storage();
        let (_a, mut laptop) = device(&[("Index.md", "# Index")]);
        let (_b, mut desktop) = device(&[]);
        engine(&storage, &key).run(&mut laptop).unwrap();

        // An agent drops its research next to the notes.
        storage.put("Research/Agent.md", b"# Found it").unwrap();

        let report = engine(&storage, &key).run(&mut laptop).unwrap();
        assert_eq!(report.downloaded, [id("Research/Agent")]);
        assert!(storage.get("Research/Agent.md").unwrap().is_none());
        assert!(!everything(&storage).contains("Found it"));

        engine(&storage, &key).run(&mut desktop).unwrap();
        assert_eq!(desktop.read(&id("Research/Agent")).unwrap(), "# Found it");
    }

    #[test]
    fn a_plaintext_edit_of_a_known_note_is_a_remote_change() {
        let (storage, key) = encrypted_storage();
        let (_a, mut vault) = device(&[("Index.md", "# Index")]);
        engine(&storage, &key).run(&mut vault).unwrap();

        storage
            .put("Index.md", b"# Index\n\nfrom the agent")
            .unwrap();

        let report = engine(&storage, &key).run(&mut vault).unwrap();
        assert_eq!(report.downloaded, [id("Index")]);
        assert_eq!(
            vault.read(&id("Index")).unwrap(),
            "# Index\n\nfrom the agent"
        );
    }

    #[test]
    fn a_plaintext_shared_file_is_imported_too() {
        let (storage, key) = encrypted_storage();
        let (dir, mut vault) = device(&[("Index.md", "# Index")]);
        engine(&storage, &key).run(&mut vault).unwrap();

        storage.put(".kknow/threads/t1.json", b"{}").unwrap();

        engine(&storage, &key).run(&mut vault).unwrap();
        assert!(storage.get(".kknow/threads/t1.json").unwrap().is_none());
        assert!(dir.path().join(".kknow/threads/t1.json").exists());
    }

    #[test]
    fn init_encrypts_a_full_plaintext_storage() {
        let storage = MemoryProvider::new();
        let (_a, mut laptop) = device(&[("A.md", "# alpha"), ("B/C.md", "# gamma")]);
        SyncEngine::new(storage.clone()).run(&mut laptop).unwrap();

        let (file, key, _) = KeyFile::create_with(PASSPHRASE, WEAK).unwrap();
        init_with(&storage, file, &key).unwrap();

        let all = everything(&storage);
        assert!(!all.contains("alpha") && !all.contains("gamma"), "{all}");
        // The manifest moved under encryption with its hashes, so the laptop
        // has nothing to do.
        assert!(engine(&storage, &key).run(&mut laptop).unwrap().is_empty());

        let (_b, mut desktop) = device(&[]);
        engine(&storage, &key).run(&mut desktop).unwrap();
        assert_eq!(desktop.read(&id("B/C")).unwrap(), "# gamma");
    }

    #[test]
    fn a_folder_storage_works_with_long_encrypted_names() {
        let remote = tempfile::tempdir().unwrap();
        let folder = crate::FolderProvider::open(remote.path()).unwrap();
        let (file, key, _) = KeyFile::create_with(PASSPHRASE, WEAK).unwrap();
        init_with(&folder, file, &key).unwrap();

        let deep = format!("{}Note.md", "Folder name/".repeat(20));
        let (_a, mut laptop) = device(&[(deep.as_str(), "# deep")]);
        let (_b, mut desktop) = device(&[]);
        let sync = |vault: &mut Vault| {
            let folder = crate::FolderProvider::open(remote.path()).unwrap();
            SyncEngine::new(Encrypted::new(folder, &key))
                .run(vault)
                .unwrap()
        };
        sync(&mut laptop);
        sync(&mut desktop);

        let id = id(deep.strip_suffix(".md").unwrap());
        assert_eq!(desktop.read(&id).unwrap(), "# deep");
        for entry in walkdir(remote.path()) {
            let name = entry.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.len() <= 255 && !name.contains("Folder"), "{name}");
        }
    }

    fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn init_twice_is_refused() {
        let (storage, _) = encrypted_storage();
        assert!(matches!(
            init(&storage, PASSPHRASE),
            Err(Error::AlreadyEncrypted)
        ));
    }

    #[test]
    fn an_interrupted_init_finishes_on_the_next_sync() {
        let storage = MemoryProvider::new();
        let (_a, mut laptop) = device(&[("A.md", "# alpha")]);
        SyncEngine::new(storage.clone()).run(&mut laptop).unwrap();

        // Only the key file is written: the app stopped here.
        let (file, key, _) = KeyFile::create_with(PASSPHRASE, WEAK).unwrap();
        storage.put(KEY_FILE, &file.to_json()).unwrap();

        assert!(engine(&storage, &key).run(&mut laptop).unwrap().is_empty());
        assert!(!everything(&storage).contains("alpha"));
    }

    #[test]
    fn a_device_without_the_key_is_refused() {
        let (storage, key) = encrypted_storage();
        let (_a, mut laptop) = device(&[("A.md", "# alpha")]);
        engine(&storage, &key).run(&mut laptop).unwrap();

        let (_b, mut other) = device(&[("B.md", "# beta")]);
        let plain = SyncEngine::new(storage.clone());
        assert!(matches!(plain.run(&mut other), Err(Error::Locked)));
        assert!(matches!(plain.plan(&other), Err(Error::Locked)));
        assert!(!everything(&storage).contains("beta"));
    }

    #[test]
    fn an_older_build_stops_at_the_decoy() {
        let (storage, _) = encrypted_storage();
        let decoy = storage.get(MANIFEST_KEY).unwrap().unwrap();
        // What an older build does: it parses the manifest in the old format.
        let value: serde_json::Value = serde_json::from_slice(&decoy).unwrap();
        assert!(value["format"].as_u64().unwrap() > 1);
        assert!(value["notes"].as_object().unwrap().is_empty());
    }

    #[test]
    fn a_deleted_decoy_comes_back() {
        let (storage, key) = encrypted_storage();
        storage.delete(MANIFEST_KEY).unwrap();
        let (_a, mut vault) = device(&[("A.md", "# alpha")]);

        engine(&storage, &key).run(&mut vault).unwrap();
        assert!(is_decoy(&storage.get(MANIFEST_KEY).unwrap().unwrap()));
    }

    #[test]
    fn a_plaintext_manifest_next_to_the_encrypted_one_stops_the_sync() {
        let (storage, key) = encrypted_storage();
        let (_a, mut vault) = device(&[("A.md", "# alpha")]);
        engine(&storage, &key).run(&mut vault).unwrap();

        // A device without encryption wrote here after the decoy was deleted.
        storage
            .put(MANIFEST_KEY, b"{\"format\": 1, \"notes\": {}}")
            .unwrap();

        assert!(matches!(
            engine(&storage, &key).run(&mut vault),
            Err(Error::MixedStorage)
        ));
    }

    #[test]
    fn a_different_key_is_refused() {
        let (storage, key) = encrypted_storage();
        let (_a, mut vault) = device(&[("A.md", "# alpha")]);
        engine(&storage, &key).run(&mut vault).unwrap();

        // Without the check, a different key would see an empty storage and
        // start a second set of notes next to the first one.
        let other = DataKey::generate().unwrap();
        let (_b, mut stranger) = device(&[("B.md", "# beta")]);
        assert!(matches!(
            engine(&storage, &other).run(&mut stranger),
            Err(Error::WrongKey)
        ));
        assert!(matches!(
            unlock(&storage, "wrong horse"),
            Err(Error::WrongPassphrase)
        ));
    }

    #[test]
    fn a_deleted_key_file_stops_the_sync() {
        let (storage, key) = encrypted_storage();
        storage.delete(KEY_FILE).unwrap();
        let (_a, mut vault) = device(&[("A.md", "# alpha")]);
        assert!(matches!(
            engine(&storage, &key).run(&mut vault),
            Err(Error::WrongKey)
        ));
    }

    #[test]
    fn unlock_and_a_new_passphrase() {
        let storage = MemoryProvider::new();
        let (file, key, code) = KeyFile::create_with(PASSPHRASE, WEAK).unwrap();
        init_with(&storage, file, &key).unwrap();

        assert_eq!(unlock(&storage, PASSPHRASE).unwrap(), key);
        assert_eq!(
            change_passphrase(&storage, &code.display(), "battery staple").unwrap(),
            key
        );
        assert_eq!(unlock(&storage, "battery staple").unwrap(), key);
        assert!(unlock(&storage, PASSPHRASE).is_err());
    }

    #[test]
    fn unlock_of_a_plain_storage_is_a_clear_error() {
        let storage = MemoryProvider::new();
        assert!(matches!(
            unlock(&storage, PASSPHRASE),
            Err(Error::NotEncrypted)
        ));
        assert!(!is_encrypted(&storage).unwrap());
    }

    #[test]
    fn a_damaged_object_is_an_error_not_garbage() {
        let (storage, key) = encrypted_storage();
        let (_a, mut vault) = device(&[("A.md", "# alpha")]);
        engine(&storage, &key).run(&mut vault).unwrap();

        let wrapped = Encrypted::new(storage.clone(), &key);
        let stored = wrapped.cipher.encrypt_name("A.md").unwrap();
        let mut bytes = storage.get(&stored).unwrap().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        storage.put(&stored, &bytes).unwrap();

        let error = wrapped.get("A.md").unwrap_err();
        assert!(error.to_string().contains("A.md"), "{error}");
    }
}
