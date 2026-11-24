use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::{NoteId, Vault};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const STATE_PATH: &str = ".kknow/sync.json";

const FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

impl Entry {
    pub fn of(content: &str) -> Self {
        Entry::of_bytes(content.as_bytes())
    }

    pub fn of_bytes(bytes: &[u8]) -> Self {
        Entry {
            hash: hash(bytes),
            size: bytes.len() as u64,
            locked: kknow_crypto::locked::is_locked(bytes),
        }
    }

    pub fn short_hash(&self) -> &str {
        &self.hash[..8.min(self.hash.len())]
    }
}

pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Wire", into = "Wire")]
pub struct Manifest {
    notes: BTreeMap<NoteId, Entry>,
    files: BTreeMap<String, Entry>,
}

impl Manifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn of_vault(vault: &Vault) -> Result<Self> {
        let mut notes = BTreeMap::new();
        for id in vault.ids() {
            let content = vault.read(id)?;
            notes.insert(id.clone(), Entry::of(&content));
        }
        let files = crate::shared::read_all(vault.root())?
            .into_iter()
            .map(|(key, bytes)| (key, Entry::of_bytes(&bytes)))
            .collect();
        Ok(Manifest { notes, files })
    }

    pub fn file_hash(&self, key: &str) -> Option<&str> {
        self.files.get(key).map(|entry| entry.hash.as_str())
    }

    pub fn insert_file(&mut self, key: String, entry: Entry) {
        self.files.insert(key, entry);
    }

    pub fn remove_file(&mut self, key: &str) {
        self.files.remove(key);
    }

    pub fn file_keys(&self) -> impl ExactSizeIterator<Item = &String> {
        self.files.keys()
    }

    pub fn file_entries(&self) -> impl ExactSizeIterator<Item = (&String, &Entry)> {
        self.files.iter()
    }

    pub fn get(&self, id: &NoteId) -> Option<&Entry> {
        self.notes.get(id)
    }

    pub fn hash_of(&self, id: &NoteId) -> Option<&str> {
        self.notes.get(id).map(|entry| entry.hash.as_str())
    }

    pub fn insert(&mut self, id: NoteId, entry: Entry) {
        self.notes.insert(id, entry);
    }

    pub fn remove(&mut self, id: &NoteId) {
        self.notes.remove(id);
    }

    pub fn contains(&self, id: &NoteId) -> bool {
        self.notes.contains_key(id)
    }

    pub fn ids(&self) -> impl ExactSizeIterator<Item = &NoteId> {
        self.notes.keys()
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = (&NoteId, &Entry)> {
        self.notes.iter()
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if crate::encrypted::is_decoy(bytes) {
            return Err(crate::encrypted::Error::Locked.into());
        }
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn to_json(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).expect("the manifest serializes");
        bytes.push(b'\n');
        bytes
    }

    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Manifest::from_json(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::new()),
            Err(source) => Err(Error::io(path, source)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, self.to_json()).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }
}

pub fn state_path(vault: &Vault) -> PathBuf {
    vault.root().join(STATE_PATH)
}

#[derive(Serialize, Deserialize)]
struct Wire {
    format: u32,
    notes: BTreeMap<String, Entry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    files: BTreeMap<String, Entry>,
}

impl From<Manifest> for Wire {
    fn from(manifest: Manifest) -> Self {
        Wire {
            format: FORMAT,
            notes: manifest
                .notes
                .into_iter()
                .map(|(id, entry)| (id.as_str().to_string(), entry))
                .collect(),
            files: manifest.files,
        }
    }
}

impl TryFrom<Wire> for Manifest {
    type Error = String;

    fn try_from(wire: Wire) -> std::result::Result<Self, Self::Error> {
        if wire.format > FORMAT {
            return Err(format!(
                "the manifest has format {}, and this build knows only {FORMAT}: update kknow",
                wire.format
            ));
        }

        let mut notes = BTreeMap::new();
        for (raw, entry) in wire.notes {
            let id = NoteId::parse(&raw).ok_or_else(|| format!("invalid key: {raw}"))?;
            notes.insert(id, entry);
        }
        for key in wire.files.keys() {
            if !crate::shared::is_shared(key) {
                return Err(format!("invalid service file: {key}"));
            }
        }
        Ok(Manifest {
            notes,
            files: wire.files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(notes: &[(&str, &str)]) -> Manifest {
        let mut manifest = Manifest::new();
        for (id, content) in notes {
            manifest.insert(NoteId::parse(id).unwrap(), Entry::of(content));
        }
        manifest
    }

    #[test]
    fn json_round_trip_keeps_every_note() {
        let original = manifest(&[("Index", "# Оглавление"), ("Projects/kknow", "# kknow")]);

        let restored = Manifest::from_json(&original.to_json()).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn same_content_gives_the_same_hash() {
        let first = Entry::of("# kknow\n");
        let second = Entry::of("# kknow\n");

        assert_eq!(first, second);
        assert_ne!(first.hash, Entry::of("# kknow").hash);
    }

    #[test]
    fn rejects_a_key_that_escapes_the_vault() {
        let json = br#"{"format":1,"notes":{"../secrets":{"hash":"00","size":0}}}"#;

        assert!(Manifest::from_json(json).is_err());
    }

    #[test]
    fn rejects_a_manifest_from_the_future() {
        let json = br#"{"format":99,"notes":{}}"#;

        assert!(Manifest::from_json(json).is_err());
    }

    #[test]
    fn missing_state_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();

        let state = Manifest::load(&dir.path().join(STATE_PATH)).unwrap();

        assert!(state.is_empty());
    }

    #[test]
    fn save_creates_the_hidden_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STATE_PATH);
        let state = manifest(&[("Index", "# Оглавление")]);

        state.save(&path).unwrap();

        assert_eq!(Manifest::load(&path).unwrap(), state);
        assert!(!dir.path().join(".kknow/sync.json.tmp").exists());
    }
}
