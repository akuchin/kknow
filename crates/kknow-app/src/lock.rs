//! Locked notes of a vault: the password, the session and the lock of one
//! note.
//!
//! The format and the crypto are in `kknow_sync::locked`. This module keeps
//! the session: the notes key lives in [`Workspace`] while the session is
//! unlocked, and only in memory. It is never written to disk or to the
//! keychain, and [`Workspace::lock_all`] drops it.
//!
//! The rule for readers: [`Workspace::read`] refuses a locked note, also in
//! an unlocked session. That is the read of agents, MCP tools, proposals and
//! threads. Only [`Workspace::open_text`], the read of the person in the
//! window, opens it.

use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::NoteId;
use kknow_sync::RecoveryCode;
use kknow_sync::crypto::{KdfParams, KeyFile, Secret};
use kknow_sync::locked::{self, LOCK_FILE, NotesKey};

use crate::{Error, Result, Workspace};

/// The lock state of the vault for the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockState {
    /// The vault has a password: `.kknow/lock.json` exists.
    pub password: bool,
    /// The notes key is in memory: locked notes open.
    pub unlocked: bool,
}

/// What opened the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlocked {
    Password,
    /// The recovery code: the person forgot the password, so the window asks
    /// for a new one.
    RecoveryCode,
}

impl Workspace {
    /// The path of `.kknow/lock.json`.
    pub fn lock_path(&self) -> PathBuf {
        self.root().join(LOCK_FILE)
    }

    pub fn lock_state(&self) -> LockState {
        LockState {
            password: self.lock_path().is_file(),
            unlocked: self.notes_key.is_some(),
        }
    }

    /// True if the file of the note is a locked note.
    pub fn is_locked(&self, id: &NoteId) -> bool {
        self.locked.contains(id)
    }

    /// The locked notes of the vault, in ID order.
    pub fn locked_notes(&self) -> impl Iterator<Item = &NoteId> {
        self.locked.iter()
    }

    /// Sets the first password of the vault and unlocks the session. The
    /// recovery code comes back once: kknow does not keep it.
    pub fn set_lock_password(&mut self, password: &str) -> Result<RecoveryCode> {
        self.set_lock_password_with(password, KdfParams::default())
    }

    /// [`Workspace::set_lock_password`] with other Argon2 parameters. Tests
    /// give weak ones to stay fast.
    #[doc(hidden)]
    pub fn set_lock_password_with(
        &mut self,
        password: &str,
        kdf: KdfParams,
    ) -> Result<RecoveryCode> {
        if self.lock_state().password {
            return Err(Error::LockPasswordExists);
        }
        let (file, key, code) = locked::create_with(password, kdf)?;
        write_lock_file(&self.lock_path(), &file)?;
        self.notes_key = Some(key);
        Ok(code)
    }

    /// Opens the session with the password or the recovery code. All locked
    /// notes of the vault open until [`Workspace::lock_all`] or the end of
    /// the process.
    pub fn unlock_notes(&mut self, secret: &str) -> Result<Unlocked> {
        let file = self.lock_file()?;
        let (key, secret) = file.open(secret)?;
        self.notes_key = Some(NotesKey::new(&key));
        Ok(match secret {
            Secret::Passphrase => Unlocked::Password,
            Secret::RecoveryCode => Unlocked::RecoveryCode,
        })
    }

    /// A new password for the same notes key. It needs an unlocked session;
    /// only `lock.json` changes, not the notes. The recovery code stays.
    pub fn change_lock_password(&mut self, password: &str) -> Result<()> {
        let mut file = self.lock_file()?;
        let key = self.notes_key()?;
        file.change_passphrase(key.data_key(), password)?;
        write_lock_file(&self.lock_path(), &file)
    }

    /// Drops the notes key from memory. All locked notes close.
    pub fn lock_all(&mut self) {
        self.notes_key = None;
    }

    /// Locks a plain note: its file becomes a locked note with the same name.
    /// It needs an unlocked session.
    pub fn lock_note(&mut self, id: &NoteId) -> Result<()> {
        if !self.contains(id) {
            return Err(kknow_core::Error::NoteNotFound(id.clone()).into());
        }
        if self.is_locked(id) {
            return Err(Error::AlreadyLocked(id.clone()));
        }
        let text = self.read(id)?;
        self.store_as(id, &text, true)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    /// Writes a locked note back as plain text. It needs an unlocked session.
    pub fn remove_lock(&mut self, id: &NoteId) -> Result<()> {
        if !self.is_locked(id) {
            return Err(Error::NotLocked(id.clone()));
        }
        let text = self.open_text(id)?;
        self.store_as(id, &text, false)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    /// The text of the note for the person in the window: a locked note opens
    /// with the key of the session, or gives [`Error::NotesClosed`].
    pub fn open_text(&self, id: &NoteId) -> Result<String> {
        let content = self.vault.read(id)?;
        if !locked::is_locked(content.as_bytes()) {
            return Ok(content);
        }
        Ok(self.notes_key()?.open(&content)?)
    }

    pub(crate) fn notes_key(&self) -> Result<&NotesKey> {
        match &self.notes_key {
            Some(key) => Ok(key),
            None if self.lock_state().password => Err(Error::NotesClosed),
            None => Err(Error::NoLockPassword),
        }
    }

    fn lock_file(&self) -> Result<KeyFile> {
        let path = self.lock_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::NoLockPassword);
            }
            Err(source) => return Err(Error::io(&path, source)),
        };
        Ok(KeyFile::from_json(&bytes)?)
    }
}

/// Writes through a temp file and `rename`: a half-written `lock.json` would
/// lose all locked notes.
fn write_lock_file(path: &Path, file: &KeyFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    let mut bytes = file.to_json();
    bytes.push(b'\n');
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
    fs::rename(&temp, path).map_err(|source| Error::io(path, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Saved;
    use crate::bookmarks::Mark;

    const WEAK: KdfParams = KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };
    const PASSWORD: &str = "correct horse";

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Diary.md"),
            "# Diary\n\nMet [[Anna]] about #health and the zebra.\n- [ ] call back\n",
        )
        .unwrap();
        fs::write(dir.path().join("Anna.md"), "# Anna\n\nSee [[Diary]].\n").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    fn locked_workspace() -> (tempfile::TempDir, Workspace, RecoveryCode) {
        let (dir, mut workspace) = workspace();
        let code = workspace.set_lock_password_with(PASSWORD, WEAK).unwrap();
        workspace.lock_note(&id("Diary")).unwrap();
        (dir, workspace, code)
    }

    fn disk(dir: &tempfile::TempDir, name: &str) -> String {
        fs::read_to_string(dir.path().join(name)).unwrap()
    }

    #[test]
    fn a_locked_note_is_ciphertext_on_disk_with_the_same_name() {
        let (dir, workspace, _) = locked_workspace();
        let text = disk(&dir, "Diary.md");
        assert!(locked::is_locked(text.as_bytes()));
        assert!(!text.contains("zebra"));
        assert!(workspace.is_locked(&id("Diary")));
        assert!(workspace.lock_state().unlocked);
        assert!(dir.path().join(LOCK_FILE).is_file());
    }

    #[test]
    fn a_vault_without_a_password_cannot_lock() {
        let (_dir, mut workspace) = workspace();
        assert!(matches!(
            workspace.lock_note(&id("Diary")),
            Err(Error::NoLockPassword)
        ));
        assert!(!workspace.is_locked(&id("Diary")));
    }

    #[test]
    fn the_index_never_has_the_text_of_a_locked_note() {
        let (_dir, workspace, _) = locked_workspace();
        let diary = id("Diary");
        // The session is unlocked, and still nothing is in the index.
        assert!(workspace.search("zebra", 10).unwrap().is_empty());
        assert!(workspace.search("health", 10).unwrap().is_empty());
        let by_name: Vec<_> = workspace
            .search("Diary", 10)
            .unwrap()
            .into_iter()
            .map(|hit| hit.id)
            .collect();
        // Anna links to it by name, so it is found too.
        assert!(by_name.contains(&diary));

        let parsed = workspace.parsed(&diary).unwrap();
        assert!(parsed.tags.is_empty() && parsed.tasks.is_empty());
        assert!(workspace.outgoing(&diary).is_empty());
        assert!(workspace.backlinks(&id("Anna")).is_empty());
        // A link to the locked note still works.
        assert_eq!(workspace.backlinks(&diary).len(), 1);
    }

    #[test]
    fn agents_never_read_a_locked_note() {
        let (_dir, mut workspace, _) = locked_workspace();
        let diary = id("Diary");
        assert!(matches!(workspace.read(&diary), Err(Error::NoteLocked(_))));
        assert!(matches!(
            workspace.set_task(&diary, 4, true),
            Err(Error::NoteLocked(_))
        ));
        assert!(workspace.render_section(&diary, None).is_err());
    }

    #[test]
    fn the_person_reads_and_saves_while_unlocked() {
        let (dir, mut workspace, _) = locked_workspace();
        let diary = id("Diary");
        let text = workspace.open_text(&diary).unwrap();
        assert!(text.contains("zebra"));

        let edited = text.replace("zebra", "giraffe");
        assert_eq!(
            workspace.save_if_unchanged(&diary, &edited, &text).unwrap(),
            Saved::Written
        );
        let on_disk = disk(&dir, "Diary.md");
        assert!(locked::is_locked(on_disk.as_bytes()));
        assert!(!on_disk.contains("giraffe"));
        assert_eq!(workspace.open_text(&diary).unwrap(), edited);
        assert!(workspace.search("giraffe", 10).unwrap().is_empty());

        workspace.set_task_unlocked(&diary, 4, true).unwrap();
        assert!(
            workspace
                .open_text(&diary)
                .unwrap()
                .contains("- [x] call back")
        );
        assert!(locked::is_locked(disk(&dir, "Diary.md").as_bytes()));
    }

    #[test]
    fn a_conflict_copy_of_a_locked_note_is_locked_too() {
        let (dir, mut workspace, _) = locked_workspace();
        let diary = id("Diary");
        let base = workspace.open_text(&diary).unwrap();
        workspace
            .save(&diary, "# Diary\n\nfrom elsewhere\n")
            .unwrap();

        let Saved::Conflict { copy, disk: theirs } = workspace
            .save_if_unchanged(&diary, "# Diary\n\nmine\n", &base)
            .unwrap()
        else {
            panic!("a conflict was expected");
        };
        assert_eq!(theirs, "# Diary\n\nfrom elsewhere\n");
        assert!(workspace.is_locked(&copy));
        let copy_text = disk(&dir, &format!("{copy}.md"));
        assert!(locked::is_locked(copy_text.as_bytes()));
        assert_eq!(workspace.open_text(&copy).unwrap(), "# Diary\n\nmine\n");
    }

    #[test]
    fn lock_all_closes_the_notes_and_the_password_opens_them() {
        let (_dir, mut workspace, _) = locked_workspace();
        let diary = id("Diary");
        workspace.lock_all();
        assert!(!workspace.lock_state().unlocked);
        assert!(matches!(
            workspace.open_text(&diary),
            Err(Error::NotesClosed)
        ));
        assert!(matches!(
            workspace.save(&diary, "plain"),
            Err(Error::NotesClosed)
        ));

        assert!(matches!(
            workspace.unlock_notes("wrong horse"),
            Err(Error::Sync(kknow_sync::Error::WrongPassphrase))
        ));
        assert_eq!(
            workspace.unlock_notes(PASSWORD).unwrap(),
            Unlocked::Password
        );
        assert!(workspace.open_text(&diary).unwrap().contains("zebra"));
    }

    #[test]
    fn the_recovery_code_opens_and_sets_a_new_password() {
        let (dir, mut workspace, code) = locked_workspace();
        let before = disk(&dir, "Diary.md");
        workspace.lock_all();
        assert_eq!(
            workspace.unlock_notes(&code.display()).unwrap(),
            Unlocked::RecoveryCode
        );
        workspace.change_lock_password("battery staple").unwrap();
        // The notes stay as they are: only lock.json changed.
        assert_eq!(disk(&dir, "Diary.md"), before);

        workspace.lock_all();
        assert!(workspace.unlock_notes(PASSWORD).is_err());
        workspace.unlock_notes("battery staple").unwrap();
        assert!(workspace.open_text(&id("Diary")).unwrap().contains("zebra"));
    }

    #[test]
    fn a_second_device_opens_the_notes_with_the_same_password() {
        let (dir, mut workspace, _) = locked_workspace();
        workspace.lock_all();
        let other = Workspace::open(dir.path()).unwrap();
        assert!(other.is_locked(&id("Diary")));
        assert_eq!(
            other.lock_state(),
            LockState {
                password: true,
                unlocked: false
            }
        );
        let mut other = other;
        other.unlock_notes(PASSWORD).unwrap();
        assert!(other.open_text(&id("Diary")).unwrap().contains("zebra"));
    }

    #[test]
    fn remove_lock_writes_plain_text_back() {
        let (dir, mut workspace, _) = locked_workspace();
        let diary = id("Diary");
        workspace.lock_all();
        assert!(matches!(
            workspace.remove_lock(&diary),
            Err(Error::NotesClosed)
        ));
        workspace.unlock_notes(PASSWORD).unwrap();
        workspace.remove_lock(&diary).unwrap();
        assert!(disk(&dir, "Diary.md").contains("zebra"));
        assert!(!workspace.is_locked(&diary));
        assert_eq!(workspace.search("zebra", 10).unwrap().len(), 1);
        assert_eq!(workspace.backlinks(&id("Anna")).len(), 1);
    }

    #[test]
    fn a_locked_note_keeps_no_bookmarks() {
        let (_dir, mut workspace) = workspace();
        let diary = id("Diary");
        let text = workspace.read(&diary).unwrap();
        workspace.toggle_bookmark(&diary, 3).unwrap();
        assert_eq!(workspace.note_bookmarks(&diary, &text).len(), 1);

        workspace.set_lock_password_with(PASSWORD, WEAK).unwrap();
        workspace.lock_note(&diary).unwrap();
        assert!(workspace.note_bookmarks(&diary, &text).is_empty());
        workspace
            .set_bookmarks(&diary, &[Mark::new(3, None)], &text)
            .unwrap();
        assert!(workspace.note_bookmarks(&diary, &text).is_empty());
        assert!(workspace.toggle_bookmark(&diary, 3).is_err());
    }

    #[test]
    fn a_locked_note_in_the_trash_stays_locked() {
        let (dir, mut workspace, _) = locked_workspace();
        workspace.delete(&id("Diary")).unwrap();
        let trash = workspace.trash();
        let entry = trash.entries().unwrap().remove(0);
        let text = trash.read(&entry.id, "Diary.md").unwrap();
        assert!(locked::is_locked(text.as_bytes()));
        assert!(!text.contains("zebra"));
        assert!(dir.path().join(".kknow/trash").is_dir());
    }

    #[test]
    fn rename_and_duplicate_keep_the_lock() {
        let (_dir, mut workspace, _) = locked_workspace();
        workspace
            .rename(&id("Diary"), &id("Private/Diary"))
            .unwrap();
        let moved = id("Private/Diary");
        assert!(workspace.is_locked(&moved));
        assert!(workspace.open_text(&moved).unwrap().contains("zebra"));

        let copy = workspace.duplicate(&moved).unwrap();
        assert!(workspace.is_locked(&copy));
        assert!(workspace.open_text(&copy).unwrap().contains("zebra"));
    }

    #[test]
    fn an_outside_write_of_a_locked_note_is_seen() {
        let (dir, mut workspace, _) = locked_workspace();
        workspace.remove_lock(&id("Diary")).unwrap();
        let mut other = Workspace::open(dir.path()).unwrap();
        other.unlock_notes(PASSWORD).unwrap();
        other.lock_note(&id("Diary")).unwrap();

        let changes = workspace
            .apply(&[crate::watch::VaultEvent::Changed(id("Diary"))])
            .unwrap();
        assert_eq!(changes.updated, [id("Diary")]);
        assert!(workspace.is_locked(&id("Diary")));
        assert!(workspace.search("zebra", 10).unwrap().is_empty());
    }

    #[test]
    fn the_first_password_is_set_once() {
        let (_dir, mut workspace, _) = locked_workspace();
        assert!(matches!(
            workspace.set_lock_password_with("another password", WEAK),
            Err(Error::LockPasswordExists)
        ));
    }
}
