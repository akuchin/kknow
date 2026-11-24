pub mod glob;
pub mod login;
pub mod runner;
pub mod spec;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use runner::Runner;
pub use spec::{COMMANDS_FILE, CommandSet, CommandSpec, Cwd, Manifest};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("command is not found: {0}")]
    Unknown(String),

    #[error("unknown command kind: {0}")]
    UnknownKind(String),

    #[error("command \"{command}\" does not apply to this target")]
    NotApplicable { command: String },

    #[error(
        "cannot find program {program}: it is not in PATH or in the usual places \
         ({places}). A window started from Finder sees a short system PATH: \
         give the full path in the program field or set PATH in env. \
         On Windows, sh and bash come with Git for Windows"
    )]
    NoProgram { program: String, places: String },

    #[error("cannot start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{}: {reason}", path.display())]
    Manifest { path: PathBuf, reason: String },

    #[error("I/O error in {path}: {source}")]
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
pub struct Target {
    pub kind: TargetKind,
    pub path: PathBuf,
    pub relative: String,
    pub note: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub vault_id: String,
    pub vault_name: String,
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
            vault_id: id.to_string(),
            vault_name: name.to_string(),
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
            vault_id: String::new(),
            vault_name: String::new(),
            vault_root: folder,
        }
    }

    pub fn standalone(&self) -> bool {
        self.vault_id.is_empty()
    }

    fn working_dir(&self, cwd: Cwd) -> &Path {
        match cwd {
            Cwd::Vault => &self.vault_root,
            Cwd::Target => match self.kind {
                TargetKind::Note => self.path.parent().unwrap_or(&self.vault_root),
                _ => &self.path,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunOutcome"))]
pub struct Outcome {
    pub command: String,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub stopped: bool,
    pub truncated: bool,
    pub millis: u64,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunStream"))]
pub enum Stream {
    Stdout,
    Stderr,
}

pub struct Watch<'a> {
    pub line: &'a (dyn Fn(Stream, &str) + Sync),
    pub stop: &'a AtomicBool,
}

const CAPTURE_LIMIT: usize = 64 * 1024;

const FIRST_POLL: Duration = Duration::from_millis(1);
const MAX_POLL: Duration = Duration::from_millis(50);

const DRAIN: Duration = Duration::from_millis(500);

const DRAIN_LIMIT: Duration = Duration::from_secs(5);

pub fn environment(target: &Target) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let mut put = |name: &str, value: String| {
        env.insert(name.to_string(), value);
    };

    put("KKNOW_PATH", target.path.to_string_lossy().into_owned());
    put("KKNOW_TARGET", target.kind.as_str().to_string());
    put("KKNOW_RELATIVE", target.relative.clone());
    if !target.standalone() {
        put(
            "KKNOW_VAULT",
            target.vault_root.to_string_lossy().into_owned(),
        );
        put("KKNOW_VAULT_ID", target.vault_id.clone());
        put("KKNOW_VAULT_NAME", target.vault_name.clone());
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

#[cfg(unix)]
const WELL_KNOWN_BINS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/opt/local/bin",
    "/usr/bin",
    "/bin",
];

pub fn locate(program: &str) -> Option<PathBuf> {
    first_real(candidates(program).into_iter())
}

fn locate_any(names: &[&str]) -> Option<PathBuf> {
    first_real(names.iter().flat_map(|name| candidates(name)))
}

fn candidates(program: &str) -> Vec<PathBuf> {
    let extensions = executable_extensions();
    if let Some(rest) = program.strip_prefix("~/") {
        let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        else {
            return Vec::new();
        };
        return with_extensions(&PathBuf::from(home).join(rest), &extensions)
            .filter(|candidate| runnable(candidate))
            .collect();
    }
    if program.contains('/') || program.contains(std::path::MAIN_SEPARATOR) {
        return with_extensions(Path::new(program), &extensions)
            .filter(|candidate| runnable(candidate))
            .collect();
    }

    let from_path = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    from_path
        .into_iter()
        .chain(well_known_bins())
        .flat_map(|dir| with_extensions(&dir.join(program), &extensions).collect::<Vec<_>>())
        .filter(|candidate| runnable(candidate) && !is_wsl_launcher(candidate))
        .collect()
}

fn first_real(candidates: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    let mut stub = None;
    for candidate in candidates {
        if std::fs::metadata(&candidate).is_ok_and(|meta| meta.len() > 0) {
            return Some(candidate);
        }
        stub.get_or_insert(candidate);
    }
    stub
}

fn with_extensions<'a>(
    path: &'a Path,
    extensions: &'a [String],
) -> impl Iterator<Item = PathBuf> + 'a {
    std::iter::once(path.to_path_buf()).chain(extensions.iter().map(move |extension| {
        let mut name = path.as_os_str().to_os_string();
        name.push(extension);
        PathBuf::from(name)
    }))
}

#[cfg(unix)]
fn executable_extensions() -> Vec<String> {
    Vec::new()
}

#[cfg(not(unix))]
fn executable_extensions() -> Vec<String> {
    parse_pathext(std::env::var("PATHEXT").ok().as_deref())
}

#[cfg(any(not(unix), test))]
fn parse_pathext(value: Option<&str>) -> Vec<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD")
        .split(';')
        .map(str::trim)
        .filter(|extension| extension.starts_with('.') && extension.len() > 1)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(unix)]
fn well_known_bins() -> Vec<PathBuf> {
    WELL_KNOWN_BINS.iter().map(PathBuf::from).collect()
}

#[cfg(not(unix))]
fn well_known_bins() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    for name in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(folder) = var(name) {
            roots.push(folder.join("Git"));
        }
    }
    if let Some(local) = var("LOCALAPPDATA") {
        roots.push(local.join("Programs").join("Git"));
    }
    if let Some(home) = var("USERPROFILE") {
        roots.push(home.join("scoop").join("apps").join("git").join("current"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("git.exe").is_file()
                && let Some(root) = git_root_of(&dir)
            {
                roots.push(root);
            }
        }
    }
    git_for_windows_bins(roots)
}

#[cfg(any(not(unix), test))]
fn git_root_of(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_string_lossy().to_lowercase();
    let parent = dir.parent()?;
    match name.as_str() {
        "cmd" => Some(parent.to_path_buf()),
        "bin"
            if parent
                .file_name()?
                .to_string_lossy()
                .eq_ignore_ascii_case("mingw64") =>
        {
            parent.parent().map(Path::to_path_buf)
        }
        "bin" => Some(parent.to_path_buf()),
        _ => None,
    }
}

#[cfg(any(not(unix), test))]
fn git_for_windows_bins(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut bins: Vec<PathBuf> = Vec::new();
    for root in roots {
        for dir in [
            root.join("bin"),
            root.join("usr").join("bin"),
            root.join("cmd"),
        ] {
            if !bins.contains(&dir) {
                bins.push(dir);
            }
        }
    }
    bins
}

fn is_wsl_launcher(path: &Path) -> bool {
    let Some(stem) = path.file_stem() else {
        return false;
    };
    let folder = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_lowercase());
    stem.eq_ignore_ascii_case("bash")
        && matches!(folder.as_deref(), Some("system32" | "windowsapps"))
}

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

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

pub fn run(spec: &CommandSpec, target: &Target) -> Result<Outcome> {
    run_with(spec, target, &BTreeMap::new())
}

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

pub fn run_watched(
    spec: &CommandSpec,
    target: &Target,
    shared: &BTreeMap<String, String>,
    watch: &Watch<'_>,
) -> Result<Outcome> {
    if !spec.applies_to(target) {
        return Err(Error::NotApplicable {
            command: spec.id.clone(),
        });
    }

    let runner =
        runner::for_kind(&spec.kind).ok_or_else(|| Error::UnknownKind(spec.kind.clone()))?;
    let names: Vec<&str> = match spec.program.as_deref() {
        Some(explicit) => vec![explicit],
        None => std::iter::once(runner.program())
            .chain(runner.also().iter().copied())
            .collect(),
    };
    let wanted = names[0];
    let program = locate_any(&names).ok_or_else(|| Error::NoProgram {
        program: wanted.to_string(),
        places: well_known_bins()
            .iter()
            .map(|dir| dir.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    })?;

    let mut command = Command::new(&program);
    #[cfg(windows)]
    command.env("CHERE_INVOKING", "1");
    command
        .args(runner.args(&spec.script, spec.login))
        .current_dir(target.working_dir(spec.cwd))
        .envs(shared)
        .envs(environment(target))
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    hide_console(&mut command);

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
    let group = Group::of(&child);

    let out = child.stdout.take().expect("stdout is a pipe");
    let err = child.stderr.take().expect("stderr is a pipe");
    let (out_pipe, err_pipe) = (Pipe::of(&out), Pipe::of(&err));
    let ended = OnceLock::new();

    std::thread::scope(|scope| {
        let reading_out = scope.spawn(|| {
            capture(
                out,
                || out_pipe.has_more(&ended),
                |line| (watch.line)(Stream::Stdout, line),
            )
        });
        let reading_err = scope.spawn(|| {
            capture(
                err,
                || err_pipe.has_more(&ended),
                |line| (watch.line)(Stream::Stderr, line),
            )
        });

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
                group.kill(&mut child);
                timed_out = !stopped;
                break None;
            }

            std::thread::sleep(pause);
            pause = (pause * 2).min(MAX_POLL);
        };
        let _ = ended.set(Instant::now());

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

struct Group {
    #[cfg(windows)]
    job: Option<Job>,
}

impl Group {
    #[cfg(unix)]
    fn of(_: &std::process::Child) -> Self {
        Group {}
    }

    #[cfg(windows)]
    fn of(child: &std::process::Child) -> Self {
        Group {
            job: Job::holding(child),
        }
    }

    fn kill(&self, child: &mut std::process::Child) {
        #[cfg(unix)]
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(windows)]
struct Job(std::os::windows::io::OwnedHandle);

#[cfg(windows)]
impl Job {
    fn holding(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return None;
        }
        let job = unsafe { OwnedHandle::from_raw_handle(handle) };
        let joined = unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) };
        (joined != 0).then_some(Job(job))
    }

    fn terminate(&self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        unsafe {
            TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

#[derive(Clone, Copy)]
struct Pipe {
    #[cfg(unix)]
    fd: std::os::fd::RawFd,
    #[cfg(windows)]
    handle: isize,
}

impl Pipe {
    #[cfg(unix)]
    fn of(source: &impl std::os::fd::AsRawFd) -> Self {
        Pipe {
            fd: source.as_raw_fd(),
        }
    }

    #[cfg(windows)]
    fn of(source: &impl std::os::windows::io::AsRawHandle) -> Self {
        Pipe {
            handle: source.as_raw_handle() as isize,
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn of<T>(_: &T) -> Self {
        Pipe {}
    }

    #[cfg(unix)]
    fn has_more(self, ended: &OnceLock<Instant>) -> bool {
        let mut silent_since = None;
        loop {
            if ended.get().is_some_and(|at| at.elapsed() >= DRAIN_LIMIT) {
                return false;
            }
            let mut fd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, MAX_POLL.as_millis() as libc::c_int) };
            if ready > 0 {
                return true;
            }
            if ready < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return true;
            }
            if ended.get().is_none() {
                continue;
            }
            let since = *silent_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= DRAIN {
                return false;
            }
        }
    }

    #[cfg(windows)]
    fn has_more(self, ended: &OnceLock<Instant>) -> bool {
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;

        let mut silent_since = None;
        loop {
            if ended.get().is_some_and(|at| at.elapsed() >= DRAIN_LIMIT) {
                return false;
            }
            let mut waiting: u32 = 0;
            let peeked = unsafe {
                PeekNamedPipe(
                    self.handle as *mut std::ffi::c_void,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut waiting,
                    std::ptr::null_mut(),
                )
            };
            if peeked == 0 {
                return true;
            }
            if waiting > 0 {
                return true;
            }
            if ended.get().is_none() {
                std::thread::sleep(MAX_POLL);
                continue;
            }
            let since = *silent_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= DRAIN {
                return false;
            }
            std::thread::sleep(MAX_POLL);
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn has_more(self, _: &OnceLock<Instant>) -> bool {
        true
    }
}

fn capture(
    mut source: impl Read,
    mut has_more: impl FnMut() -> bool,
    line: impl Fn(&str),
) -> (String, bool) {
    let mut kept: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    let mut sent = 0;

    while has_more() {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
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
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        let nasty = dir.path().join(if cfg!(windows) {
            "'; touch взломано; #.md"
        } else {
            "\"; touch взломано; #.md"
        });
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

        let outcome = run(&spec("printf x > где-я"), &target).unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        assert!(
            dir.path().join("где-я").exists(),
            "команда начала работу не в папке файла"
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
        assert!(matches!(
            run(&wants_id, &bare),
            Err(Error::NotApplicable { .. })
        ));
    }

    #[test]
    fn the_page_id_reaches_the_command_line_as_an_argument() {
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
        let dir = tempfile::tempdir().unwrap();
        let mut plain = spec("printf '%s' \"$PATH\"");
        plain
            .env
            .insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        let mut login = plain.clone();
        login.login = true;

        let bare = run(&plain, &note(dir.path())).unwrap();
        let profile = run(&login, &note(dir.path())).unwrap();

        assert!(
            bare.stdout.contains("/usr/bin:/bin"),
            "заданный PATH дошёл до скрипта: {}",
            bare.stdout
        );
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

        let at_vault = run(&spec("printf x > от-корня"), &target).unwrap();
        let mut at_target = spec("printf x > от-заметки");
        at_target.cwd = Cwd::Target;
        let at_note = run(&at_target, &target).unwrap();

        assert!(at_vault.ok(), "{at_vault:?}");
        assert!(at_note.ok(), "{at_note:?}");
        assert!(
            dir.path().join("от-корня").exists(),
            "по умолчанию работа начинается в корне хранилища"
        );
        assert!(
            dir.path().join("Docs").join("от-заметки").exists(),
            "с Cwd::Target работа начинается в папке заметки"
        );
    }

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
        assert_eq!(outcome.stdout, "раз\nдва\nбез конца");
    }

    #[test]
    fn a_line_comes_while_the_command_still_runs() {
        let dir = tempfile::tempdir().unwrap();
        let stop = AtomicBool::new(false);
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
        let stop = AtomicBool::new(true);

        let (outcome, _) = watched("sleep 30 | cat", &stop);

        assert!(outcome.stopped, "{outcome:?}");
        assert!(outcome.millis < 20_000, "{outcome:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_does_not_wait_for_a_stranger_that_holds_the_pipe() {
        let stop = AtomicBool::new(false);
        let dir = tempfile::tempdir().unwrap();
        let watch = Watch {
            line: &|_, text| {
                if text == "начали" {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            stop: &stop,
        };

        let outcome = run_watched(
            &spec(r#"perl -e '$| = 1; setpgrp(0, 0); print "начали\n"; sleep 20' & sleep 30"#),
            &note(dir.path()),
            &BTreeMap::new(),
            &watch,
        )
        .unwrap();

        assert!(outcome.stopped, "{outcome:?}");
        assert!(outcome.millis < 10_000, "{outcome:?}");
        assert_eq!(outcome.stdout, "начали\n");
    }

    #[test]
    fn the_end_of_the_process_does_not_cut_the_output() {
        let stop = AtomicBool::new(false);

        let (outcome, _) = watched("(sleep 0.2; echo поздно) & seq 1 5000", &stop);

        assert!(outcome.ok(), "{outcome:?}");
        assert!(
            outcome.stdout.contains("\n5000\n"),
            "{}",
            outcome.stdout.len()
        );
        assert!(outcome.stdout.ends_with("поздно\n"), "{outcome:?}");
    }

    #[test]
    fn a_cut_output_gives_only_the_kept_lines() {
        let stop = AtomicBool::new(false);

        let (outcome, lines) = watched("yes 'строка' | head -n 100000", &stop);

        assert!(outcome.truncated);
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

        assert!(refusal.contains("такой-программы-нет"), "{refusal}");
        assert!(refusal.contains("PATH"), "{refusal}");
    }

    #[test]
    fn an_interpreter_is_found_outside_a_short_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        assert!(locate("sh").is_some(), "системная оболочка есть всегда");
        assert!(locate("такой-программы-нет").is_none());
        std::fs::create_dir_all(bin.join("ловушка")).unwrap();
        assert!(locate(&bin.join("ловушка").to_string_lossy()).is_none());
    }

    #[test]
    fn a_windows_program_is_found_with_its_extension() {
        let extensions = parse_pathext(Some(".COM;.EXE; ;.BAT;CMD"));
        assert_eq!(extensions, [".com", ".exe", ".bat"]);
        assert_eq!(parse_pathext(None), [".com", ".exe", ".bat", ".cmd"]);

        let tried: Vec<PathBuf> = with_extensions(Path::new("bin/sh"), &extensions).collect();
        assert_eq!(
            tried,
            ["bin/sh", "bin/sh.com", "bin/sh.exe", "bin/sh.bat"].map(PathBuf::from)
        );
    }

    #[test]
    fn git_bash_is_found_next_to_git() {
        let root = PathBuf::from("Programs").join("Git");
        assert_eq!(git_root_of(&root.join("cmd")), Some(root.clone()));
        assert_eq!(git_root_of(&root.join("bin")), Some(root.clone()));
        assert_eq!(
            git_root_of(&root.join("mingw64").join("bin")),
            Some(root.clone())
        );
        assert_eq!(git_root_of(Path::new("Windows")), None);

        let bins = git_for_windows_bins(vec![root.clone(), root.clone()]);
        assert_eq!(
            bins,
            [
                root.join("bin"),
                root.join("usr").join("bin"),
                root.join("cmd")
            ]
        );
    }

    #[test]
    fn the_wsl_launcher_is_not_bash() {
        let system = PathBuf::from("Windows").join("System32");
        assert!(is_wsl_launcher(&system.join("bash.exe")));
        assert!(is_wsl_launcher(
            &PathBuf::from("WindowsApps").join("bash.exe")
        ));
        assert!(!is_wsl_launcher(
            &PathBuf::from("Git").join("bin").join("bash.exe")
        ));
        assert!(!is_wsl_launcher(&system.join("cmd.exe")));
    }

    #[test]
    fn a_home_relative_program_is_expanded() {
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
            Err(Error::Spawn { .. }) => {}
            Err(other) => panic!("{other}"),
        }
    }
}
