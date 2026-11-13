//! The command manifest: which commands attach to which targets.
//!
//! There is one file per app, `commands.json` next to `vaults.json`, not one
//! in the vault directory. The reason is security, not convenience. A command
//! contains executable code, and a vault directory can come through sync,
//! from another machine or from a shared folder. A theme from a foreign vault
//! can only change colors, and its values are checked. A command from a
//! foreign vault would run with the app's permissions. So each command
//! declares its own scope (`vaults`, `match`), and only a user with access to
//! this machine's settings can add one.
//!
//! ```json
//! {
//!   "version": 1,
//!   "commands": [
//!     {
//!       "id": "confluence",
//!       "name": "Send to Confluence",
//!       "kind": "shell",
//!       "script": "md2confluence --file \"$KKNOW_PATH\"",
//!       "targets": ["note", "folder"],
//!       "vaults": ["work"],
//!       "match": ["Docs/**"]
//!     }
//!   ]
//! }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::glob;
use crate::{Error, Result, Target, TargetKind};

/// The commands file name inside the settings directory.
pub const COMMANDS_FILE: &str = "commands.json";

pub const VERSION: u32 = 1;

/// Command timeout in seconds when the manifest does not set one.
pub const DEFAULT_TIMEOUT: u64 = 60;

/// The working directory of a command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "CommandCwd"))]
pub enum Cwd {
    /// The vault root. This is the default: scripts usually look at the whole
    /// directory, and `$KKNOW_PATH` gives the target itself.
    #[default]
    Vault,
    /// The target folder: for a note, its parent folder; for a folder, itself.
    Target,
}

/// One command as written in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CommandSpec {
    /// Stable name. The CLI and the app call the command by it.
    pub id: String,
    /// Menu label.
    pub name: String,
    /// Runner kind: `shell`, `bash`, `fish`, `zsh`, `python`.
    pub kind: String,
    /// Script text.
    ///
    /// The target path is **not** inserted into it. It comes as an environment
    /// variable. See [`crate::run`] for the reason.
    pub script: String,

    /// Target kinds for the command. The default is all kinds: a manifest with
    /// four fields must work, and the user can narrow the scope later.
    #[serde(default = "all_targets")]
    pub targets: Vec<TargetKind>,

    /// Vault IDs; empty means all vaults.
    #[serde(default)]
    pub vaults: Vec<String>,

    /// Path patterns from the vault root; empty means any path.
    #[serde(default, rename = "match")]
    pub patterns: Vec<String>,

    /// Note metadata fields that the command needs to appear.
    ///
    /// This binds a command parameter to a file, not to the manifest. A note
    /// with `confluence: 123456789` (in frontmatter or a comment) gets the
    /// "Send to Confluence" item, and other notes do not. An empty value counts
    /// as missing: `confluence:` without a number does not match.
    #[serde(default)]
    pub requires: Vec<String>,

    /// Read the shell profile before the run.
    ///
    /// An app opened from Finder gets `PATH` from launchd, not from a shell,
    /// and that `PATH` is minimal. The flag uses `sh -lc` instead of `sh -c`.
    /// The default is off: the profile takes time and can print text into the
    /// same stream that we show as command output.
    #[serde(default)]
    pub login: bool,

    /// Overrides the default program: python from a virtual environment, a
    /// custom build tool, any program from `PATH` or a full path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub program: Option<String>,

    #[serde(default)]
    pub cwd: Cwd,

    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Ask before the run. Only the app does this; the CLI has nobody to ask.
    #[serde(default)]
    pub confirm: bool,

    /// Extra environment variables for the process.
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
    /// Checks if the command applies to this target.
    ///
    /// There are three optional conditions: target kind, vault and path. An
    /// empty condition does not narrow anything. A command without `vaults`
    /// and `match` applies to everything, which is a good default for a first
    /// command. This includes standalone files: they are notes.
    ///
    /// A standalone file is in no vault and has no vault root, so a command
    /// with `vaults` or `match` is not for it. These fields mean "only there".
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
        // Only notes have frontmatter fields, so `requires` also means
        // "this command is for notes".
        if !self.requires.iter().all(|key| {
            target
                .fields
                .get(key)
                .is_some_and(|value| !value.is_empty())
        }) {
            return false;
        }
        // A path does not narrow a whole vault: `match` is about its contents.
        if self.patterns.is_empty() || target.kind == TargetKind::Vault {
            return true;
        }
        self.patterns
            .iter()
            .any(|pattern| glob::matches(pattern, &target.relative))
    }

    /// Problems in the command. Empty means the command can run.
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

/// What is wrong with the commands file or with one of its entries.
///
/// It is data, not a ready text: the CLI prints `Display`, and the window
/// translates `problem.command.<code>` with the fields, in its own language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "CommandProblem"))]
pub enum Problem {
    /// The file does not parse.
    File {
        path: String,
        detail: String,
    },
    /// An entry without an ID is skipped.
    NoId,
    /// A second entry with a known ID is skipped.
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
    /// The command that the problem is about, if it has an ID.
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
            Problem::DuplicateId { id } => write!(f, "{id}: this id is already used, the entry is skipped"),
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

/// The manifest as stored in the file.
///
/// It is public for the settings form. The form edits the file and needs
/// **all** entries, including invalid ones. [`CommandSet`] does not work for
/// this because it drops invalid entries by design. A command with a typo
/// would disappear from the form, and the first save would remove it from
/// the file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default = "current_version")]
    pub version: u32,
    /// Environment shared by all commands in the file.
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

    /// Reads the file and drops nothing. A missing file gives [`defaults`].
    ///
    /// An unclosed bracket gives [`Error::Manifest`], not an empty result. A
    /// person writes this file, and the editor must learn that the read failed.
    /// Otherwise the form would show nothing, and a save would erase the file.
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

    /// Writes through a temporary file, like the vault registry and settings.
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

    /// Problems in the entries; the same list that [`CommandSet`] reports.
    ///
    /// The form shows each problem next to its field. It keeps and edits the
    /// invalid entry instead of losing it.
    pub fn problems(&self) -> Vec<Problem> {
        sift(&self.commands).1
    }
}

/// Splits entries into valid commands and problems with the rest.
///
/// This is the only place in the crate for it: the set takes the commands, the
/// settings form takes the problems, so the two cannot disagree.
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

/// Valid commands of this machine, and problems with the invalid entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSet {
    pub commands: Vec<CommandSpec>,
    /// Environment shared by all commands in the file.
    ///
    /// Put here what all commands need: `PATH` with extra directories, a
    /// company server address, a default space name. A command's own `env`
    /// goes on top of this.
    pub env: BTreeMap<String, String>,
    /// What was dropped and why. A command must not disappear silently: a
    /// person wrote it and will look for it.
    pub problems: Vec<Problem>,
}

/// Keeps the valid commands; invalid entries become problems.
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

    /// Reads the file. A missing file gives the commands of [`defaults`].
    ///
    /// Unlike the vault registry, a broken file is **not** moved aside. The app
    /// writes the registry, but a person writes this file, and an unclosed
    /// bracket must not cost them their edit. Instead, we return a problem and
    /// leave the file in place.
    pub fn load(path: &Path) -> Result<Self> {
        match Manifest::load(path) {
            Ok(manifest) => Ok(CommandSet::from(&manifest)),
            // A broken file gives a problem and no commands, but not an error:
            // the app and the CLI must keep working, and the author fixes the
            // file.
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

    /// Parses a manifest. An invalid entry becomes a problem; it does not
    /// remove all commands.
    pub fn parse(json: &str) -> std::result::Result<Self, String> {
        let manifest: Manifest = serde_json::from_str(json).map_err(|error| error.to_string())?;
        Ok(CommandSet::from(&manifest))
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|spec| spec.id == id)
    }

    /// Commands that apply to this target, in file order.
    pub fn for_target(&self, target: &Target) -> Vec<&CommandSpec> {
        self.commands
            .iter()
            .filter(|spec| spec.applies_to(target))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Runs a command of the set by its ID.
    ///
    /// This is the normal entry point for the app and the CLI. Only here does
    /// the command get the shared file environment. A direct [`crate::run`]
    /// call would silently lose it.
    pub fn run(&self, id: &str, target: &Target) -> Result<crate::Outcome> {
        let spec = self.get(id).ok_or_else(|| Error::Unknown(id.to_string()))?;
        crate::run_with(spec, target, &self.env)
    }

    /// [`Self::run`] that shows the output as it comes and can be stopped.
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

/// The commands that ship with kknow.
///
/// A missing file gives these, not an empty list: a new user gets useful
/// commands at once and sees how to write their own. Once the file exists,
/// it is the only source. A user who deletes the commands in the form gets an
/// empty file, and the defaults do not come back.
pub fn defaults() -> Manifest {
    Manifest {
        version: VERSION,
        // Empty: a guessed `PATH` here would be true only on one machine.
        env: BTreeMap::new(),
        commands: vec![confluence(), agent()],
    }
}

fn confluence() -> CommandSpec {
    CommandSpec {
        id: "confluence".to_string(),
        name: "Update the Confluence page".to_string(),
        // fish reads its config on every start, so the command sees the
        // same PATH as the terminal. For sh and zsh, use `login`.
        kind: "fish".to_string(),
        // The page number comes from the note: the line
        // `confluence: 123456789` in its frontmatter, or the comment
        // `<!-- confluence: 123456789 -->` in its body.
        script: "confluence-tools replace $KKNOW_FM_CONFLUENCE \"$KKNOW_PATH\"".to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        // So the menu item appears only on notes with this field.
        requires: vec!["confluence".to_string()],
        login: false,
        program: None,
        cwd: Cwd::Vault,
        timeout_seconds: DEFAULT_TIMEOUT,
        confirm: false,
        env: BTreeMap::new(),
    }
}

/// The variable of [`agent`] that picks the agent program.
pub const AGENT_VAR: &str = "AGENT";

/// `MODE` in the `env` of the [`agent`] command: how the agent changes the
/// note.
pub const MODE_VAR: &str = "MODE";

/// Gives the agent notes of a note (`<!-- @agent «words»: task -->`) to a
/// coding agent.
///
/// `AGENT` in the command `env` picks the program: `claude` (Claude Code) or
/// `copilot` (GitHub Copilot CLI). `MODE` picks what the agent may do:
///
/// - `edit` (the default): it reads and edits the file itself;
/// - `propose`: it may not touch files. It works through the MCP server of
///   kknow: each change is a proposal that the person accepts in the window,
///   and a doubt is a question in the thread of the comment. This needs the
///   server added to the agent as `kknow` (the MCP float shows the command).
///
/// An option, not the new default: `propose` needs the MCP server, and the
/// command must work out of the box.
fn agent() -> CommandSpec {
    CommandSpec {
        id: "agent".to_string(),
        name: "Process comments with an agent".to_string(),
        // `sh`, not fish: the command must work on a machine without fish.
        kind: "shell".to_string(),
        script: AGENT_SCRIPT.to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        requires: Vec::new(),
        login: false,
        program: None,
        // The agent only sees files under its working directory.
        cwd: Cwd::Target,
        // An agent works for minutes, not seconds.
        timeout_seconds: 3600,
        // The agent changes the note, so the person agrees first.
        confirm: true,
        env: BTreeMap::from([
            (AGENT_VAR.to_string(), "claude".to_string()),
            (MODE_VAR.to_string(), "edit".to_string()),
        ]),
    }
}

/// The script of [`agent`].
///
/// The `PATH` line is necessary for a window opened from Finder: its `PATH`
/// has neither `~/.local/bin` (claude) nor `/opt/homebrew/bin` (copilot).
///
/// Claude gets `Edit(./**)`, not a bare `Edit`. The bare rule covers the Edit
/// tool alone, and an agent that rewrites a large part of the note picks
/// Write, which `-p` then denies. A rule with a path covers all file-editing
/// tools, and `./**` keeps them inside the folder of the note (`cwd`).
///
/// In `propose` mode the agent gets `Read` and the tools of the `kknow` MCP
/// server (`mcp__kknow` for claude, `kknow` for copilot), and no file edits.
///
/// Both modes have the same rule for doubt: do not guess. Before, the prompt
/// said "do not ask questions, make the best decision", and the person found
/// guesses in the note. Now an unclear task stays open with a question or
/// with variants to choose from.
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
        // A vault has an empty relative path, and `match` does not apply to it:
        // a pattern is about the contents of the directory.
        let spec = &set(r#"{"commands":[{
                 "id":"push","name":"Отправить","kind":"shell","script":"…",
                 "match":["Docs/**"]}]}"#)
        .commands[0];

        assert!(spec.applies_to(&target(TargetKind::Vault, "", "work")));
        assert!(!spec.applies_to(&target(TargetKind::Note, "Index.md", "work")));
    }

    /// A standalone file is a note without a vault. A command without a scope
    /// is for it too; a command that names vaults or paths is not.
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

        // The file passes its parameters the same way a note does.
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

    /// An empty file is the choice of the user, not a missing file: the
    /// shipped commands do not come back after the user deletes them.
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

    /// Runs the script of the shipped agent command with a fake `claude`
    /// that prints its arguments. `HOME` points to a temporary folder, and
    /// the script looks in `$HOME/.local/bin` first, so a real claude on the
    /// machine never starts.
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
        // The threads are above the folder of the note: the agent may read them.
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
        // A person wrote the file, so it stays in place. Unlike the vault
        // registry, this file is never moved aside.
        assert!(path.is_file());
        assert!(!dir.path().join("commands.json.broken").exists());
    }

    /// The settings form edits the manifest, not the set. An invalid entry must
    /// reach the form intact and survive a save. Otherwise a typo in the kind
    /// would cost the user the whole command.
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

        // The form gets an error: editing a file that failed to parse would
        // erase it on the first save.
        assert!(matches!(Manifest::load(&path), Err(Error::Manifest { .. })));
        // The app and the CLI get a problem and no commands, and keep working.
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
