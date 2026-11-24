use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::glob;
use crate::{Error, Result, Target, TargetKind};

pub const COMMANDS_FILE: &str = "commands.json";

pub const VERSION: u32 = 1;

pub const DEFAULT_TIMEOUT: u64 = 60;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "CommandCwd"))]
pub enum Cwd {
    #[default]
    Vault,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CommandSpec {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub script: String,

    #[serde(default = "all_targets")]
    pub targets: Vec<TargetKind>,

    #[serde(default)]
    pub vaults: Vec<String>,

    #[serde(default, rename = "match")]
    pub patterns: Vec<String>,

    #[serde(default)]
    pub requires: Vec<String>,

    #[serde(default)]
    pub login: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub program: Option<String>,

    #[serde(default)]
    pub cwd: Cwd,

    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default)]
    pub confirm: bool,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<BTreeMap<String, String>>", optional))]
    pub env: BTreeMap<String, String>,
}

fn all_targets() -> Vec<TargetKind> {
    TargetKind::ALL.to_vec()
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT
}

impl CommandSpec {
    pub fn applies_to(&self, target: &Target) -> bool {
        if !self.targets.contains(&target.kind) {
            return false;
        }
        match &target.vault {
            None if !(self.vaults.is_empty() && self.patterns.is_empty()) => return false,
            Some(vault) if !self.vaults.is_empty() && !self.vaults.contains(&vault.id) => {
                return false;
            }
            _ => {}
        }
        if !self.requires.iter().all(|key| {
            target
                .fields
                .get(key)
                .is_some_and(|value| !value.is_empty())
        }) {
            return false;
        }
        if self.patterns.is_empty() || target.kind == TargetKind::Vault {
            return true;
        }
        self.patterns
            .iter()
            .any(|pattern| glob::matches(pattern, &target.relative))
    }

    fn problems(&self) -> Vec<Problem> {
        let id = || self.id.clone();
        let mut problems = Vec::new();

        if self.name.trim().is_empty() {
            problems.push(Problem::NoName { id: id() });
        }
        if self.script.trim().is_empty() {
            problems.push(Problem::EmptyScript { id: id() });
        }
        if crate::runner::for_kind(&self.kind).is_none() {
            problems.push(Problem::UnknownKind {
                id: id(),
                kind: self.kind.clone(),
                known: crate::runner::kinds().join(", "),
            });
        }
        if self.targets.is_empty() {
            problems.push(Problem::NoTargets { id: id() });
        }

        problems
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "CommandProblem"))]
pub enum Problem {
    File {
        path: String,
        detail: String,
    },
    NoId,
    DuplicateId {
        id: String,
    },
    NoName {
        id: String,
    },
    EmptyScript {
        id: String,
    },
    UnknownKind {
        id: String,
        kind: String,
        known: String,
    },
    NoTargets {
        id: String,
    },
}

impl Problem {
    pub fn id(&self) -> Option<&str> {
        match self {
            Problem::File { .. } | Problem::NoId => None,
            Problem::DuplicateId { id }
            | Problem::NoName { id }
            | Problem::EmptyScript { id }
            | Problem::UnknownKind { id, .. }
            | Problem::NoTargets { id } => Some(id),
        }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::File { path, detail } => write!(f, "{path}: {detail}"),
            Problem::NoId => write!(f, "a command without id is skipped"),
            Problem::DuplicateId { id } => {
                write!(f, "{id}: this id is already used, the entry is skipped")
            }
            Problem::NoName { id } => write!(f, "{id}: no name"),
            Problem::EmptyScript { id } => write!(f, "{id}: the script is empty"),
            Problem::UnknownKind { id, kind, known } => {
                write!(f, "{id}: unknown kind \"{kind}\"; known kinds: {known}")
            }
            Problem::NoTargets { id } => {
                write!(f, "{id}: targets is empty, so the command shows nowhere")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            version: VERSION,
            env: BTreeMap::new(),
            commands: Vec::new(),
        }
    }
}

fn current_version() -> u32 {
    VERSION
}

impl Manifest {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(COMMANDS_FILE)
    }

    pub fn load(path: &Path, defaults: Manifest) -> Result<Self> {
        let json = match fs::read_to_string(path) {
            Ok(json) => json,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(defaults);
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        serde_json::from_str(&json).map_err(|error| Error::Manifest {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
    }

    pub fn problems(&self) -> Vec<Problem> {
        sift(&self.commands).1
    }
}

fn sift(commands: &[CommandSpec]) -> (Vec<&CommandSpec>, Vec<Problem>) {
    let mut good: Vec<&CommandSpec> = Vec::new();
    let mut problems = Vec::new();

    for spec in commands {
        let issues = spec.problems();

        if spec.id.trim().is_empty() {
            problems.push(Problem::NoId);
        } else if good.iter().any(|other| other.id == spec.id) {
            problems.push(Problem::DuplicateId {
                id: spec.id.clone(),
            });
        } else if issues.is_empty() {
            good.push(spec);
        } else {
            problems.extend(issues);
        }
    }

    (good, problems)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSet {
    pub commands: Vec<CommandSpec>,
    pub env: BTreeMap<String, String>,
    pub problems: Vec<Problem>,
}

impl From<&Manifest> for CommandSet {
    fn from(manifest: &Manifest) -> Self {
        let (commands, problems) = sift(&manifest.commands);
        CommandSet {
            commands: commands.into_iter().cloned().collect(),
            env: manifest.env.clone(),
            problems,
        }
    }
}

impl CommandSet {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        Manifest::path(config_dir)
    }

    pub fn load(path: &Path, defaults: Manifest) -> Result<Self> {
        match Manifest::load(path, defaults) {
            Ok(manifest) => Ok(CommandSet::from(&manifest)),
            Err(Error::Manifest { path, reason }) => Ok(CommandSet {
                problems: vec![Problem::File {
                    path: path.display().to_string(),
                    detail: reason,
                }],
                ..CommandSet::default()
            }),
            Err(other) => Err(other),
        }
    }

    pub fn parse(json: &str) -> std::result::Result<Self, String> {
        let manifest: Manifest = serde_json::from_str(json).map_err(|error| error.to_string())?;
        Ok(CommandSet::from(&manifest))
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|spec| spec.id == id)
    }

    pub fn for_target(&self, target: &Target) -> Vec<&CommandSpec> {
        self.commands
            .iter()
            .filter(|spec| spec.applies_to(target))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn run(&self, id: &str, target: &Target) -> Result<crate::Outcome> {
        let spec = self.get(id).ok_or_else(|| Error::Unknown(id.to_string()))?;
        crate::run_with(spec, target, &self.env)
    }

    pub fn run_watched(
        &self,
        id: &str,
        target: &Target,
        watch: &crate::Watch<'_>,
    ) -> Result<crate::Outcome> {
        let spec = self.get(id).ok_or_else(|| Error::Unknown(id.to_string()))?;
        crate::run_watched(spec, target, &self.env, watch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VaultRef;

    fn target(kind: TargetKind, relative: &str, vault: &str) -> Target {
        Target {
            kind,
            path: PathBuf::from("/vault").join(relative),
            relative: relative.to_string(),
            note: None,
            fields: BTreeMap::new(),
            vault: Some(VaultRef {
                id: vault.to_string(),
                name: vault.to_string(),
            }),
            vault_root: PathBuf::from("/vault"),
        }
    }

    fn set(json: &str) -> CommandSet {
        CommandSet::parse(json).expect("манифест разобран")
    }

    #[test]
    fn a_minimal_command_applies_everywhere() {
        let parsed = set(r#"{"commands":[
                 {"id":"push","name":"Отправить","kind":"shell","script":"echo $KKNOW_PATH"}]}"#);

        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let spec = &parsed.commands[0];
        assert_eq!(spec.targets, TargetKind::ALL, "по умолчанию — на всё");
        assert_eq!(spec.timeout_seconds, DEFAULT_TIMEOUT);
        assert_eq!(spec.cwd, Cwd::Vault);

        for kind in TargetKind::ALL {
            assert!(spec.applies_to(&target(kind, "Docs/Index.md", "work")));
        }
    }

    #[test]
    fn targets_vaults_and_patterns_narrow_it_down() {
        let spec = &set(r#"{"commands":[{
                 "id":"push","name":"Отправить","kind":"shell","script":"…",
                 "targets":["note"],"vaults":["work"],"match":["Docs/**"]}]}"#)
        .commands[0];

        assert!(spec.applies_to(&target(TargetKind::Note, "Docs/Index.md", "work")));
        assert!(
            !spec.applies_to(&target(TargetKind::Folder, "Docs", "work")),
            "вид цели не тот"
        );
        assert!(
            !spec.applies_to(&target(TargetKind::Note, "Docs/Index.md", "дом")),
            "хранилище не то"
        );
        assert!(
            !spec.applies_to(&target(TargetKind::Note, "Дневник/2026.md", "work")),
            "путь не тот"
        );
    }

    #[test]
    fn a_vault_is_never_narrowed_by_a_path() {
        let spec = &set(r#"{"commands":[{
                 "id":"push","name":"Отправить","kind":"shell","script":"…",
                 "match":["Docs/**"]}]}"#)
        .commands[0];

        assert!(spec.applies_to(&target(TargetKind::Vault, "", "work")));
        assert!(!spec.applies_to(&target(TargetKind::Note, "Index.md", "work")));
    }

    #[test]
    fn a_standalone_file_gets_only_the_unscoped_commands() {
        let parsed = set(r#"{"commands":[
                 {"id":"all","name":"Везде","kind":"shell","script":"…"},
                 {"id":"notes","name":"Заметки","kind":"shell","script":"…","targets":["note"]},
                 {"id":"folders","name":"Папки","kind":"shell","script":"…","targets":["folder"]},
                 {"id":"work","name":"Работа","kind":"shell","script":"…","vaults":["work"]},
                 {"id":"docs","name":"Docs","kind":"shell","script":"…","match":["**/*.md"]},
                 {"id":"wiki","name":"Вики","kind":"shell","script":"…","requires":["confluence"]}]}"#);
        let mut file = Target::file("/Users/me/README.md", BTreeMap::new());

        let ids = |file: &Target| -> Vec<String> {
            parsed
                .for_target(file)
                .into_iter()
                .map(|spec| spec.id.clone())
                .collect()
        };

        assert!(file.standalone());
        assert_eq!(ids(&file), ["all", "notes"]);

        file.fields
            .insert("confluence".to_string(), "123456789".to_string());
        assert_eq!(ids(&file), ["all", "notes", "wiki"]);
    }

    #[test]
    fn a_broken_command_is_a_complaint_not_a_lost_file() {
        let parsed = set(r#"{"commands":[
                 {"id":"a","name":"A","kind":"phyton","script":"…"},
                 {"id":"b","name":"","kind":"shell","script":"…"},
                 {"id":"c","name":"C","kind":"shell","script":"   "},
                 {"id":"","name":"D","kind":"shell","script":"…"},
                 {"id":"e","name":"E","kind":"shell","script":"echo"},
                 {"id":"e","name":"Дубль","kind":"shell","script":"echo"}]}"#);

        let ids: Vec<&str> = parsed.commands.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["e"], "живой осталась только годная команда");
        assert_eq!(parsed.problems.len(), 5, "{:?}", parsed.problems);
        assert!(
            matches!(&parsed.problems[0], Problem::UnknownKind { kind, .. } if kind == "phyton"),
            "{:?}",
            parsed.problems
        );
    }

    #[test]
    fn a_missing_file_gives_the_given_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = CommandSet::path(dir.path());
        let given = Manifest {
            commands: set(
                r#"{"commands":[{"id":"one","name":"One","kind":"shell","script":"true"}]}"#,
            )
            .commands,
            ..Manifest::default()
        };

        let loaded = CommandSet::load(&path, given.clone()).unwrap();

        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        let ids: Vec<&str> = loaded.commands.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["one"]);
        assert_eq!(Manifest::load(&path, given.clone()).unwrap(), given);
        assert!(!path.exists(), "чтение файл не создаёт");
    }

    #[test]
    fn an_empty_file_stays_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = Manifest::path(dir.path());
        fs::write(&path, r#"{"version":1,"commands":[]}"#).unwrap();

        assert!(
            CommandSet::load(&path, Manifest::default())
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[cfg(unix)]
    #[cfg(unix)]
    #[cfg(unix)]
    #[test]
    fn a_broken_file_is_reported_but_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = CommandSet::path(dir.path());
        fs::write(&path, "{ это не json").unwrap();

        let set = CommandSet::load(&path, Manifest::default()).unwrap();

        assert!(set.is_empty());
        assert_eq!(set.problems.len(), 1);
        assert!(path.is_file());
        assert!(!dir.path().join("commands.json.broken").exists());
    }

    #[test]
    fn the_manifest_keeps_what_the_set_throws_away() {
        let dir = tempfile::tempdir().unwrap();
        let path = Manifest::path(dir.path());
        fs::write(
            &path,
            r#"{"commands":[
                 {"id":"a","name":"A","kind":"phyton","script":"echo"},
                 {"id":"b","name":"B","kind":"shell","script":"echo"}]}"#,
        )
        .unwrap();

        let manifest = Manifest::load(&path, Manifest::default()).unwrap();

        assert_eq!(manifest.commands.len(), 2, "в файле обе");
        assert_eq!(
            CommandSet::load(&path, Manifest::default())
                .unwrap()
                .commands
                .len(),
            1,
            "годна одна"
        );
        assert_eq!(
            manifest.problems(),
            CommandSet::load(&path, Manifest::default())
                .unwrap()
                .problems
        );

        fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        assert_eq!(
            Manifest::load(&path, Manifest::default()).unwrap(),
            manifest,
            "запись ничего не унесла"
        );
    }

    #[test]
    fn a_broken_file_stops_the_editor_but_not_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = Manifest::path(dir.path());
        fs::write(&path, "{ это не json").unwrap();

        assert!(matches!(
            Manifest::load(&path, Manifest::default()),
            Err(Error::Manifest { .. })
        ));
        assert_eq!(
            CommandSet::load(&path, Manifest::default())
                .unwrap()
                .problems
                .len(),
            1
        );
    }

    #[test]
    fn for_target_keeps_the_order_of_the_file() {
        let parsed = set(r#"{"commands":[
                 {"id":"a","name":"A","kind":"shell","script":"…","targets":["note"]},
                 {"id":"b","name":"B","kind":"shell","script":"…","targets":["vault"]},
                 {"id":"c","name":"C","kind":"shell","script":"…"}]}"#);

        let note = target(TargetKind::Note, "Index.md", "work");
        let ids: Vec<&str> = parsed
            .for_target(&note)
            .iter()
            .map(|spec| spec.id.as_str())
            .collect();

        assert_eq!(ids, ["a", "c"]);
    }
}
