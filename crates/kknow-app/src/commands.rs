//! Resolves a command target from the name given outside.
//!
//! The runner lives in `kknow-run` and knows nothing about notes. This module
//! does the reverse step: a string from a menu or a CLI argument becomes a
//! [`Target`] with an absolute path. The code is shared because the window and
//! the CLI must resolve targets the same way.

use std::path::{Path, PathBuf};

use kknow_run::{CommandSet, Target, TargetKind};

use crate::{Error, Result, Workspace};

/// The commands file is next to the vault registry: it is a machine setting,
/// not a notes directory setting. See `kknow_run::spec` for the reason.
pub fn config_path() -> Option<PathBuf> {
    crate::vaults::config_dir().map(CommandSet::path)
}

/// Commands of this machine.
pub fn load() -> Result<CommandSet> {
    let Some(path) = config_path() else {
        return Ok(CommandSet::default());
    };
    Ok(CommandSet::load(&path)?)
}

/// The whole vault.
pub fn vault_target(workspace: &Workspace, id: &str, name: &str) -> Target {
    Target::vault(id, name, workspace.root())
}

/// A note by ID, in the same form that `kknow show` and the window accept.
pub fn note_target(workspace: &Workspace, id: &str, name: &str, raw: &str) -> Result<Target> {
    let note = workspace
        .find(raw)
        .ok_or_else(|| Error::UnknownTarget(raw.to_string()))?
        .clone();

    // Note metadata is how a note passes parameters to its commands: a wiki
    // page number, a space name, anything else. Frontmatter and comments are
    // the two sources, but parsing already merged them. They come from the
    // parsed note, so there is no disk read here.
    let fields = workspace
        .parsed(&note)
        .map(|parsed| parsed.fields.clone())
        .unwrap_or_default();

    Ok(Target {
        kind: TargetKind::Note,
        path: workspace.path_for(&note),
        // `match` patterns describe files, so compare with the file path.
        // Otherwise `**/*.md` matches nothing.
        relative: format!("{note}.md"),
        note: Some(note.as_str().to_string()),
        fields,
        vault_id: id.to_string(),
        vault_name: name.to_string(),
        vault_root: workspace.root().to_path_buf(),
    })
}

/// A folder inside the notes directory; the path is relative to its root.
pub fn folder_target(workspace: &Workspace, id: &str, name: &str, raw: &str) -> Result<Target> {
    let relative = relative_folder(raw).ok_or_else(|| Error::UnknownTarget(raw.to_string()))?;
    let path = workspace.root().join(&relative);

    if !inside(workspace.root(), &path) {
        return Err(Error::UnknownTarget(raw.to_string()));
    }

    Ok(Target {
        kind: TargetKind::Folder,
        path,
        relative,
        note: None,
        // Frontmatter belongs to a note, not to a folder. Collecting it from all
        // notes inside would need a rule for which note wins.
        fields: Default::default(),
        vault_id: id.to_string(),
        vault_name: name.to_string(),
        vault_root: workspace.root().to_path_buf(),
    })
}

/// A standalone file: markdown opened without a vault.
///
/// The caller checks that the person opened this file (the window keeps the
/// list). Here the file is read only for its metadata: a `README.md` passes a
/// page number to its command the same way a note does.
///
/// The path becomes absolute: the CLI gets `README.md` as typed, and the
/// script needs a full `$KKNOW_PATH` and a real folder to start in.
pub fn file_target(path: &Path) -> Result<Target> {
    let path = path
        .canonicalize()
        .map_err(|source| Error::io(path, source))?;
    let content = std::fs::read_to_string(&path).map_err(|source| Error::io(&path, source))?;
    Ok(Target::file(path, kknow_index::parse(&content).fields))
}

/// Target by kind and string. Without a kind, guess the way the CLI needs.
///
/// Notes come first: `kknow run push Docs` usually means the note `Docs`, not
/// the folder with the same name. Use an explicit `--folder` to resolve this.
pub fn resolve(
    workspace: &Workspace,
    id: &str,
    name: &str,
    kind: Option<TargetKind>,
    raw: Option<&str>,
) -> Result<Target> {
    match (kind, raw) {
        (Some(TargetKind::Vault), _) | (None, None) => Ok(vault_target(workspace, id, name)),
        (Some(TargetKind::Note), Some(raw)) => note_target(workspace, id, name, raw),
        (Some(TargetKind::Folder), Some(raw)) => folder_target(workspace, id, name, raw),
        (Some(_), None) => Err(Error::UnknownTarget(String::new())),
        (None, Some(raw)) => note_target(workspace, id, name, raw)
            .or_else(|_| folder_target(workspace, id, name, raw)),
    }
}

/// Folder path from the root: no `..`, no absolute paths, no empty parts.
///
/// The string comes from the window, which draws the tree from core data. We
/// still check it: the command gets this path as its working directory, and
/// `../..` would send it outside the vault.
fn relative_folder(raw: &str) -> Option<String> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return None;
    }

    let parts: Vec<&str> = raw
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();

    if parts.is_empty() || parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

/// Checks that the path is inside the root and is a directory.
///
/// Compare canonical paths: a symlink that points outside looks nested until
/// the system resolves it.
fn inside(root: &Path, path: &Path) -> bool {
    let (Ok(root), Ok(path)) = (root.canonicalize(), path.canonicalize()) else {
        return false;
    };
    path.is_dir() && path.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Docs/Внутри")).unwrap();
        fs::create_dir_all(dir.path().join("Пустая")).unwrap();
        fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();
        fs::write(
            dir.path().join("Docs/Внутри/Свод.md"),
            "---\nconfluence: 123456789\n---\n\n# Свод\n",
        )
        .unwrap();

        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    #[test]
    fn a_note_target_knows_its_file_its_id_and_its_relative_path() {
        let (_dir, workspace) = workspace();

        let target = note_target(&workspace, "work", "Работа", "Docs/Внутри/Свод").unwrap();

        assert_eq!(target.kind, TargetKind::Note);
        assert_eq!(target.path, workspace.root().join("Docs/Внутри/Свод.md"));
        assert_eq!(target.relative, "Docs/Внутри/Свод.md");
        assert_eq!(target.note.as_deref(), Some("Docs/Внутри/Свод"));
        assert_eq!(target.vault_root, workspace.root());
    }

    #[test]
    fn a_short_name_finds_the_note_the_same_way_the_cli_does() {
        let (_dir, workspace) = workspace();

        let target = note_target(&workspace, "work", "Работа", "Свод").unwrap();

        assert_eq!(target.note.as_deref(), Some("Docs/Внутри/Свод"));
    }

    /// A command parameter belongs to the file, not to the manifest: the page
    /// number is in the note itself and goes from there to the command.
    #[test]
    fn a_note_brings_its_frontmatter_fields_along() {
        let (_dir, workspace) = workspace();

        let with = note_target(&workspace, "work", "Работа", "Свод").unwrap();
        let without = note_target(&workspace, "work", "Работа", "Index").unwrap();

        assert_eq!(with.fields["confluence"], "123456789");
        assert!(without.fields.is_empty());
    }

    #[test]
    fn a_folder_target_may_be_empty_of_notes() {
        let (_dir, workspace) = workspace();

        let target = folder_target(&workspace, "work", "Работа", "Пустая").unwrap();

        assert_eq!(target.kind, TargetKind::Folder);
        assert_eq!(target.relative, "Пустая");
        assert!(target.note.is_none());
    }

    #[test]
    fn a_folder_cannot_lead_out_of_the_vault() {
        let (_dir, workspace) = workspace();

        for raw in ["..", "Docs/../..", "/etc", "", "нет такой"] {
            assert!(
                folder_target(&workspace, "work", "Работа", raw).is_err(),
                "{raw}"
            );
        }
        // A note is not a folder either: target kinds do not replace each other.
        assert!(folder_target(&workspace, "work", "Работа", "Index.md").is_err());
    }

    #[test]
    fn a_vault_target_is_the_root_itself() {
        let (_dir, workspace) = workspace();

        let target = vault_target(&workspace, "work", "Работа");

        assert_eq!(target.kind, TargetKind::Vault);
        assert_eq!(target.path, workspace.root());
        assert_eq!(target.relative, "");
    }

    #[test]
    fn a_standalone_file_brings_its_fields_and_has_no_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.md");
        fs::write(&path, "# Проект\n\n<!-- confluence: 987654321 -->\n").unwrap();

        let target = file_target(&path).unwrap();

        assert_eq!(target.kind, TargetKind::Note);
        assert_eq!(target.path, path.canonicalize().unwrap());
        assert!(target.standalone());
        assert_eq!(target.fields["confluence"], "987654321");
        assert!(file_target(&dir.path().join("нет.md")).is_err());
    }

    #[test]
    fn guessing_prefers_a_note_and_falls_back_to_a_folder() {
        let (_dir, workspace) = workspace();

        let guessed = |raw: Option<&str>| {
            resolve(&workspace, "work", "Работа", None, raw).map(|target| target.kind)
        };

        assert_eq!(guessed(Some("Index")).unwrap(), TargetKind::Note);
        assert_eq!(guessed(Some("Docs")).unwrap(), TargetKind::Folder);
        assert_eq!(guessed(None).unwrap(), TargetKind::Vault);
        assert!(guessed(Some("ни то ни сё")).is_err());
    }

    #[test]
    fn an_explicit_kind_is_not_guessed_around() {
        let (_dir, workspace) = workspace();

        // `Docs` is a folder, so it must not resolve as a note: the target kind
        // decides which commands apply to it.
        assert!(
            resolve(
                &workspace,
                "work",
                "Работа",
                Some(TargetKind::Note),
                Some("Docs")
            )
            .is_err()
        );
    }
}
