use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::spec::Cwd;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TargetKind {
    Note,
    Folder,
    Vault,
}

impl TargetKind {
    pub const ALL: [TargetKind; 3] = [TargetKind::Note, TargetKind::Folder, TargetKind::Vault];

    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Note => "note",
            TargetKind::Folder => "folder",
            TargetKind::Vault => "vault",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        TargetKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    pub path: PathBuf,
    pub relative: String,
    pub note: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub vault: Option<VaultRef>,
    pub vault_root: PathBuf,
}

impl Target {
    pub fn vault(id: &str, name: &str, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Target {
            kind: TargetKind::Vault,
            path: root.clone(),
            relative: String::new(),
            note: None,
            fields: BTreeMap::new(),
            vault: Some(VaultRef {
                id: id.to_string(),
                name: name.to_string(),
            }),
            vault_root: root,
        }
    }

    pub fn file(path: impl Into<PathBuf>, fields: BTreeMap<String, String>) -> Self {
        let path = path.into();
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Target {
            kind: TargetKind::Note,
            relative: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path,
            note: None,
            fields,
            vault: None,
            vault_root: folder,
        }
    }

    pub fn standalone(&self) -> bool {
        self.vault.is_none()
    }

    pub(crate) fn working_dir(&self, cwd: Cwd) -> &Path {
        match cwd {
            Cwd::Vault => &self.vault_root,
            Cwd::Target => match self.kind {
                TargetKind::Note => self.path.parent().unwrap_or(&self.vault_root),
                _ => &self.path,
            },
        }
    }
}

pub fn environment(target: &Target) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let mut put = |name: &str, value: String| {
        env.insert(name.to_string(), value);
    };

    put("KKNOW_PATH", target.path.to_string_lossy().into_owned());
    put("KKNOW_TARGET", target.kind.as_str().to_string());
    put("KKNOW_RELATIVE", target.relative.clone());
    if let Some(vault) = &target.vault {
        put(
            "KKNOW_VAULT",
            target.vault_root.to_string_lossy().into_owned(),
        );
        put("KKNOW_VAULT_ID", vault.id.clone());
        put("KKNOW_VAULT_NAME", vault.name.clone());
    }
    if let Some(note) = &target.note {
        put("KKNOW_NOTE", note.clone());
    }

    for (key, value) in &target.fields {
        if let Some(name) = variable_name(key) {
            put(&name, value.clone());
        }
    }

    env
}

fn variable_name(key: &str) -> Option<String> {
    let mut name = String::new();
    for symbol in key.chars() {
        if symbol.is_ascii_alphanumeric() {
            name.push(symbol.to_ascii_uppercase());
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }

    let name = name.trim_matches('_');
    if name.is_empty() || name.starts_with(|symbol: char| symbol.is_ascii_digit()) {
        return None;
    }
    Some(format!("KKNOW_FM_{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::run;
    use crate::test_support::{note, spec};

    #[test]
    fn the_vault_variable_is_the_one_the_cli_reads() {
        let dir = tempfile::tempdir().unwrap();
        let target = note(dir.path());

        let env = environment(&target);

        assert_eq!(env["KKNOW_VAULT"], dir.path().to_string_lossy());
        assert_eq!(env["KKNOW_VAULT_ID"], "work");
        assert_eq!(env["KKNOW_VAULT_NAME"], "Работа");
        assert_eq!(env["KKNOW_RELATIVE"], "Docs/Index.md");
    }

    #[test]
    fn a_standalone_file_runs_in_its_folder_without_a_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.md");
        std::fs::write(&path, "# Проект\n").unwrap();
        let target = Target::file(&path, BTreeMap::new());

        assert!(target.vault.is_none());
        let env = environment(&target);
        assert_eq!(env["KKNOW_TARGET"], "note");
        assert_eq!(env["KKNOW_RELATIVE"], "README.md");
        for absent in [
            "KKNOW_VAULT",
            "KKNOW_VAULT_ID",
            "KKNOW_VAULT_NAME",
            "KKNOW_NOTE",
        ] {
            assert!(!env.contains_key(absent), "{absent}");
        }

        let outcome = run(&spec("printf x > где-я"), &target).unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        assert!(
            dir.path().join("где-я").exists(),
            "команда начала работу не в папке файла"
        );
    }

    #[test]
    fn a_vault_target_has_no_note_variable() {
        let target = Target::vault("work", "Работа", "/заметки");

        let env = environment(&target);

        assert!(!env.contains_key("KKNOW_NOTE"));
        assert_eq!(env["KKNOW_RELATIVE"], "");
        assert_eq!(env["KKNOW_TARGET"], "vault");
    }
}
