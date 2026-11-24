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
        if target.standalone() && !(self.vaults.is_empty() && self.patterns.is_empty()) {
            return false;
        }
        if !self.vaults.is_empty() && !self.vaults.contains(&target.vault_id) {
            return false;
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

    pub fn load(path: &Path) -> Result<Self> {
        let json = match fs::read_to_string(path) {
            Ok(json) => json,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(defaults());
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        serde_json::from_str(&json).map_err(|error| Error::Manifest {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("the manifest serializes");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
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

    pub fn load(path: &Path) -> Result<Self> {
        match Manifest::load(path) {
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

pub fn defaults() -> Manifest {
    Manifest {
        version: VERSION,
        env: BTreeMap::new(),
        commands: vec![confluence(), agent()],
    }
}

fn confluence() -> CommandSpec {
    CommandSpec {
        id: "confluence".to_string(),
        name: "Update the Confluence page".to_string(),
        kind: "fish".to_string(),
        script: "confluence-tools replace $KKNOW_FM_CONFLUENCE \"$KKNOW_PATH\"".to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        requires: vec!["confluence".to_string()],
        login: false,
        program: None,
        cwd: Cwd::Vault,
        timeout_seconds: DEFAULT_TIMEOUT,
        confirm: false,
        env: BTreeMap::new(),
    }
}

pub const AGENT_VAR: &str = "AGENT";

pub const MODE_VAR: &str = "MODE";

fn agent() -> CommandSpec {
    CommandSpec {
        id: "agent".to_string(),
        name: "Process comments with an agent".to_string(),
        kind: "shell".to_string(),
        script: AGENT_SCRIPT.to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        requires: Vec::new(),
        login: false,
        program: None,
        cwd: Cwd::Target,
        timeout_seconds: 3600,
        confirm: true,
        env: BTreeMap::from([
            (AGENT_VAR.to_string(), "claude".to_string()),
            (MODE_VAR.to_string(), "edit".to_string()),
        ]),
    }
}

const AGENT_SCRIPT: &str = r#"PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

format="Each comment has this format: <!-- @agent «TARGET»: INSTRUCTION -->, or <!-- @agent#ID «TARGET»: INSTRUCTION --> with a thread ID.
TARGET is the text that the comment refers to. Find it just before the comment.
A comment without «TARGET» refers to its whole paragraph."

doubt="When you are not sure, do not guess.
- If the instruction is unclear, or sources disagree, ask the person.
- If there are several good ways to do it, give the person the variants to choose from.
- Do the comments that are clear, and leave the others open."

case "$MODE" in
  ""|edit)
    prompt="Process the review comments in $KKNOW_PATH.

$format

For each comment:
1. Read the instruction and the context around TARGET.
2. If you can do the change, edit the text. Keep the style and format of the document.
3. Find all related places in the document and make them consistent with the change.
   Related places are: the same term or fact, references to it, links and anchors to changed headings.
4. Remove each comment that you processed.

A comment with #ID has a thread: the file $KKNOW_VAULT/.kknow/threads/ID.json. Read it before the comment.
- \"status\": \"question\" means the person has not answered your question yet. Skip the comment.
- \"status\": \"answered\" means the last message from \"me\" answers your question. Do the task with this answer.

$doubt
To ask or to give variants, keep the comment and add just after it:
<!-- @agent-note: QUESTION --> or <!-- @agent-note: VARIANTS: 1) ... 2) ... -->.
kknow moves the note into the thread of the comment, and the person answers there.

At the end, write a short list: each change with its location, and each comment that you did not process."
    claude_tools="Read,Edit(./**)"
    copilot_tool="write($KKNOW_PATH)"
    ;;
  propose)
    prompt="Process the review comments of the note $KKNOW_NOTE in the kknow vault $KKNOW_VAULT_NAME.
Do not edit files. Work only through the tools of the kknow MCP server.

$format

1. Call agent_comments with note=$KKNOW_NOTE to get the comments and their threads.
   Skip comments with the status question or review: they wait for the person.
   A comment with the status answered has the answer of the person in its thread.
2. For each comment, call claim_comment. It gives the comment ID.
3. Read the note with read_note. Plan the change and the related places that must stay consistent.
4. Call propose_edit with the edits, a short reason and comment=ID.
   Each old text must occur exactly once in the note: take more words around it if needed.

$doubt
To ask, call reply_comment with question=true. To give variants, make one propose_edit for each variant
and say in each reason which variant it is.

At the end, write a short list: each proposal, each question, and each comment that you did not process."
    claude_tools="Read,mcp__kknow"
    copilot_tool="kknow"
    ;;
  *) echo "MODE=\"${MODE}\": set edit or propose" >&2; exit 2 ;;
esac

# The threads are in the vault root, above the folder of the note.
set --
if [ -n "$KKNOW_VAULT" ]; then set -- --add-dir "$KKNOW_VAULT"; fi

case "$AGENT" in
  claude) exec claude -p "$prompt" --allowedTools "$claude_tools" "$@" ;;
  copilot) exec copilot -p "$prompt" --silent --allow-tool "$copilot_tool" "$@" ;;
  *) echo "AGENT=\"${AGENT}\": set claude or copilot" >&2; exit 2 ;;
esac
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn target(kind: TargetKind, relative: &str, vault: &str) -> Target {
        Target {
            kind,
            path: PathBuf::from("/vault").join(relative),
            relative: relative.to_string(),
            note: None,
            fields: BTreeMap::new(),
            vault_id: vault.to_string(),
            vault_name: vault.to_string(),
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
    fn a_missing_file_gives_the_shipped_commands() {
        let dir = tempfile::tempdir().unwrap();
        let path = CommandSet::path(dir.path());

        let set = CommandSet::load(&path).unwrap();

        assert!(set.problems.is_empty(), "{:?}", set.problems);
        let ids: Vec<&str> = set.commands.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["confluence", "agent"]);
        assert_eq!(Manifest::load(&path).unwrap(), defaults());
        assert!(!path.exists(), "чтение файл не создаёт");
    }

    #[test]
    fn an_empty_file_stays_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = Manifest::path(dir.path());
        Manifest::default().save(&path).unwrap();

        assert!(CommandSet::load(&path).unwrap().is_empty());
    }

    #[test]
    fn the_agent_is_claude_unless_the_user_picks_another() {
        let set = CommandSet::from(&defaults());
        let agent = set.get("agent").unwrap();

        assert_eq!(agent.env[AGENT_VAR], "claude");
        assert_eq!(agent.env[MODE_VAR], "edit");
        assert!(agent.script.contains("claude)"));
        assert!(agent.script.contains("copilot)"));
    }

    #[cfg(unix)]
    fn run_agent(mode: &str) -> (bool, String) {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("claude");
        fs::write(
            &fake,
            "#!/bin/sh\nfor arg in \"$@\"; do echo \"[$arg]\"; done\n",
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();

        let set = CommandSet::from(&defaults());
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&set.get("agent").unwrap().script)
            .env("HOME", home.path())
            .env("AGENT", "claude")
            .env("MODE", mode)
            .env("KKNOW_PATH", "/vault/План.md")
            .env("KKNOW_NOTE", "План")
            .env("KKNOW_VAULT_NAME", "Личное")
            .env("KKNOW_VAULT", "/my vault")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        (output.status.success(), text)
    }

    #[cfg(unix)]
    #[test]
    fn the_agent_edits_the_file_by_default_and_asks_when_in_doubt() {
        let (ok, args) = run_agent("edit");

        assert!(ok, "{args}");
        assert!(args.contains("[Read,Edit(./**)]"), "{args}");
        assert!(args.contains("do not guess"), "{args}");
        assert!(args.contains("@agent-note: VARIANTS"), "{args}");
        assert!(args.contains("/my vault/.kknow/threads/ID.json"), "{args}");
        assert!(args.contains(r#""status": "answered""#), "{args}");
        assert!(args.contains("[--add-dir]\n[/my vault]"), "{args}");
    }

    #[cfg(unix)]
    #[test]
    fn in_propose_mode_the_agent_has_only_the_kknow_tools() {
        let (ok, args) = run_agent("propose");

        assert!(ok, "{args}");
        assert!(args.contains("[Read,mcp__kknow]"), "{args}");
        assert!(!args.contains("Edit("), "{args}");
        assert!(args.contains("note=План"), "{args}");
        assert!(args.contains("propose_edit"), "{args}");
        assert!(args.contains("reply_comment with question=true"), "{args}");
    }

    #[cfg(unix)]
    #[test]
    fn an_unknown_mode_is_refused() {
        let (ok, text) = run_agent("yolo");

        assert!(!ok);
        assert!(text.contains(r#"MODE="yolo""#), "{text}");
    }

    #[test]
    fn a_broken_file_is_reported_but_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = CommandSet::path(dir.path());
        fs::write(&path, "{ это не json").unwrap();

        let set = CommandSet::load(&path).unwrap();

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

        let manifest = Manifest::load(&path).unwrap();

        assert_eq!(manifest.commands.len(), 2, "в файле обе");
        assert_eq!(
            CommandSet::load(&path).unwrap().commands.len(),
            1,
            "годна одна"
        );
        assert_eq!(
            manifest.problems(),
            CommandSet::load(&path).unwrap().problems
        );

        manifest.save(&path).unwrap();
        assert_eq!(
            Manifest::load(&path).unwrap(),
            manifest,
            "запись ничего не унесла"
        );
    }

    #[test]
    fn a_broken_file_stops_the_editor_but_not_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = Manifest::path(dir.path());
        fs::write(&path, "{ это не json").unwrap();

        assert!(matches!(Manifest::load(&path), Err(Error::Manifest { .. })));
        assert_eq!(CommandSet::load(&path).unwrap().problems.len(), 1);
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = CommandSet::path(dir.path());

        defaults().save(&path).unwrap();
        let loaded = CommandSet::load(&path).unwrap();

        assert_eq!(loaded, CommandSet::from(&defaults()));
        assert!(loaded.get("confluence").is_some());
        assert!(!dir.path().join("commands.json.tmp").exists());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("\"version\": 1")
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
