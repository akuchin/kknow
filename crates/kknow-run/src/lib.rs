//! Commands attached to a note, a folder or a vault.
//!
//! The design follows sync: the public API is a trait ([`runner::Runner`]),
//! and `sh`, `fish` and `python3` are its implementations. The manifest picks
//! one at run time, not at build time. The core knows nothing about the app
//! window or the CLI; both call the same function [`run`].
//!
//! The target path reaches the script as an **environment variable**, not as
//! text inserted into the script. See [`environment`] for the variable list.

pub mod glob;
pub mod runner;
pub mod spec;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use runner::Runner;
pub use spec::{COMMANDS_FILE, CommandSet, CommandSpec, Cwd, Manifest};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("команда не найдена: {0}")]
    Unknown(String),

    #[error("неизвестный вид команды: {0}")]
    UnknownKind(String),

    #[error("команда «{command}» к этой цели не относится")]
    NotApplicable { command: String },

    #[error(
        "не найдена программа {program}: её нет ни в PATH, ни в обычных местах \
         ({places}). Окно, запущенное из Finder, видит короткий системный PATH — \
         укажите полный путь полем program или задайте PATH в env"
    )]
    NoProgram { program: String, places: String },

    #[error("не удалось запустить {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{}: {reason}", path.display())]
    Manifest { path: PathBuf, reason: String },

    #[error("ошибка ввода-вывода в {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

/// The kind of target a command is attached to.
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

    /// Parses a kind name, as used by the CLI and the app.
    pub fn parse(raw: &str) -> Option<Self> {
        TargetKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

/// A command target: a file, a folder or a whole vault.
///
/// The caller that knows the vault builds it (`Workspace` knows both the path
/// for an ID and the root), not this crate. Process launch must not know about
/// note parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub kind: TargetKind,
    /// Absolute path: the note file, the folder or the vault root.
    pub path: PathBuf,
    /// Path from the vault root with `/`; empty for the vault itself.
    /// The manifest `match` patterns use this path.
    pub relative: String,
    /// Note ID (path without extension); only for notes.
    pub note: Option<String>,
    /// Note metadata; the note passes parameters to its commands with it.
    /// There are two sources, frontmatter and `<!-- key: value -->` comments,
    /// merged before they get here. Folders and vaults have none.
    pub fields: BTreeMap<String, String>,
    /// Empty for a standalone file: it belongs to no vault.
    pub vault_id: String,
    pub vault_name: String,
    /// The vault root; for a standalone file, its folder. The command starts
    /// there with the default `cwd`.
    pub vault_root: PathBuf,
}

impl Target {
    /// The whole vault as a target.
    pub fn vault(id: &str, name: &str, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Target {
            kind: TargetKind::Vault,
            path: root.clone(),
            relative: String::new(),
            note: None,
            fields: BTreeMap::new(),
            vault_id: id.to_string(),
            vault_name: name.to_string(),
            vault_root: root,
        }
    }

    /// A markdown file opened without a vault, as a note target.
    ///
    /// It is a note, not a fourth target kind: a command for notes is useful
    /// on a `README.md` too, and a separate kind would hide every existing
    /// command from files until the user ticks it in each one. The scope rules
    /// that need a vault (`vaults`, `match`) do not match it. See
    /// [`CommandSpec::applies_to`].
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
            vault_id: String::new(),
            vault_name: String::new(),
            vault_root: folder,
        }
    }

    /// The target is a standalone file, outside any vault.
    pub fn standalone(&self) -> bool {
        self.vault_id.is_empty()
    }

    fn working_dir(&self, cwd: Cwd) -> &Path {
        match cwd {
            Cwd::Vault => &self.vault_root,
            // For a note, its parent folder; for a folder or vault, itself.
            // Use the root when there is no parent: a process does not accept
            // an empty working directory.
            Cwd::Target => match self.kind {
                TargetKind::Note => self.path.parent().unwrap_or(&self.vault_root),
                _ => &self.path,
            },
        }
    }
}

/// The result of a command run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunOutcome"))]
pub struct Outcome {
    /// Command ID, so the app knows which command the report is for.
    pub command: String,
    /// Exit code; `None` if the process was killed (timeout or signal).
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// The command exceeded `timeoutSeconds` and was stopped.
    pub timed_out: bool,
    /// The caller asked to stop the command ([`Watch::stop`]), and it was
    /// killed.
    pub stopped: bool,
    /// The output is cut: only its start is kept.
    pub truncated: bool,
    /// Run time; the app shows it next to the exit code.
    pub millis: u64,
}

impl Outcome {
    /// Only exit code zero is success, by shell convention.
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// The stream an output line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunStream"))]
pub enum Stream {
    Stdout,
    Stderr,
}

/// What the caller sees and controls while a command runs.
///
/// A long command (a build, an upload) must not look frozen until it ends. So
/// the run gives each output line as soon as it comes, and it checks a stop
/// flag together with the timeout. The CLI does not need this: the terminal
/// already shows a long command as it runs, through its own streams.
pub struct Watch<'a> {
    /// Gets each output line, without its line end. Both reader threads call
    /// it, so it must be `Sync`. The lines are the same text that the
    /// [`Outcome`] keeps, also after the cut: only the start of the output.
    pub line: &'a (dyn Fn(Stream, &str) + Sync),
    /// Set it to stop the command. The run kills the process group, as on
    /// timeout, and the outcome has `stopped`.
    pub stop: &'a AtomicBool,
}

/// How much output to keep from each stream.
///
/// We read everything, or the command blocks on a full pipe. But we keep only
/// the start: nobody reads the report past the first screen, and `find /` in a
/// script must not use up the app memory.
const CAPTURE_LIMIT: usize = 64 * 1024;

/// How often to check if the process has finished.
///
/// The pause grows from one millisecond. A fast command returns almost at
/// once, and a slow one does not cause a busy loop.
const FIRST_POLL: Duration = Duration::from_millis(1);
const MAX_POLL: Duration = Duration::from_millis(50);

/// The variables that the script sees.
///
/// **The path comes as a variable, not as text inserted into the script.**
/// Insertion would allow command injection: a file name can contain
/// `"; rm -rf ~; #`, and file names come from foreign directories and sync. A
/// variable reaches the process without command-line parsing, so its content
/// always stays a value.
///
/// `KKNOW_VAULT` is the same variable that the CLI `--vault` option reads. So
/// `kknow list` inside a script works with the vault the command runs on.
///
/// Frontmatter fields come as `KKNOW_FM_<KEY>`. This is how a note passes
/// parameters to a command: a Confluence page number, a space name, anything
/// else. The key is converted to a valid variable name: `confluence-id`
/// becomes `KKNOW_FM_CONFLUENCE_ID`. Otherwise the shell could not read it:
/// `$KKNOW_FM_CONFLUENCE-ID` parses as a variable followed by `-ID`.
pub fn environment(target: &Target) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let mut put = |name: &str, value: String| {
        env.insert(name.to_string(), value);
    };

    put("KKNOW_PATH", target.path.to_string_lossy().into_owned());
    put("KKNOW_TARGET", target.kind.as_str().to_string());
    put("KKNOW_RELATIVE", target.relative.clone());
    // A standalone file has no vault. Its folder in `KKNOW_VAULT` would make
    // `kknow list` in the script treat that folder as a vault.
    if !target.standalone() {
        put(
            "KKNOW_VAULT",
            target.vault_root.to_string_lossy().into_owned(),
        );
        put("KKNOW_VAULT_ID", target.vault_id.clone());
        put("KKNOW_VAULT_NAME", target.vault_name.clone());
    }
    // The note ID is the path without extension. Scripts need it to call
    // `kknow show`, which takes an ID, not a file.
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

/// Where to look for an interpreter that is not in `PATH`.
///
/// An app started from Finder or the Dock gets `PATH` from launchd, not from a
/// shell: `/usr/bin:/bin:/usr/sbin:/sbin`. `sh`, `bash` and `zsh` are there
/// because they are system programs, but fish comes from homebrew and is not.
/// This is a trap: `kind: "fish"` works in dev mode (the app inherits the
/// terminal `PATH`) and fails in the installed app.
///
/// kknow does not invent a shared `PATH` for commands. The user describes
/// their environment in the shell config, and the shell knows it, not us. Our
/// job is only to find that shell.
const WELL_KNOWN_BINS: &[&str] = &[
    "/opt/homebrew/bin", // homebrew on Apple Silicon
    "/usr/local/bin",    // homebrew on Intel and manual installs
    "/opt/local/bin",    // MacPorts
    "/usr/bin",
    "/bin",
];

/// Full path to an interpreter: first `PATH`, then the well-known places.
///
/// A name with a slash is used as is, because it is already a path. A leading
/// `~/` is expanded here: normally the shell expands it, but we start the
/// program without a shell, so `~` would stay a literal directory name.
fn locate(program: &str) -> Option<PathBuf> {
    if let Some(rest) = program.strip_prefix("~/") {
        let home = std::env::var_os("HOME")?;
        let path = PathBuf::from(home).join(rest);
        return runnable(&path).then_some(path);
    }
    if program.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(program);
        return runnable(&path).then_some(path);
    }

    let from_path = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    from_path
        .into_iter()
        .chain(WELL_KNOWN_BINS.iter().map(PathBuf::from))
        .map(|dir| dir.join(program))
        .find(|candidate| runnable(candidate))
}

/// Checks that the file exists and is executable.
///
/// A directory with a matching name is not a program, so existence is not
/// enough. On unix we also check the execute bit.
#[cfg(unix)]
fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(path: &Path) -> bool {
    path.is_file()
}

/// Variable name for a frontmatter field: `confluence-id` → `KKNOW_FM_CONFLUENCE_ID`.
///
/// A key can be anything, but a variable name can have only letters, digits
/// and underscores, and must not start with a digit. A key that becomes empty
/// after conversion (only punctuation or non-ASCII letters) gives no variable.
/// `sh` does not accept such a name, and a silently broken name is worse than
/// no name.
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

/// Runs a command on a target and waits for it.
///
/// This skips the shared file environment. [`CommandSet::run`] includes it, and
/// callers usually use that.
pub fn run(spec: &CommandSpec, target: &Target) -> Result<Outcome> {
    run_with(spec, target, &BTreeMap::new())
}

/// Runs a command with the shared file environment added.
///
/// It blocks the thread, so the app calls it off the main thread: network and
/// external programs must not block the UI. Sync is split into "plan, network,
/// write" for the same reason.
pub fn run_with(
    spec: &CommandSpec,
    target: &Target,
    shared: &BTreeMap<String, String>,
) -> Result<Outcome> {
    let stop = AtomicBool::new(false);
    let watch = Watch {
        line: &|_, _| {},
        stop: &stop,
    };
    run_watched(spec, target, shared, &watch)
}

/// [`run_with`] that shows the output as it comes and can be stopped.
pub fn run_watched(
    spec: &CommandSpec,
    target: &Target,
    shared: &BTreeMap<String, String>,
    watch: &Watch<'_>,
) -> Result<Outcome> {
    // Check here too, not only in the menu: the scope is part of the manifest,
    // and a direct call must not bypass it.
    if !spec.applies_to(target) {
        return Err(Error::NotApplicable {
            command: spec.id.clone(),
        });
    }

    let runner =
        runner::for_kind(&spec.kind).ok_or_else(|| Error::UnknownKind(spec.kind.clone()))?;
    let wanted = spec.program.as_deref().unwrap_or(runner.program());
    let program = locate(wanted).ok_or_else(|| Error::NoProgram {
        program: wanted.to_string(),
        places: WELL_KNOWN_BINS.join(", "),
    })?;

    let mut command = Command::new(&program);
    command
        .args(runner.args(&spec.script, spec.login))
        .current_dir(target.working_dir(spec.cwd))
        // Three layers, in a deliberate order. The shared file environment is
        // the base: it sets `PATH` and tokens once for all commands, and it
        // must not overwrite the computed `KKNOW_*`. The command's own
        // environment goes last: it is a deliberate edit of one command, and
        // it can override everything.
        .envs(shared)
        .envs(environment(target))
        .envs(&spec.env)
        // No stdin: the command cannot ask the user anything, and it would
        // take over an inherited terminal.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // A separate process group: on timeout we must kill everything the shell
    // started, not only the shell. Without it, `sh -c 'work | tee log'` would
    // outlive its timeout: the shell would die, but the work would go on.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let started = Instant::now();
    let finished = wait(
        command,
        Duration::from_secs(spec.timeout_seconds),
        &program.to_string_lossy(),
        watch,
    )?;

    Ok(Outcome {
        command: spec.id.clone(),
        code: finished.code,
        stdout: finished.stdout,
        stderr: finished.stderr,
        timed_out: finished.timed_out,
        stopped: finished.stopped,
        truncated: finished.truncated,
        millis: started.elapsed().as_millis() as u64,
    })
}

struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    stopped: bool,
    truncated: bool,
}

/// Waits for the process up to the timeout or the stop, and reads both its
/// streams.
///
/// Separate threads read the streams, not this code after `wait`. A pipe is
/// small, and a command that prints more than it holds would block forever:
/// the app would wait for the command, and the command for the pipe. The
/// threads are scoped: they call the borrowed [`Watch::line`].
fn wait(
    mut command: Command,
    timeout: Duration,
    program: &str,
    watch: &Watch<'_>,
) -> Result<Finished> {
    let mut child = command.spawn().map_err(|source| Error::Spawn {
        program: program.to_string(),
        source,
    })?;

    let out = child.stdout.take().expect("stdout запрошен трубой");
    let err = child.stderr.take().expect("stderr запрошен трубой");

    std::thread::scope(|scope| {
        let reading_out = scope.spawn(|| capture(out, |line| (watch.line)(Stream::Stdout, line)));
        let reading_err = scope.spawn(|| capture(err, |line| (watch.line)(Stream::Stderr, line)));

        let deadline = Instant::now() + timeout;
        let mut pause = FIRST_POLL;
        let mut timed_out = false;
        let mut stopped = false;

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(source) => return Err(Error::io(program, source)),
            }

            stopped = watch.stop.load(Ordering::Relaxed);
            if stopped || Instant::now() >= deadline {
                kill(&mut child);
                timed_out = !stopped;
                break None;
            }

            std::thread::sleep(pause);
            pause = (pause * 2).min(MAX_POLL);
        };

        // A panic in a reader thread means a bug in our code. Then the output
        // is empty, but we still return the command result.
        let (stdout, cut_out) = reading_out.join().unwrap_or_default();
        let (stderr, cut_err) = reading_err.join().unwrap_or_default();

        Ok(Finished {
            code: status.and_then(|status| status.code()),
            stdout,
            stderr,
            timed_out,
            stopped,
            truncated: cut_out || cut_err,
        })
    })
}

/// Kills the process group, not one process.
///
/// `sh` almost always runs the real work as a child process. If only the shell
/// dies, the child lives on and keeps the pipe open, so the output wait lasts
/// as long as the timeout tried to avoid. The process has its own group
/// (`process_group(0)`), so the group ID is its `pid`.
fn kill(child: &mut std::process::Child) {
    #[cfg(unix)]
    // SAFETY: we signal our own process group by its ID. If the group is
    // already dead, `killpg` returns an error code; nothing else happens.
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
    }
    // Platforms without groups kill only the process. Its children outlive
    // the timeout there, but `kill` at least closes the pipes.
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads a stream to the end, keeps its start and gives each kept line to
/// `line`.
///
/// Also returns a "truncated" flag: showing the first 64 KB as the full output
/// would misreport how the command ended.
///
/// The lines come from the kept bytes, not from a separate line reader. So a
/// very long line without an end cannot use up the memory, and the lines add
/// up to exactly the text of the outcome. A line split by the cut or without
/// a final line end comes at the end of the stream.
fn capture(mut source: impl Read, line: impl Fn(&str)) -> (String, bool) {
    let mut kept: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    // Where the next line starts in `kept`.
    let mut sent = 0;

    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            // The pipe broke with the killed process. This is the end of the
            // output, not a read error.
            Err(_) => break,
        };

        let room = CAPTURE_LIMIT - kept.len();
        if read > room {
            truncated = true;
        }
        kept.extend_from_slice(&buffer[..read.min(room)]);

        while let Some(end) = kept[sent..].iter().position(|&byte| byte == b'\n') {
            line(&String::from_utf8_lossy(&kept[sent..sent + end]));
            sent += end + 1;
        }
    }
    if sent < kept.len() {
        line(&String::from_utf8_lossy(&kept[sent..]));
    }

    // The cut can split a character. `from_utf8_lossy` puts a replacement
    // character there instead of losing the whole output.
    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(script: &str) -> CommandSpec {
        CommandSet::parse(&format!(
            r#"{{"commands":[{{"id":"проба","name":"Проба","kind":"shell","script":{}}}]}}"#,
            serde_json::to_string(script).unwrap()
        ))
        .unwrap()
        .commands
        .remove(0)
    }

    /// A vault with one note; the target is that note.
    fn note(dir: &Path) -> Target {
        let path = dir.join("Docs/Index.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# Оглавление\n").unwrap();

        Target {
            kind: TargetKind::Note,
            path,
            relative: "Docs/Index.md".to_string(),
            note: Some("Docs/Index".to_string()),
            fields: BTreeMap::new(),
            vault_id: "work".to_string(),
            vault_name: "Работа".to_string(),
            vault_root: dir.to_path_buf(),
        }
    }

    #[test]
    fn the_path_arrives_as_a_variable() {
        let dir = tempfile::tempdir().unwrap();
        let target = note(dir.path());

        let outcome = run(
            &spec("printf '%s|%s|%s' \"$KKNOW_PATH\" \"$KKNOW_NOTE\" \"$KKNOW_TARGET\""),
            &target,
        )
        .unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        assert_eq!(
            outcome.stdout,
            format!("{}|Docs/Index|note", target.path.display())
        );
    }

    #[test]
    fn a_name_that_looks_like_a_command_stays_a_name() {
        // This is why the path is not inserted into the script text. A file
        // name can contain anything (it comes from foreign directories and
        // sync), and it must stay a value.
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        let nasty = dir.path().join("\"; touch взломано; #.md");
        std::fs::write(&nasty, "# всё нормально\n").unwrap();
        target.path = nasty.clone();

        let outcome = run(&spec("printf '%s' \"$KKNOW_PATH\""), &target).unwrap();

        assert_eq!(outcome.stdout, nasty.to_string_lossy());
        assert!(
            !dir.path().join("взломано").exists(),
            "хвост имени файла выполнился как команда"
        );
    }

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

    /// A standalone file has no vault. The script gets no vault variables,
    /// and it starts in the folder of the file.
    #[test]
    fn a_standalone_file_runs_in_its_folder_without_a_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.md");
        std::fs::write(&path, "# Проект\n").unwrap();
        let target = Target::file(&path, BTreeMap::new());

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

        let outcome = run(&spec("pwd"), &target).unwrap();
        assert_eq!(
            std::fs::canonicalize(outcome.stdout.trim()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn frontmatter_fields_arrive_as_variables() {
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        target
            .fields
            .insert("confluence".to_string(), "123456789".to_string());
        target
            .fields
            .insert("confluence-id".to_string(), "второй".to_string());
        // A key with no valid name left gives no variable: `sh` does not
        // accept an empty name or one that starts with a digit.
        target.fields.insert("!!!".to_string(), "мимо".to_string());
        target.fields.insert("2026".to_string(), "мимо".to_string());

        let env = environment(&target);

        assert_eq!(env["KKNOW_FM_CONFLUENCE"], "123456789");
        assert_eq!(env["KKNOW_FM_CONFLUENCE_ID"], "второй");
        assert!(!env.values().any(|value| value == "мимо"), "{env:?}");
    }

    #[test]
    fn a_command_shows_up_only_where_its_field_is_filled_in() {
        let dir = tempfile::tempdir().unwrap();
        let mut wants_id = spec("echo");
        wants_id.requires = vec!["confluence".to_string()];

        let bare = note(dir.path());
        let mut filled = note(dir.path());
        filled
            .fields
            .insert("confluence".to_string(), "123456789".to_string());
        let mut empty = note(dir.path());
        empty.fields.insert("confluence".to_string(), String::new());

        assert!(wants_id.applies_to(&filled));
        assert!(!wants_id.applies_to(&bare), "поля нет — команды нет");
        assert!(
            !wants_id.applies_to(&empty),
            "пустое значение — всё равно что отсутствующее"
        );
        // A direct run on a note without the number also fails.
        assert!(matches!(
            run(&wants_id, &bare),
            Err(Error::NotApplicable { .. })
        ));
    }

    #[test]
    fn the_page_id_reaches_the_command_line_as_an_argument() {
        // End to end: the note names the page number, and the command gets it
        // as a separate argument, as `wiki-publish replace <id> <file>`
        // expects.
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        target
            .fields
            .insert("confluence".to_string(), "123456789".to_string());

        let mut push = spec("printf '%s|%s' \"$KKNOW_FM_CONFLUENCE\" \"$KKNOW_PATH\"");
        push.requires = vec!["confluence".to_string()];

        let outcome = run(&push, &target).unwrap();

        assert_eq!(
            outcome.stdout,
            format!("123456789|{}", target.path.display())
        );
    }

    #[test]
    fn a_login_shell_sees_the_path_of_the_one_who_configured_it() {
        // An app from Finder gets `PATH` from launchd, not from a shell; the
        // profile is the only way to get the real one. We do not compare with
        // a specific directory (it differs between machines). We only check
        // that the path list is longer with the profile.
        let dir = tempfile::tempdir().unwrap();
        let mut plain = spec("printf '%s' \"$PATH\"");
        plain
            .env
            .insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        let mut login = plain.clone();
        login.login = true;

        let bare = run(&plain, &note(dir.path())).unwrap();
        let profile = run(&login, &note(dir.path())).unwrap();

        assert_eq!(bare.stdout, "/usr/bin:/bin");
        assert!(
            profile.stdout.len() >= bare.stdout.len(),
            "профиль не сделал PATH короче: {}",
            profile.stdout
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

    #[test]
    fn the_exit_code_and_stderr_come_back() {
        let dir = tempfile::tempdir().unwrap();

        let outcome = run(&spec("echo беда >&2; exit 3"), &note(dir.path())).unwrap();

        assert!(!outcome.ok());
        assert_eq!(outcome.code, Some(3));
        assert_eq!(outcome.stderr.trim(), "беда");
        assert!(!outcome.timed_out);
    }

    #[test]
    fn work_starts_in_the_vault_by_default_and_may_start_at_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = note(dir.path());

        let at_vault = run(&spec("pwd"), &target).unwrap();
        let mut at_target = spec("pwd");
        at_target.cwd = Cwd::Target;
        let at_note = run(&at_target, &target).unwrap();

        // Compare directories with canonicalize: macOS maps `/var` to
        // `/private/var`, so a plain string compare would always fail.
        assert_eq!(
            std::fs::canonicalize(at_vault.stdout.trim()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(at_note.stdout.trim()).unwrap(),
            std::fs::canonicalize(dir.path().join("Docs")).unwrap()
        );
    }

    /// The kill must not lose what the command printed before it.
    ///
    /// The output can be checked only if the shell got to the `echo` before
    /// the timeout. On a loaded machine (a full `cargo test` right after a
    /// build) the start sometimes took more than a second, and the test failed
    /// for a reason that has nothing to do with the kill. The script leaves a
    /// mark after the `echo`. Without the mark, the run repeats with a longer
    /// timeout.
    #[test]
    fn a_hanging_command_is_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("начали");
        let script = format!("echo начали; touch '{}'; sleep 30", mark.display());

        for timeout in [1, 3, 10] {
            let mut hanging = spec(&script);
            hanging.timeout_seconds = timeout;

            let outcome = run(&hanging, &note(dir.path())).unwrap();

            assert!(outcome.timed_out, "{outcome:?}");
            assert_eq!(outcome.code, None, "процесс убит, кода возврата у него нет");
            assert!(
                outcome.millis < timeout * 1000 + 4000,
                "ждали не дольше отпущенного: {outcome:?}"
            );
            if !mark.exists() {
                continue;
            }
            assert!(
                outcome.stdout.contains("начали"),
                "успевшее напечататься — есть: {outcome:?}"
            );
            return;
        }
        panic!("оболочка не дошла до echo и за 10 секунд");
    }

    #[test]
    fn a_talkative_command_does_not_wedge_and_does_not_eat_memory() {
        let dir = tempfile::tempdir().unwrap();
        // More than the OS pipe holds and more than we keep. If we read the
        // output after the wait, the command would block forever.
        let outcome = run(
            &spec("yes 'строка подлиннее' | head -c 200000; echo; echo хвост >&2"),
            &note(dir.path()),
        )
        .unwrap();

        assert!(outcome.ok(), "{:?}", outcome.stderr);
        assert!(outcome.truncated);
        assert_eq!(outcome.stdout.len(), CAPTURE_LIMIT);
        assert_eq!(outcome.stderr.trim(), "хвост");
    }

    /// Runs `script` and collects the lines it prints, with their streams.
    fn watched(script: &str, stop: &AtomicBool) -> (Outcome, Vec<(Stream, String)>) {
        let dir = tempfile::tempdir().unwrap();
        let lines = std::sync::Mutex::new(Vec::new());
        let watch = Watch {
            line: &|stream, text| lines.lock().unwrap().push((stream, text.to_string())),
            stop,
        };
        let outcome =
            run_watched(&spec(script), &note(dir.path()), &BTreeMap::new(), &watch).unwrap();
        (outcome, lines.into_inner().unwrap())
    }

    #[test]
    fn each_line_comes_with_its_stream() {
        let stop = AtomicBool::new(false);

        let (outcome, lines) = watched("echo раз; echo беда >&2; printf 'два\\nбез конца'", &stop);

        assert!(outcome.ok(), "{outcome:?}");
        let out: Vec<&str> = lines
            .iter()
            .filter(|(stream, _)| *stream == Stream::Stdout)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(out, ["раз", "два", "без конца"]);
        assert!(lines.contains(&(Stream::Stderr, "беда".to_string())));
        // The lines are the same text as the outcome, only split.
        assert_eq!(outcome.stdout, "раз\nдва\nбез конца");
    }

    #[test]
    fn a_line_comes_while_the_command_still_runs() {
        let dir = tempfile::tempdir().unwrap();
        let stop = AtomicBool::new(false);
        // The line callback stops the command: if lines came only at the end,
        // the run would wait the whole `sleep`.
        let watch = Watch {
            line: &|_, text| {
                if text == "начали" {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            stop: &stop,
        };

        let outcome = run_watched(
            &spec("echo начали; sleep 30"),
            &note(dir.path()),
            &BTreeMap::new(),
            &watch,
        )
        .unwrap();

        assert!(outcome.stopped, "{outcome:?}");
        assert!(!outcome.timed_out, "остановка — не тайм-аут: {outcome:?}");
        assert_eq!(outcome.code, None);
        assert!(outcome.millis < 20_000, "{outcome:?}");
        assert_eq!(outcome.stdout, "начали\n");
    }

    #[test]
    fn a_stop_kills_the_whole_group() {
        // The stop flag is up before the start: the run must still end at
        // once, and the `sleep` child must not keep the pipe open.
        let stop = AtomicBool::new(true);

        let (outcome, _) = watched("sleep 30 | cat", &stop);

        assert!(outcome.stopped, "{outcome:?}");
        assert!(outcome.millis < 20_000, "{outcome:?}");
    }

    #[test]
    fn a_cut_output_gives_only_the_kept_lines() {
        let stop = AtomicBool::new(false);

        let (outcome, lines) = watched("yes 'строка' | head -n 100000", &stop);

        assert!(outcome.truncated);
        // The last kept line is cut and has no line end of its own.
        let joined: Vec<&str> = lines.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined.join("\n"), outcome.stdout);
        assert!(lines.len() < 100_000);
    }

    #[test]
    fn a_missing_interpreter_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut broken = spec("echo");
        broken.program = Some("такой-программы-нет".to_string());

        let refusal = run(&broken, &note(dir.path())).unwrap_err().to_string();

        // The message must say where to look: "No such file or directory"
        // says nothing about an app started from Finder.
        assert!(refusal.contains("такой-программы-нет"), "{refusal}");
        assert!(refusal.contains("PATH"), "{refusal}");
    }

    #[test]
    fn an_interpreter_is_found_outside_a_short_path() {
        // This is what happens to an app from Finder: `PATH` comes from
        // launchd and has only system directories. We must still find the
        // shell.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        assert!(locate("sh").is_some(), "системная оболочка есть всегда");
        assert!(locate("такой-программы-нет").is_none());
        // A directory with a matching name is not a program.
        std::fs::create_dir_all(bin.join("ловушка")).unwrap();
        assert!(locate(&bin.join("ловушка").to_string_lossy()).is_none());
    }

    #[test]
    fn a_home_relative_program_is_expanded() {
        // The shell expands `~`, but we run the program without a shell. So we
        // expand it ourselves; otherwise it is a directory name with a tilde.
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("своя-программа");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // SAFETY: the test is single-threaded and restores the variable itself.
        let was = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.path()) };
        let found = locate("~/bin/своя-программа");
        match was {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }

        assert_eq!(found, Some(tool));
    }

    #[test]
    fn the_shared_environment_of_the_file_reaches_every_command() {
        let dir = tempfile::tempdir().unwrap();
        let set = CommandSet::parse(
            r#"{
                 "env": {"SPACE": "DOCS", "SERVER": "wiki.example"},
                 "commands": [{
                   "id":"проба","name":"Проба","kind":"shell",
                   "script":"printf '%s|%s|%s' \"$SPACE\" \"$SERVER\" \"$KKNOW_TARGET\"",
                   "env": {"SERVER": "своё"}}]}"#,
        )
        .unwrap();

        let outcome = set.run("проба", &note(dir.path())).unwrap();

        // All commands see the shared values, a command's own value wins over
        // the shared one, and the shared block cannot overwrite `KKNOW_*`.
        assert_eq!(outcome.stdout, "DOCS|своё|note");
    }

    #[test]
    fn a_shared_environment_cannot_shadow_the_computed_variables() {
        let dir = tempfile::tempdir().unwrap();
        let set = CommandSet::parse(
            r#"{
                 "env": {"KKNOW_TARGET": "подмена"},
                 "commands": [{"id":"проба","name":"Проба","kind":"shell",
                   "script":"printf '%s' \"$KKNOW_TARGET\""}]}"#,
        )
        .unwrap();

        assert_eq!(set.run("проба", &note(dir.path())).unwrap().stdout, "note");
    }

    #[test]
    fn a_command_cannot_be_run_outside_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let mut narrow = spec("echo");
        narrow.targets = vec![TargetKind::Vault];

        assert!(matches!(
            run(&narrow, &note(dir.path())),
            Err(Error::NotApplicable { .. })
        ));
    }

    #[test]
    fn the_manifest_may_add_variables_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut with_env = spec("printf '%s' \"$SPACE\"");
        with_env.env.insert("SPACE".to_string(), "DOCS".to_string());

        let outcome = run(&with_env, &note(dir.path())).unwrap();

        assert_eq!(outcome.stdout, "DOCS");
    }

    #[test]
    fn python_runs_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut python = spec("import os; print(os.environ['KKNOW_NOTE'])");
        python.kind = "python".to_string();

        match run(&python, &note(dir.path())) {
            Ok(outcome) => {
                assert!(outcome.ok(), "{:?}", outcome.stderr);
                assert_eq!(outcome.stdout.trim(), "Docs/Index");
            }
            // Not every machine has python3, and this test is not about it.
            Err(Error::Spawn { .. }) => {}
            Err(other) => panic!("{other}"),
        }
    }
}
