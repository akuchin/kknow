//! The manifest: a list of notes with content hashes.
//!
//! One format describes three states: what is on disk, what is in the cloud,
//! and what we saw at the end of the last sync. Sync is the comparison of
//! these three lists.
//!
//! The manifest has no modification time on purpose. Clocks on two machines
//! differ, and S3 sets its own upload time, so the "newer" file by time is
//! often the older one by content. A content hash has no such problem: the
//! bytes are the same or they are not.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::{NoteId, Vault};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Where the vault keeps its last known state.
///
/// The directory is hidden, so the vault does not see it as a note.
pub const STATE_PATH: &str = ".kknow/sync.json";

/// Format version. Do not touch a manifest from a newer format: this is
/// better than a silent reduction to what this build understands.
const FORMAT: u32 = 1;

/// A note as sync sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// blake3 hash of the contents, in hex.
    pub hash: String,
    /// Size in bytes. Only for people who read the manifest.
    pub size: u64,
}

impl Entry {
    pub fn of(content: &str) -> Self {
        Entry {
            hash: hash(content.as_bytes()),
            size: content.len() as u64,
        }
    }

    /// The first eight hash characters. A person can tell them apart, and they
    /// are enough to name a conflict copy.
    pub fn short_hash(&self) -> &str {
        &self.hash[..8.min(self.hash.len())]
    }
}

pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The state of a set of notes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Wire", into = "Wire")]
pub struct Manifest {
    notes: BTreeMap<NoteId, Entry>,
}

impl Manifest {
    pub fn new() -> Self {
        Self::default()
    }

    /// The vault state on disk: reads and hashes each note.
    ///
    /// A full walk on each sync looks wasteful, but blake3 hashes 20 MB faster
    /// than the disk reads them. The trick "same size and time means same
    /// contents" would save milliseconds and add a whole class of bugs.
    pub fn of_vault(vault: &Vault) -> Result<Self> {
        let mut notes = BTreeMap::new();
        for id in vault.ids() {
            let content = vault.read(id)?;
            notes.insert(id.clone(), Entry::of(&content));
        }
        Ok(Manifest { notes })
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
        Ok(serde_json::from_slice(bytes)?)
    }

    /// Indented JSON: the manifest stays in the cloud for years, and one day a
    /// person will read or fix it by hand.
    pub fn to_json(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).expect("манифест сериализуем");
        bytes.push(b'\n');
        bytes
    }

    /// Reads the state from disk. No file means no sync happened yet.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Manifest::from_json(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::new()),
            Err(source) => Err(Error::io(path, source)),
        }
    }

    /// Writes the state through a temporary file: an interrupted write leaves
    /// the old manifest, not half of the new one.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, self.to_json()).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }
}

/// Path to the saved vault state.
pub fn state_path(vault: &Vault) -> PathBuf {
    vault.root().join(STATE_PATH)
}

/// The manifest on disk and in the cloud.
///
/// Note keys are strings in JSON and [`NoteId`] in memory, so the format is a
/// separate type. This is also the place that validates IDs from a remote
/// manifest instead of trusting them.
#[derive(Serialize, Deserialize)]
struct Wire {
    format: u32,
    notes: BTreeMap<String, Entry>,
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
        }
    }
}

impl TryFrom<Wire> for Manifest {
    type Error = String;

    fn try_from(wire: Wire) -> std::result::Result<Self, Self::Error> {
        if wire.format > FORMAT {
            return Err(format!(
                "манифест версии {}, а эта сборка знает только {FORMAT}",
                wire.format
            ));
        }

        let mut notes = BTreeMap::new();
        for (raw, entry) in wire.notes {
            let id = NoteId::parse(&raw).ok_or_else(|| format!("недопустимый ключ: {raw}"))?;
            notes.insert(id, entry);
        }
        Ok(Manifest { notes })
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
