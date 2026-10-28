//! Vault registry: the notes directories that kknow knows.
//!
//! A vault is a directory plus what the app knows about it: name, colour label
//! and position in the list. Sync settings are not here: they are inside the
//! directory (`.kknow/remote.json`). Moving them to a shared file would bring
//! back a bug we removed: a second vault in someone else's bucket.
//!
//! The window and the CLI share the registry. Otherwise "Work" in the app and
//! `kknow --vault work` in the shell would be two different lists. The file path
//! comes as an argument: the core must not know about Tauri or where the
//! platform keeps settings. [`config_dir`] is here only so that both frontends
//! ask for the same place.

use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::NoteId;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const REGISTRY_FILE: &str = "vaults.json";

/// File from the previous version: one remembered directory. The registry is
/// built from it.
pub const LEGACY_FILE: &str = "last-vault.txt";

/// Number of vaults that hotkeys ⌘1…⌘9 open.
pub const HOTKEYS: usize = 9;

/// Vault label colour.
///
/// The set is fixed, not any `#hex` from settings: the label is next to UI text,
/// and a random shade would be unreadable on a dark theme. The theme sets the
/// actual values (`--label-*`); here is only the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelColor {
    Blue,
    Violet,
    Amber,
    Red,
    Green,
    Teal,
}

impl LabelColor {
    pub const ALL: [LabelColor; 6] = [
        LabelColor::Blue,
        LabelColor::Violet,
        LabelColor::Amber,
        LabelColor::Red,
        LabelColor::Green,
        LabelColor::Teal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LabelColor::Blue => "blue",
            LabelColor::Violet => "violet",
            LabelColor::Amber => "amber",
            LabelColor::Red => "red",
            LabelColor::Green => "green",
            LabelColor::Teal => "teal",
        }
    }
}

/// One vault in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    /// Stable ID that commands and events use. The name can change, the ID
    /// cannot.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub color: LabelColor,
    /// Text labels, for example `work`. They group vaults: `kknow mcp --label
    /// work` serves all vaults with the label. Not the same as the colour
    /// label above: that one is a mark for the eye, these are for selection.
    ///
    /// An old registry has no field, so it reads as empty, and an empty list
    /// is not written. Thus the format version stays the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl VaultEntry {
    /// Checks that the directory exists. A missing one (an external disk, a
    /// cloud folder not downloaded yet) stays in the list as unavailable.
    pub fn available(&self) -> bool {
        self.path.is_dir()
    }
}

/// Current registry format version.
pub const VERSION: u32 = 1;

/// The list of vaults and which one is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultRegistry {
    /// Format version: unlike the directory settings, the registry lives through
    /// several releases and gets new fields.
    #[serde(default = "current_version")]
    version: u32,
    #[serde(default)]
    vaults: Vec<VaultEntry>,
    /// ID of the active vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<String>,
}

fn current_version() -> u32 {
    VERSION
}

/// An empty registry already has the current version: `#[derive(Default)]` would
/// write version zero and make the field useless.
impl Default for VaultRegistry {
    fn default() -> Self {
        VaultRegistry {
            version: VERSION,
            vaults: Vec::new(),
            active: None,
        }
    }
}

/// Result of reading the registry.
#[derive(Debug)]
pub struct Loaded {
    pub registry: VaultRegistry,
    /// The file did not parse: it is moved aside and the list starts empty. A
    /// silent "list is empty" looks like "list is lost", so the problem goes up
    /// to the status bar.
    pub problem: Option<String>,
}

impl VaultRegistry {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(REGISTRY_FILE)
    }

    /// Reads the registry. No file gives an empty list. A broken file also does,
    /// plus a problem message and a backup copy next to it.
    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    registry: VaultRegistry::default(),
                    problem: None,
                });
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        match serde_json::from_slice::<VaultRegistry>(&bytes) {
            Ok(mut registry) => {
                registry.tidy();
                Ok(Loaded {
                    registry,
                    problem: None,
                })
            }
            Err(error) => {
                let broken = path.with_extension("json.broken");
                let _ = fs::rename(path, &broken);
                Ok(Loaded {
                    registry: VaultRegistry::default(),
                    problem: Some(format!(
                        "реестр хранилищ не разобрать ({error}); он отложен в {}",
                        broken.display()
                    )),
                })
            }
        }
    }

    /// Writes through a temporary file, like the manifest and the settings.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("реестр сериализуем");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    /// Repairs the values that were read: the active vault must exist, and IDs
    /// must be unique. People edit the file by hand, so repair it, do not panic.
    fn tidy(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        self.vaults.retain(|entry| {
            let fresh = !entry.id.is_empty() && !seen.contains(&entry.id);
            if fresh {
                seen.push(entry.id.clone());
            }
            fresh
        });

        if !self.active.as_ref().is_some_and(|id| seen.contains(id)) {
            self.active = self.vaults.first().map(|entry| entry.id.clone());
        }
    }

    pub fn entries(&self) -> &[VaultEntry] {
        &self.vaults
    }

    pub fn is_empty(&self) -> bool {
        self.vaults.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&VaultEntry> {
        self.vaults.iter().find(|entry| entry.id == id)
    }

    /// The active vault: the main window works with it.
    pub fn active(&self) -> Option<&VaultEntry> {
        self.active.as_deref().and_then(|id| self.get(id))
    }

    pub fn set_active(&mut self, id: &str) -> Result<()> {
        if self.get(id).is_none() {
            return Err(Error::UnknownVault(id.to_string()));
        }
        self.active = Some(id.to_string());
        Ok(())
    }

    /// The vault for ⌘1…⌘9 is a position in the list.
    pub fn by_hotkey(&self, number: usize) -> Option<&VaultEntry> {
        if number == 0 || number > HOTKEYS {
            return None;
        }
        self.vaults.get(number - 1)
    }

    /// Finds a vault by ID, name or path.
    ///
    /// The CLI needs this: `kknow --vault work` and `kknow --vault ~/notes` must
    /// lead to the same place.
    pub fn find(&self, needle: &str) -> Option<&VaultEntry> {
        if let Some(entry) = self.get(needle) {
            return Some(entry);
        }

        let lower = needle.to_lowercase();
        if let Some(entry) = self
            .vaults
            .iter()
            .find(|entry| entry.name.to_lowercase() == lower)
        {
            return Some(entry);
        }

        let path = Path::new(needle).canonicalize().ok()?;
        self.vaults.iter().find(|entry| entry.path == path)
    }

    /// The vault that contains the file, and the note the file is in it.
    ///
    /// A file from a known directory opens as a note of that vault, not as a
    /// single file. Then it has links, backlinks and search, and one note does
    /// not show as two rows. The vault does not treat a file in a hidden folder
    /// (`.kknow`, `.trash`) as a note, so such a file stays single.
    pub fn locate(&self, path: &Path) -> Option<(&VaultEntry, NoteId)> {
        let path = path.canonicalize().ok()?;
        let entry = self
            .vaults
            .iter()
            .find(|entry| path.starts_with(&entry.path))?;
        let relative = path.strip_prefix(&entry.path).ok()?;

        let hidden = relative
            .components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'));
        if hidden {
            return None;
        }
        NoteId::from_relative_path(relative).map(|id| (entry, id))
    }

    /// Adds the directory and makes it active if no vault is active.
    ///
    /// The path is canonicalized: macOS replaces `/var` with `/private/var`, and
    /// without this one directory could get into the list twice.
    pub fn add(&mut self, path: impl AsRef<Path>, name: Option<String>) -> Result<&VaultEntry> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(Error::NotADirectory(path.to_path_buf()));
        }
        let path = path
            .canonicalize()
            .map_err(|source| Error::io(path, source))?;

        if let Some(existing) = self.vaults.iter().find(|entry| entry.path == path) {
            return Err(Error::DuplicateVault(existing.path.clone()));
        }
        // Nested vaults would share files: two watchers on one directory and two
        // syncs to different remotes would split the notes apart.
        if let Some(other) = self
            .vaults
            .iter()
            .find(|entry| path.starts_with(&entry.path) || entry.path.starts_with(&path))
        {
            return Err(Error::NestedVault(path, other.path.clone()));
        }

        let name = name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| folder_name(&path));

        let entry = VaultEntry {
            id: self.free_id(&name),
            color: self.free_color(),
            name,
            path,
            labels: Vec::new(),
        };

        self.vaults.push(entry);
        let added = self.vaults.last().expect("только что добавили");
        if self.active.is_none() {
            self.active = Some(added.id.clone());
        }
        Ok(added)
    }

    /// Removes the vault from the list. The files stay on disk.
    pub fn remove(&mut self, id: &str) -> Result<VaultEntry> {
        let index = self
            .vaults
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(|| Error::UnknownVault(id.to_string()))?;

        let removed = self.vaults.remove(index);
        if self.active.as_deref() == Some(id) {
            // Another vault becomes active: the window must show something.
            self.active = self.vaults.first().map(|entry| entry.id.clone());
        }
        Ok(removed)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::InvalidName(name.to_string()));
        }
        self.entry_mut(id)?.name = name.to_string();
        Ok(())
    }

    pub fn set_color(&mut self, id: &str, color: LabelColor) -> Result<()> {
        self.entry_mut(id)?.color = color;
        Ok(())
    }

    /// Replaces the labels of the vault. The labels go through
    /// [`clean_labels`], so the stored form is always the same.
    pub fn set_labels<'a>(
        &mut self,
        id: &str,
        labels: impl IntoIterator<Item = &'a str>,
    ) -> Result<()> {
        self.entry_mut(id)?.labels = clean_labels(labels);
        Ok(())
    }

    /// Vaults with at least one of the labels, in list order.
    ///
    /// "At least one", not "all": `--label work --label study` is a person
    /// who wants both groups, not the vaults that are in both.
    pub fn with_labels<'a>(&self, labels: impl IntoIterator<Item = &'a str>) -> Vec<&VaultEntry> {
        let wanted = clean_labels(labels);
        self.vaults
            .iter()
            .filter(|entry| entry.labels.iter().any(|label| wanted.contains(label)))
            .collect()
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut VaultEntry> {
        self.vaults
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| Error::UnknownVault(id.to_string()))
    }

    /// ID from the name; a number is added if the ID is taken.
    fn free_id(&self, name: &str) -> String {
        let base = slug(name);
        if self.get(&base).is_none() {
            return base;
        }
        (2..)
            .map(|number| format!("{base}-{number}"))
            .find(|candidate| self.get(candidate).is_none())
            .expect("свободный идентификатор находится")
    }

    /// The first label colour not in use. When all are in use, start the cycle
    /// again.
    fn free_color(&self) -> LabelColor {
        LabelColor::ALL
            .into_iter()
            .find(|color| self.vaults.iter().all(|entry| entry.color != *color))
            .unwrap_or(LabelColor::ALL[self.vaults.len() % LabelColor::ALL.len()])
    }
}

/// Labels in one form: no `#` in front, lower case, spaces become hyphens.
/// Empty labels and repeats go away, the order stays.
///
/// One form is necessary because a person types the label in two places: in
/// the settings and in `--label`. `Work`, `#work` and ` work ` must be the
/// same label. Spaces become hyphens because a label is one word on the
/// command line.
pub fn clean_labels<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for label in raw {
        let label = label
            .trim()
            .trim_start_matches('#')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase();
        if !label.is_empty() && !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels
}

/// The directory name is the default vault name.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// ID from a name: letters and digits stay, everything else becomes a hyphen.
///
/// Cyrillic is not transliterated: the ID is for the machine, not for people,
/// so a Cyrillic ID is as good as a Latin one. A name without letters and
/// digits gives `vault`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for symbol in name.to_lowercase().chars() {
        if symbol.is_alphanumeric() {
            out.push(symbol);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }

    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "vault".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The app settings directory.
///
/// It matches what Tauri gives (`app_config_dir` for `dev.kknow.desktop`). Both
/// frontends call it, so the window and the CLI always use one registry file,
/// not two similar ones.
pub fn config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);

    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library/Application Support/dev.kknow.desktop"));
    }
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("kknow"));
    }

    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".config")))
        .map(|dir| dir.join("kknow"))
}

/// Reads the registry. On first use, it moves in the directory that the
/// previous version remembered.
///
/// Shared entry point for the window and the CLI: the migration must happen
/// once and the same way, no matter which one starts first.
pub fn open(config_dir: &Path) -> Result<Loaded> {
    let path = VaultRegistry::path(config_dir);
    let mut loaded = VaultRegistry::load(&path)?;

    if loaded.registry.is_empty() && migrate_legacy(config_dir, &mut loaded.registry)? {
        loaded.registry.save(&path)?;
    }
    Ok(loaded)
}

/// Turns the directory from the previous version into a one-vault registry.
///
/// Then the old file is deleted: there is nothing to migrate a second time, and
/// a remaining file would look like a second source of truth.
pub fn migrate_legacy(config_dir: &Path, registry: &mut VaultRegistry) -> Result<bool> {
    let legacy = config_dir.join(LEGACY_FILE);
    let Ok(text) = fs::read_to_string(&legacy) else {
        return Ok(false);
    };

    let path = PathBuf::from(text.trim());
    let added = path.is_dir() && registry.add(&path, None).is_ok();
    let _ = fs::remove_file(&legacy);
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn folder(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn add_gives_a_name_an_id_and_a_colour() {
        let home = dir();
        let mut registry = VaultRegistry::default();

        let entry = registry.add(folder(home.path(), "Заметки"), None).unwrap();

        assert_eq!(entry.name, "Заметки");
        assert_eq!(entry.id, "заметки");
        assert_eq!(entry.color, LabelColor::Blue);
        assert_eq!(registry.active().unwrap().id, "заметки");
    }

    #[test]
    fn labels_have_one_form() {
        assert_eq!(
            clean_labels(["Work", " #work ", "личное хранилище", "", "#"]),
            ["work", "личное-хранилище"]
        );
    }

    #[test]
    fn with_labels_takes_vaults_with_any_of_the_labels() {
        let home = dir();
        let mut registry = VaultRegistry::default();
        for name in ["Работа", "Учёба", "Дом"] {
            registry.add(folder(home.path(), name), None).unwrap();
        }
        registry.set_labels("работа", ["work", "ai"]).unwrap();
        registry.set_labels("учёба", ["#Study"]).unwrap();

        let names = |labels: &[&str]| -> Vec<String> {
            registry
                .with_labels(labels.iter().copied())
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        };
        assert_eq!(names(&["WORK"]), ["Работа"]);
        assert_eq!(names(&["work", "study"]), ["Работа", "Учёба"]);
        assert!(names(&["nothing"]).is_empty());
    }

    #[test]
    fn labels_survive_a_save_and_old_files_read_without_them() {
        let home = dir();
        let file = home.path().join(REGISTRY_FILE);
        let mut registry = VaultRegistry::default();
        registry.add(folder(home.path(), "notes"), None).unwrap();
        registry.save(&file).unwrap();
        assert!(
            !fs::read_to_string(&file).unwrap().contains("labels"),
            "пустой список не пишется"
        );

        registry.set_labels("notes", ["work"]).unwrap();
        registry.save(&file).unwrap();
        let loaded = VaultRegistry::load(&file).unwrap().registry;
        assert_eq!(loaded.entries()[0].labels, ["work"]);
    }

    #[test]
    fn names_may_repeat_but_ids_may_not() {
        let home = dir();
        let mut registry = VaultRegistry::default();

        registry
            .add(folder(home.path(), "a/notes"), Some("Notes".into()))
            .unwrap();
        let second = registry
            .add(folder(home.path(), "b/notes"), Some("Notes".into()))
            .unwrap();

        assert_eq!(second.id, "notes-2");
        assert_ne!(registry.entries()[0].color, registry.entries()[1].color);
    }

    #[test]
    fn a_file_inside_a_vault_is_its_note() {
        let home = dir();
        let notes = folder(home.path(), "notes");
        fs::create_dir_all(notes.join("Проекты")).unwrap();
        fs::create_dir_all(notes.join(".kknow")).unwrap();
        fs::write(notes.join("Проекты/kknow.md"), "# kknow\n").unwrap();
        fs::write(notes.join(".kknow/служебное.md"), "").unwrap();
        fs::write(home.path().join("README.md"), "").unwrap();
        let mut registry = VaultRegistry::default();
        registry.add(&notes, None).unwrap();

        let (entry, id) = registry
            .locate(&notes.join("Проекты/kknow.md"))
            .expect("файл из хранилища");
        assert_eq!(entry.name, "notes");
        assert_eq!(id.as_str(), "Проекты/kknow");

        assert!(registry.locate(&home.path().join("README.md")).is_none());
        assert!(
            registry
                .locate(&notes.join(".kknow/служебное.md"))
                .is_none(),
            "скрытую папку vault не читает"
        );
    }

    #[test]
    fn the_same_folder_is_not_added_twice() {
        let home = dir();
        let mut registry = VaultRegistry::default();
        let path = folder(home.path(), "notes");
        registry.add(&path, None).unwrap();

        // The same directory by another path: canonicalization removes the `.`.
        let indirect = path.join(".");
        assert!(matches!(
            registry.add(&indirect, None),
            Err(Error::DuplicateVault(_))
        ));
        assert_eq!(registry.entries().len(), 1);
    }

    #[test]
    fn nested_vaults_are_refused_both_ways() {
        let home = dir();
        let outer = folder(home.path(), "notes");
        let inner = folder(home.path(), "notes/inner");

        let mut registry = VaultRegistry::default();
        registry.add(&outer, None).unwrap();
        assert!(matches!(
            registry.add(&inner, None),
            Err(Error::NestedVault(_, _))
        ));

        let mut other = VaultRegistry::default();
        other.add(&inner, None).unwrap();
        assert!(matches!(
            other.add(&outer, None),
            Err(Error::NestedVault(_, _))
        ));
    }

    #[test]
    fn a_folder_that_is_gone_stays_in_the_list() {
        let home = dir();
        let path = folder(home.path(), "внешний диск");
        let mut registry = VaultRegistry::default();
        registry.add(&path, None).unwrap();

        fs::remove_dir_all(&path).unwrap();

        assert_eq!(registry.entries().len(), 1);
        assert!(!registry.entries()[0].available());
    }

    #[test]
    fn round_trip_through_the_file() {
        let home = dir();
        let path = VaultRegistry::path(home.path());
        let mut registry = VaultRegistry::default();
        registry.add(folder(home.path(), "notes"), None).unwrap();
        registry.add(folder(home.path(), "work"), None).unwrap();
        registry.set_active("work").unwrap();

        registry.save(&path).unwrap();
        let loaded = VaultRegistry::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.registry, registry);
        assert_eq!(loaded.registry.active().unwrap().id, "work");
        assert!(!home.path().join("vaults.json.tmp").exists());

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""version": 1"#), "{written}");
    }

    #[test]
    fn a_missing_file_is_an_empty_registry() {
        let home = dir();

        let loaded = VaultRegistry::load(&VaultRegistry::path(home.path())).unwrap();

        assert!(loaded.registry.is_empty());
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn a_broken_file_is_set_aside_and_reported() {
        let home = dir();
        let path = VaultRegistry::path(home.path());
        fs::write(&path, "{ это не json").unwrap();

        let loaded = VaultRegistry::load(&path).unwrap();

        assert!(loaded.registry.is_empty());
        assert!(loaded.problem.is_some(), "о потере списка надо сказать");
        assert!(home.path().join("vaults.json.broken").is_file());
    }

    #[test]
    fn a_hand_edited_file_is_repaired_not_trusted() {
        let home = dir();
        let path = VaultRegistry::path(home.path());
        fs::write(
            &path,
            r#"{"version":1,"active":"нет такого","vaults":[
                 {"id":"a","name":"A","path":"/tmp/a","color":"blue"},
                 {"id":"a","name":"Дубль","path":"/tmp/b","color":"red"}]}"#,
        )
        .unwrap();

        let registry = VaultRegistry::load(&path).unwrap().registry;

        assert_eq!(registry.entries().len(), 1, "дубль по id выброшен");
        assert_eq!(
            registry.active().unwrap().id,
            "a",
            "активное — первое живое"
        );
    }

    #[test]
    fn removing_the_active_vault_moves_the_mark() {
        let home = dir();
        let mut registry = VaultRegistry::default();
        registry.add(folder(home.path(), "notes"), None).unwrap();
        registry.add(folder(home.path(), "work"), None).unwrap();
        registry.set_active("work").unwrap();

        registry.remove("work").unwrap();

        assert_eq!(registry.active().unwrap().id, "notes");
        assert!(matches!(
            registry.remove("work"),
            Err(Error::UnknownVault(_))
        ));
    }

    #[test]
    fn found_by_id_name_or_path() {
        let home = dir();
        let mut registry = VaultRegistry::default();
        let path = folder(home.path(), "notes");
        registry.add(&path, Some("Работа".into())).unwrap();

        assert!(registry.find("работа").is_some());
        assert!(registry.find("работа".to_uppercase().as_str()).is_some());
        assert!(registry.find(path.to_str().unwrap()).is_some());
        assert!(registry.find("нет такого").is_none());
    }

    #[test]
    fn hotkeys_are_positions_in_the_list() {
        let home = dir();
        let mut registry = VaultRegistry::default();
        registry.add(folder(home.path(), "notes"), None).unwrap();
        registry.add(folder(home.path(), "work"), None).unwrap();

        assert_eq!(registry.by_hotkey(1).unwrap().id, "notes");
        assert_eq!(registry.by_hotkey(2).unwrap().id, "work");
        assert!(registry.by_hotkey(3).is_none());
        assert!(registry.by_hotkey(0).is_none());
    }

    #[test]
    fn the_remembered_folder_becomes_the_first_vault() {
        let home = dir();
        let notes = folder(home.path(), "notes");
        fs::write(
            home.path().join(LEGACY_FILE),
            format!("{}\n", notes.display()),
        )
        .unwrap();

        let mut registry = VaultRegistry::default();
        assert!(migrate_legacy(home.path(), &mut registry).unwrap());

        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.active().unwrap().name, "notes");
        assert!(
            !home.path().join(LEGACY_FILE).exists(),
            "второго источника правды остаться не должно"
        );
        // The second run has nothing to migrate.
        assert!(!migrate_legacy(home.path(), &mut registry).unwrap());
    }

    #[test]
    fn a_name_without_letters_still_gets_an_id() {
        assert_eq!(slug("!!!"), "vault");
        assert_eq!(slug("Мои заметки 2026"), "мои-заметки-2026");
        assert_eq!(slug("  Work — ASW  "), "work-asw");
    }
}
