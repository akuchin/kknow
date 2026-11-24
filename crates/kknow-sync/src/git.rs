use crate::shared::Merge;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::config;
use crate::plan::CONFLICT_MARK;
use crate::{Error as SyncError, Result};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("git is not found: {}. Set the path to git in the settings", .0.display())]
    Missing(PathBuf),

    #[error("git: {0}")]
    Command(String),

    #[error(
        "the vault folder is not empty and is not a git repository. Connect an empty folder or a folder with a repository"
    )]
    NotEmpty,

    #[error("the vault is not the root of a git repository (the root is {}). One vault is one repository", .0.display())]
    NotRoot(PathBuf),

    #[error("the repository has no remote origin: set the address")]
    NoOrigin,

    #[error("the git server does not answer ({0}). Check the address and the network connection")]
    Unreachable(String),

    #[error("the git server refused the sign-in ({0}). Check the token or the SSH key")]
    Denied(String),

    #[error("the repository is not found ({0}). Check its address")]
    RepoMissing(String),

    #[error(
        "the repository address contains a login or a token. Remove it from the address and give the token separately"
    )]
    UrlCredentials,

    #[error("HEAD is not on a branch: switch to a branch")]
    Detached,

    #[error("the commit message is empty")]
    EmptyMessage,

    #[error("commit your changes first: the merge would overwrite them ({})", .0.join(", "))]
    LocalChanges(Vec<String>),
}

const TOKEN_VARIABLE: &str = "KKNOW_GIT_TOKEN";

const TOKEN_HELPER: &str = "credential.helper=!f() { echo username=x-access-token; echo \"password=$KKNOW_GIT_TOKEN\"; }; f";

const EXCLUDE: &[&str] = &[
    "/.kknow/remote.json",
    "/.kknow/sync.json",
    "/.kknow/*.tmp",
    "/.kknow/trash/",
    "/.trash/",
];

const EXCLUDE_HEADER: &str = "# kknow: the settings of this machine and the trash";

const SERVICE_NAMES: &[&str] = &[".kknow", ".trash", ".DS_Store"];

const REMOTE: &str = "origin";

const DEFAULT_BRANCH: &str = "main";

const FALLBACK_NAME: &str = "kknow";
const FALLBACK_EMAIL: &str = "kknow@localhost";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Index,
    WorkTree,
}

impl Side {
    pub fn from_staged(staged: bool) -> Self {
        if staged { Side::Index } else { Side::WorkTree }
    }
}

#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    root: PathBuf,
    token: Option<String>,
    env: Vec<(String, String)>,
    merge: Option<Merge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitChange {
    pub path: String,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub from: Option<String>,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub staged: Option<ChangeKind>,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub unstaged: Option<ChangeKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitStatus {
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub branch: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<GitChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConflict {
    pub path: String,
    pub copy: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merged {
    pub changed: Vec<String>,
    pub conflicts: Vec<GitConflict>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitReport {
    pub merged: Merged,
    pub pushed: usize,
}

impl Git {
    pub fn new(program: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Git {
            program: program.into(),
            root: root.into(),
            token: None,
            env: Vec::new(),
            merge: None,
        }
    }

    pub fn with_merge(mut self, merge: Merge) -> Self {
        self.merge = Some(merge);
        self
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|token| !token.is_empty());
        self
    }

    pub fn with_env(mut self, name: &str, value: impl AsRef<Path>) -> Self {
        self.env.push((
            name.to_string(),
            value.as_ref().to_string_lossy().into_owned(),
        ));
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn version(&self) -> Result<String> {
        let out = self.text(&["version"])?;
        Ok(out.trim().to_string())
    }

    pub fn is_repository(root: &Path) -> bool {
        root.join(".git").exists()
    }

    pub fn origin(&self) -> Result<Option<String>> {
        match self.text(&["remote", "get-url", REMOTE]) {
            Ok(url) => Ok(Some(url.trim().to_string())),
            Err(SyncError::Git(Error::Command(_) | Error::NoOrigin)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn attach(&self, url: Option<&str>) -> Result<String> {
        let url = url.map(str::trim).filter(|url| !url.is_empty());
        if let Some(url) = url {
            check_url(url)?;
        }

        if Self::is_repository(&self.root) {
            self.check_root()?;
            let url = match (self.origin()?, url) {
                (Some(origin), _) => origin,
                (None, Some(url)) => {
                    self.run(&["remote", "add", REMOTE, url])?;
                    url.to_string()
                }
                (None, None) => return Err(Error::NoOrigin.into()),
            };
            self.exclude()?;
            return Ok(url);
        }

        let url = url.ok_or(config::Error::NoGitUrl)?;
        if !is_empty(&self.root)? {
            return Err(Error::NotEmpty.into());
        }

        let branch = self.remote_default_branch(url)?;

        self.run(&["init", "-q"])?;
        let head = format!("refs/heads/{}", branch.as_deref().unwrap_or(DEFAULT_BRANCH));
        self.run(&["symbolic-ref", "HEAD", &head])?;
        self.run(&["remote", "add", REMOTE, url])?;
        self.exclude()?;

        if let Some(branch) = branch {
            self.fetch()?;
            let tracked = format!("{REMOTE}/{branch}");
            self.run(&["checkout", "-q", "-f", "-B", &branch, "--track", &tracked])?;
        }
        Ok(url.to_string())
    }

    fn check_root(&self) -> Result<()> {
        let top = self.text(&["rev-parse", "--show-toplevel"])?;
        let top =
            fs::canonicalize(top.trim()).map_err(|source| SyncError::io(top.trim(), source))?;
        let root =
            fs::canonicalize(&self.root).map_err(|source| SyncError::io(&self.root, source))?;
        if top == root {
            Ok(())
        } else {
            Err(Error::NotRoot(top).into())
        }
    }

    fn remote_default_branch(&self, url: &str) -> Result<Option<String>> {
        let out = self.text(&["ls-remote", "--symref", url])?;
        let mut head = None;
        let mut branches = Vec::new();
        for line in out.lines() {
            if let Some(target) = line.strip_prefix("ref: refs/heads/") {
                head = target
                    .split_once('\t')
                    .map(|(branch, _)| branch.to_string());
            } else if let Some((_, reference)) = line.split_once('\t')
                && let Some(branch) = reference.strip_prefix("refs/heads/")
            {
                branches.push(branch.to_string());
            }
        }

        let has = |name: &str| branches.iter().any(|branch| branch == name);
        Ok(head
            .filter(|head| has(head))
            .or_else(|| {
                ["main", "master"]
                    .into_iter()
                    .find(|name| has(name))
                    .map(String::from)
            })
            .or_else(|| branches.first().cloned()))
    }

    pub fn exclude(&self) -> Result<()> {
        let path = self.root.join(".git/info/exclude");
        let current = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(source) => return Err(SyncError::io(&path, source)),
        };

        let missing: Vec<&str> = EXCLUDE
            .iter()
            .copied()
            .filter(|line| !current.lines().any(|existing| existing.trim() == *line))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }

        let mut text = current;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.contains(EXCLUDE_HEADER) {
            text.push_str(EXCLUDE_HEADER);
            text.push('\n');
        }
        for line in missing {
            text.push_str(line);
            text.push('\n');
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SyncError::io(parent, source))?;
        }
        fs::write(&path, text).map_err(|source| SyncError::io(&path, source))
    }

    pub fn status(&self) -> Result<GitStatus> {
        self.exclude()?;
        let out = self.run(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ])?;
        let mut status = parse_status(&String::from_utf8_lossy(&out));

        if let Some(branch) = &status.branch
            && !self.exists(&format!("refs/remotes/{REMOTE}/{branch}"))
            && self.head().is_some()
        {
            status.ahead = self.count("HEAD")?;
        }
        Ok(status)
    }

    pub fn stage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["add", "-A", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["reset", "-q", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    pub fn diff(&self, path: &str, from: Option<&str>, side: Side) -> Result<String> {
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
        match side {
            Side::Index => args.push("--cached"),
            Side::WorkTree if self.text(&["ls-files", "--", path])?.is_empty() => {
                args.extend(["--no-index", "--", "/dev/null", path]);
                return self.text_of_diff(&args);
            }
            Side::WorkTree => {}
        }
        args.push("--");
        args.extend(from);
        args.push(path);
        self.text(&args)
    }

    pub fn discard(&self, path: &str, from: Option<&str>, side: Side) -> Result<()> {
        match side {
            Side::WorkTree => self.discard_work_tree(path),
            Side::Index => self.discard_index(path, from),
        }
    }

    fn discard_work_tree(&self, path: &str) -> Result<()> {
        if !self.text(&["ls-files", "--", path])?.is_empty() {
            self.run(&["checkout", "-q", "--", path])?;
        }
        Ok(())
    }

    fn discard_index(&self, path: &str, from: Option<&str>) -> Result<()> {
        for path in from.into_iter().chain([path]) {
            if self.head().is_some() && self.exists(&format!("HEAD:{path}")) {
                self.run(&["checkout", "-q", "HEAD", "--", path])?;
            } else {
                self.run(&["rm", "-q", "--cached", "--ignore-unmatch", "--", path])?;
            }
        }
        Ok(())
    }

    fn text_of_diff(&self, args: &[&str]) -> Result<String> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|source| SyncError::io(&self.program, source))?;
        match output.status.code() {
            Some(0 | 1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
            _ => Err(Error::Command(failure_text(&output.stderr)).into()),
        }
    }

    pub fn commit(&self, message: &str) -> Result<()> {
        let message = message.trim();
        if message.is_empty() {
            return Err(Error::EmptyMessage.into());
        }
        let mut args = self.identity();
        args.extend(["commit", "-q", "-m", message].map(String::from));
        self.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
            .map(drop)
    }

    fn identity(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (key, fallback) in [("user.name", FALLBACK_NAME), ("user.email", FALLBACK_EMAIL)] {
            if self.text(&["config", "--get", key]).is_err() {
                args.push("-c".to_string());
                args.push(format!("{key}={fallback}"));
            }
        }
        args
    }

    pub fn fetch(&self) -> Result<()> {
        self.run(&["fetch", "-q", "--prune", REMOTE]).map(drop)
    }

    pub fn merge(&self) -> Result<Merged> {
        let branch = self.branch()?;
        let remote = format!("refs/remotes/{REMOTE}/{branch}");
        if !self.exists(&remote) {
            return Ok(Merged::default());
        }
        let before = self.head();
        if let Some(before) = &before
            && self.count(&format!("{before}..{remote}"))? == 0
        {
            return Ok(Merged::default());
        }

        let mut merged = Merged::default();
        let mut args = self.identity();
        args.extend(["merge", "-q", "--no-edit", &remote].map(String::from));
        if let Err(error) = self.run(&args.iter().map(String::as_str).collect::<Vec<_>>()) {
            let unmerged = self.unmerged()?;
            if unmerged.is_empty() {
                return Err(local_changes(error));
            }
            merged.conflicts = self.resolve(&unmerged)?;
            let mut args = self.identity();
            args.extend(["commit", "-q", "--no-edit"].map(String::from));
            self.run(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        }

        merged.changed = match before {
            Some(before) => self.paths(&["diff", "--name-only", "-z", &before, "HEAD"])?,
            None => self.paths(&["ls-tree", "-r", "--name-only", "-z", "HEAD"])?,
        };
        Ok(merged)
    }

    pub fn push(&self) -> Result<usize> {
        let Some(_) = self.head() else {
            return Ok(0);
        };
        let branch = self.branch()?;
        let remote = format!("refs/remotes/{REMOTE}/{branch}");
        let ahead = if self.exists(&remote) {
            self.count(&format!("{remote}..HEAD"))?
        } else {
            self.count("HEAD")?
        };
        if ahead == 0 {
            return Ok(0);
        }

        match self.run(&["push", "-q", "-u", REMOTE, &branch]) {
            Ok(_) => Ok(ahead),
            Err(SyncError::Git(Error::Command(message))) if message.contains("rejected") => {
                Err(SyncError::RemoteChanged)
            }
            Err(error) => Err(error),
        }
    }

    pub fn sync(&self) -> Result<GitReport> {
        self.fetch()?;
        let merged = self.merge()?;
        let pushed = self.push()?;
        Ok(GitReport { merged, pushed })
    }

    fn resolve(&self, paths: &[String]) -> Result<Vec<GitConflict>> {
        let mut conflicts = Vec::new();
        for path in paths {
            let ours = self.stage_blob(2, path)?;
            let theirs = self.stage_blob(3, path)?;
            let file = self.root.join(path);

            match (ours, theirs) {
                (Some(ours), Some(theirs))
                    if crate::shared::is_shared(path) && self.merge.is_some() =>
                {
                    let merge = self.merge.expect("checked above");
                    let joined = merge(path, &ours, &theirs).unwrap_or(ours);
                    write(&file, &joined)?;
                    self.run(&["add", "--", path])?;
                }
                (Some(ours), Some(theirs)) => {
                    let copy = conflict_copy(path, &theirs);
                    write(&file, &ours)?;
                    write(&self.root.join(&copy), &theirs)?;
                    self.run(&["add", "--", path, &copy])?;
                    conflicts.push(GitConflict {
                        path: path.clone(),
                        copy,
                    });
                }
                (Some(kept), None) | (None, Some(kept)) => {
                    write(&file, &kept)?;
                    self.run(&["add", "--", path])?;
                }
                (None, None) => {
                    self.run(&["rm", "-q", "--cached", "--ignore-unmatch", "--", path])?;
                }
            }
        }
        Ok(conflicts)
    }

    fn stage_blob(&self, stage: u8, path: &str) -> Result<Option<Vec<u8>>> {
        match self.run(&["show", &format!(":{stage}:{path}")]) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(SyncError::Git(Error::Command(_))) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn unmerged(&self) -> Result<Vec<String>> {
        self.paths(&["diff", "--name-only", "-z", "--diff-filter=U"])
    }

    fn branch(&self) -> Result<String> {
        self.text(&["symbolic-ref", "-q", "--short", "HEAD"])
            .map(|name| name.trim().to_string())
            .map_err(|_| SyncError::from(Error::Detached))
    }

    fn head(&self) -> Option<String> {
        self.text(&["rev-parse", "-q", "--verify", "HEAD"])
            .ok()
            .map(|id| id.trim().to_string())
    }

    fn exists(&self, reference: &str) -> bool {
        self.run(&["rev-parse", "-q", "--verify", reference])
            .is_ok()
    }

    fn count(&self, range: &str) -> Result<usize> {
        let out = self.text(&["rev-list", "--count", range])?;
        Ok(out.trim().parse().unwrap_or(0))
    }

    fn paths(&self, args: &[&str]) -> Result<Vec<String>> {
        let out = self.run(args)?;
        Ok(String::from_utf8_lossy(&out)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn text(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args)?).into_owned())
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|source| match source.kind() {
                ErrorKind::NotFound | ErrorKind::PermissionDenied => {
                    Error::Missing(self.program.clone()).into()
                }
                _ => SyncError::io(&self.program, source),
            })?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(classify(failure_text(&output.stderr)).into())
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .args([
                "-c",
                "http.lowSpeedLimit=1000",
                "-c",
                "http.lowSpeedTime=30",
            ]);

        if let Some(token) = &self.token {
            command.env(TOKEN_VARIABLE, token).args([
                "-c",
                "credential.helper=",
                "-c",
                TOKEN_HELPER,
            ]);
        }
        for (name, value) in &self.env {
            command.env(name, value);
        }
        command
    }
}

fn check_url(url: &str) -> Result<()> {
    let Some((_, rest)) = url.split_once("://") else {
        return Ok(());
    };
    let authority = rest.split('/').next().unwrap_or_default();
    match authority.split_once('@') {
        Some((user, _)) if user.contains(':') || url.starts_with("http") => {
            Err(Error::UrlCredentials.into())
        }
        _ => Ok(()),
    }
}

fn is_empty(root: &Path) -> Result<bool> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(source) => return Err(SyncError::io(root, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| SyncError::io(root, source))?;
        let name = entry.file_name();
        if !SERVICE_NAMES.iter().any(|service| name == *service) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn conflict_copy(path: &str, theirs: &[u8]) -> String {
    let hash = blake3::hash(theirs).to_hex();
    let short = &hash.as_str()[..8];

    let (dir, name) = match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name),
        None => (String::new(), path),
    };
    match name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()) {
        Some((stem, extension)) => format!("{dir}{stem}{CONFLICT_MARK}{short}).{extension}"),
        None => format!("{dir}{name}{CONFLICT_MARK}{short})"),
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SyncError::io(parent, source))?;
    }
    fs::write(path, bytes).map_err(|source| SyncError::io(path, source))
}

fn failure_text(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with("hint:"))
        .collect();
    if lines.is_empty() {
        "git failed".to_string()
    } else {
        lines.join("\n")
    }
}

fn classify(message: String) -> Error {
    let has = |needles: &[&str]| needles.iter().any(|needle| message.contains(needle));
    if has(&[
        &format!("'{REMOTE}' does not appear to be a git repository"),
        &format!("No such remote '{REMOTE}'"),
    ]) {
        Error::NoOrigin
    } else if has(&[
        "Could not resolve host",
        "Could not resolve hostname",
        "Failed to connect to",
        "Connection refused",
        "Connection timed out",
        "Operation timed out",
        "Network is unreachable",
        "ssh: connect to host",
    ]) {
        Error::Unreachable(first_line(&message))
    } else if has(&[
        "could not read Username",
        "could not read Password",
        "Authentication failed",
        "Permission denied (publickey",
        "Invalid username or password",
        "The requested URL returned error: 403",
        "Host key verification failed",
    ]) {
        Error::Denied(first_line(&message))
    } else if has(&[
        "Repository not found",
        "does not appear to be a git repository",
        "The requested URL returned error: 404",
    ]) || (message.contains("repository '") && message.contains("' not found"))
    {
        Error::RepoMissing(first_line(&message))
    } else {
        Error::Command(message)
    }
}

fn first_line(message: &str) -> String {
    let line = message
        .lines()
        .find(|line| line.starts_with("fatal:") || line.starts_with("ERROR:"))
        .or_else(|| message.lines().next())
        .unwrap_or_default();
    line.trim_start_matches("fatal:")
        .trim_start_matches("ERROR:")
        .trim()
        .to_string()
}

fn local_changes(error: SyncError) -> SyncError {
    let SyncError::Git(Error::Command(message)) = &error else {
        return error;
    };
    if !message.contains("would be overwritten") {
        return error;
    }
    let files: Vec<String> = message
        .lines()
        .filter(|line| line.starts_with('\t'))
        .map(|line| line.trim().to_string())
        .collect();
    Error::LocalChanges(files).into()
}

fn kind(code: char) -> Option<ChangeKind> {
    match code {
        'M' | 'T' => Some(ChangeKind::Modified),
        'A' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'R' | 'C' => Some(ChangeKind::Renamed),
        _ => None,
    }
}

fn parse_status(out: &str) -> GitStatus {
    let mut status = GitStatus::default();
    let mut records = out.split('\0').filter(|record| !record.is_empty());

    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            if let Some(name) = header.strip_prefix("branch.head ") {
                status.branch = (name != "(detached)").then(|| name.to_string());
            } else if let Some(counts) = header.strip_prefix("branch.ab ") {
                for count in counts.split(' ') {
                    if let Some(ahead) = count.strip_prefix('+') {
                        status.ahead = ahead.parse().unwrap_or(0);
                    } else if let Some(behind) = count.strip_prefix('-') {
                        status.behind = behind.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }

        let codes = |fields: usize| -> Option<(char, char, String)> {
            let parts: Vec<&str> = record.splitn(fields, ' ').collect();
            let mut xy = parts.get(1)?.chars();
            Some((xy.next()?, xy.next()?, parts.get(fields - 1)?.to_string()))
        };

        let change = match record.chars().next() {
            Some('1') => codes(9).map(|(x, y, path)| GitChange {
                path,
                from: None,
                staged: kind(x),
                unstaged: kind(y),
            }),
            Some('2') => codes(10).map(|(x, y, path)| GitChange {
                path,
                from: records.next().map(str::to_string),
                staged: kind(x),
                unstaged: kind(y),
            }),
            Some('u') => codes(11).map(|(_, _, path)| GitChange {
                path,
                from: None,
                staged: None,
                unstaged: Some(ChangeKind::Conflicted),
            }),
            Some('?') => record.get(2..).map(|path| GitChange {
                path: path.to_string(),
                from: None,
                staged: None,
                unstaged: Some(ChangeKind::Untracked),
            }),
            _ => None,
        };
        status.changes.extend(change);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, home: &Path) -> Git {
        Git::new("git", root)
            .with_env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("HOME", home)
    }

    struct World {
        _dir: tempfile::TempDir,
        home: PathBuf,
        remote: PathBuf,
        base: PathBuf,
    }

    impl World {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().join("home");
            let remote = dir.path().join("remote.git");
            fs::create_dir_all(&home).unwrap();
            fs::write(home.join("gitconfig"), "").unwrap();
            fs::create_dir_all(&remote).unwrap();
            git(&remote, &home).run(&["init", "-q", "--bare"]).unwrap();
            World {
                base: dir.path().to_path_buf(),
                _dir: dir,
                home,
                remote,
            }
        }

        fn url(&self) -> String {
            self.remote.to_string_lossy().into_owned()
        }

        fn machine(&self, name: &str) -> Git {
            let root = self.base.join(name);
            fs::create_dir_all(&root).unwrap();
            let git = git(&root, &self.home);
            git.attach(Some(&self.url())).unwrap();
            git
        }
    }

    fn put(git: &Git, path: &str, text: &str) {
        write(&git.root().join(path), text.as_bytes()).unwrap();
    }

    fn read(git: &Git, path: &str) -> String {
        fs::read_to_string(git.root().join(path)).unwrap()
    }

    fn commit_all(git: &Git, message: &str) {
        git.run(&["add", "-A"]).unwrap();
        git.commit(message).unwrap();
    }

    #[test]
    fn an_empty_vault_attaches_to_an_empty_remote_and_pushes() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "# Оглавление\n");
        commit_all(&laptop, "первая заметка");

        let report = laptop.sync().unwrap();

        assert_eq!(report.pushed, 1);
        let desk = world.machine("desk");
        assert_eq!(read(&desk, "Index.md"), "# Оглавление\n");
        assert_eq!(desk.status().unwrap().branch.as_deref(), Some("main"));
    }

    #[test]
    fn edits_travel_both_ways_and_merge_by_parts() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "раз\nдва\nтри\n");
        commit_all(&laptop, "старт");
        laptop.sync().unwrap();
        let desk = world.machine("desk");

        put(&laptop, "Index.md", "РАЗ\nдва\nтри\n");
        commit_all(&laptop, "ноутбук");
        laptop.sync().unwrap();
        put(&desk, "Index.md", "раз\nдва\nТРИ\n");
        commit_all(&desk, "стол");

        let report = desk.sync().unwrap();

        assert!(report.merged.conflicts.is_empty());
        assert_eq!(report.merged.changed, vec!["Index.md".to_string()]);
        assert_eq!(report.pushed, 2, "свой коммит и коммит слияния");
        assert_eq!(read(&desk, "Index.md"), "РАЗ\nдва\nТРИ\n");
    }

    #[test]
    fn a_conflict_keeps_the_local_note_and_copies_the_remote_one() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Дневник/Сегодня.md", "начало\n");
        commit_all(&laptop, "старт");
        laptop.sync().unwrap();
        let desk = world.machine("desk");

        put(&laptop, "Дневник/Сегодня.md", "с ноутбука\n");
        commit_all(&laptop, "ноутбук");
        laptop.sync().unwrap();
        put(&desk, "Дневник/Сегодня.md", "со стола\n");
        commit_all(&desk, "стол");

        let report = desk.sync().unwrap();

        let [conflict] = report.merged.conflicts.as_slice() else {
            panic!("ожидался один конфликт: {report:?}");
        };
        assert_eq!(conflict.path, "Дневник/Сегодня.md");
        assert!(conflict.copy.starts_with("Дневник/Сегодня (conflict "));
        assert!(conflict.copy.ends_with(").md"));
        assert_eq!(read(&desk, "Дневник/Сегодня.md"), "со стола\n");
        assert_eq!(read(&desk, &conflict.copy), "с ноутбука\n");
        assert!(
            desk.status().unwrap().changes.is_empty(),
            "слияние закоммичено"
        );

        laptop.sync().unwrap();
        assert_eq!(read(&laptop, &conflict.copy), "с ноутбука\n");
    }

    #[test]
    fn two_edits_of_a_locked_note_give_a_copy_not_a_merge() {
        let key = kknow_crypto::locked::NotesKey::new(&kknow_crypto::DataKey::generate().unwrap());
        let world = World::new();
        let laptop = world.machine("laptop");
        put(
            &laptop,
            "Diary.md",
            &key.seal("line one\nline two\n").unwrap(),
        );
        commit_all(&laptop, "start");
        laptop.sync().unwrap();
        let desk = world.machine("desk");

        let theirs = key.seal("LINE ONE\nline two\n").unwrap();
        put(&laptop, "Diary.md", &theirs);
        commit_all(&laptop, "laptop");
        laptop.sync().unwrap();
        let mine = key.seal("line one\nLINE TWO\n").unwrap();
        put(&desk, "Diary.md", &mine);
        commit_all(&desk, "desk");

        let report = desk.sync().unwrap();
        let [conflict] = report.merged.conflicts.as_slice() else {
            panic!("one conflict was expected: {report:?}");
        };
        assert_eq!(read(&desk, "Diary.md"), mine);
        assert_eq!(read(&desk, &conflict.copy), theirs);
        assert_eq!(
            key.open(&read(&desk, &conflict.copy)).unwrap(),
            "LINE ONE\nline two\n"
        );
    }

    #[test]
    fn uncommitted_edits_in_the_way_stop_the_merge_with_their_names() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "старт\n");
        commit_all(&laptop, "старт");
        laptop.sync().unwrap();
        let desk = world.machine("desk");

        put(&laptop, "Index.md", "с ноутбука\n");
        commit_all(&laptop, "ноутбук");
        laptop.sync().unwrap();
        put(&desk, "Index.md", "правка без коммита\n");

        let error = desk.sync().unwrap_err();

        assert!(
            matches!(&error, SyncError::Git(Error::LocalChanges(files)) if files == &["Index.md".to_string()]),
            "{error:?}"
        );
        assert_eq!(
            read(&desk, "Index.md"),
            "правка без коммита\n",
            "правка цела"
        );
    }

    #[test]
    fn status_lists_the_index_and_the_work_tree_apart() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Старая.md", "a\n");
        put(&laptop, "Удалить.md", "b\n");
        commit_all(&laptop, "старт");

        put(&laptop, "Старая.md", "a2\n");
        put(&laptop, "Новая заметка.md", "c\n");
        fs::remove_file(laptop.root().join("Удалить.md")).unwrap();
        laptop.stage(&["Новая заметка.md".to_string()]).unwrap();

        let status = laptop.status().unwrap();

        let find = |path: &str| {
            status
                .changes
                .iter()
                .find(|change| change.path == path)
                .unwrap_or_else(|| panic!("нет {path} в {status:?}"))
        };
        assert_eq!(find("Новая заметка.md").staged, Some(ChangeKind::Added));
        assert_eq!(find("Старая.md").unstaged, Some(ChangeKind::Modified));
        assert_eq!(find("Удалить.md").unstaged, Some(ChangeKind::Deleted));
        assert_eq!(status.ahead, 1, "remote ещё не видел первый коммит");

        laptop.unstage(&["Новая заметка.md".to_string()]).unwrap();
        let status = laptop.status().unwrap();
        let note = status
            .changes
            .iter()
            .find(|change| change.path == "Новая заметка.md")
            .unwrap();
        assert_eq!(note.staged, None);
        assert_eq!(note.unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn unstaging_works_before_the_first_commit() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "a\n");
        laptop.stage(&["Index.md".to_string()]).unwrap();

        laptop.unstage(&["Index.md".to_string()]).unwrap();

        let status = laptop.status().unwrap();
        assert_eq!(status.changes[0].unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn a_diff_shows_each_side_and_untracked_files() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "раз\n");
        commit_all(&laptop, "старт");

        put(&laptop, "Index.md", "раз\nдва\n");
        laptop.stage(&["Index.md".to_string()]).unwrap();
        put(&laptop, "Index.md", "раз\nдва\nтри\n");
        put(&laptop, "Новая.md", "текст\n");

        let staged = laptop.diff("Index.md", None, Side::Index).unwrap();
        assert!(
            staged.contains("+два") && !staged.contains("+три"),
            "{staged}"
        );
        let unstaged = laptop.diff("Index.md", None, Side::WorkTree).unwrap();
        assert!(
            unstaged.contains("+три") && !unstaged.contains("+два"),
            "{unstaged}"
        );
        let new = laptop.diff("Новая.md", None, Side::WorkTree).unwrap();
        assert!(new.contains("new file") && new.contains("+текст"), "{new}");
    }

    #[test]
    fn a_discard_takes_one_side_back() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "раз\n");
        put(&laptop, "Старое.md", "старое\n");
        commit_all(&laptop, "старт");

        put(&laptop, "Index.md", "раз\nдва\n");
        laptop.stage(&["Index.md".to_string()]).unwrap();
        put(&laptop, "Index.md", "раз\nдва\nтри\n");
        laptop.discard("Index.md", None, Side::WorkTree).unwrap();
        assert_eq!(read(&laptop, "Index.md"), "раз\nдва\n");

        laptop.discard("Index.md", None, Side::Index).unwrap();
        assert_eq!(read(&laptop, "Index.md"), "раз\n");

        fs::rename(
            laptop.root().join("Старое.md"),
            laptop.root().join("Новое.md"),
        )
        .unwrap();
        laptop
            .stage(&["Старое.md".to_string(), "Новое.md".to_string()])
            .unwrap();
        fs::remove_file(laptop.root().join("Новое.md")).unwrap();
        laptop
            .discard("Новое.md", Some("Старое.md"), Side::Index)
            .unwrap();
        assert_eq!(read(&laptop, "Старое.md"), "старое\n");

        put(&laptop, "Новая.md", "текст\n");
        laptop.stage(&["Новая.md".to_string()]).unwrap();
        laptop.discard("Новая.md", None, Side::Index).unwrap();
        laptop.discard("Новая.md", None, Side::WorkTree).unwrap();

        let status = laptop.status().unwrap();
        assert_eq!(status.changes.len(), 1, "{status:?}");
        assert_eq!(status.changes[0].path, "Новая.md");
        assert_eq!(status.changes[0].unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn a_commit_works_without_an_identity_and_refuses_an_empty_message() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "a\n");
        laptop.stage(&["Index.md".to_string()]).unwrap();

        assert!(matches!(
            laptop.commit("  "),
            Err(SyncError::Git(Error::EmptyMessage))
        ));
        laptop.commit("заметка").unwrap();

        let author = laptop.text(&["log", "-1", "--format=%an <%ae>"]).unwrap();
        assert_eq!(author.trim(), "kknow <kknow@localhost>");
    }

    #[test]
    fn the_machine_settings_stay_out_of_the_repository() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, ".kknow/remote.json", "{}");
        put(&laptop, ".kknow/sync.json", "{}");
        put(&laptop, ".kknow/bookmarks.json", "[]");
        put(&laptop, ".trash/старое.md", "x");
        put(&laptop, ".kknow/trash/1700000000/старое.md", "x");

        let paths: Vec<String> = laptop
            .status()
            .unwrap()
            .changes
            .into_iter()
            .map(|change| change.path)
            .collect();

        assert_eq!(paths, vec![".kknow/bookmarks.json".to_string()]);

        laptop.attach(None).unwrap();
        let exclude = fs::read_to_string(laptop.root().join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches("/.kknow/remote.json").count(), 1);
    }

    #[test]
    fn an_existing_repository_gives_its_origin() {
        let world = World::new();
        let laptop = world.machine("laptop");

        assert_eq!(laptop.attach(None).unwrap(), world.url());
        assert_eq!(
            laptop
                .attach(Some("https://example.com/other.git"))
                .unwrap(),
            world.url(),
            "адрес берётся из origin"
        );
    }

    #[test]
    fn a_folder_with_notes_and_no_git_is_refused() {
        let world = World::new();
        let root = world.base.join("notes");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Index.md"), "a").unwrap();
        let git = git(&root, &world.home);

        assert!(matches!(
            git.attach(Some(&world.url())),
            Err(SyncError::Git(Error::NotEmpty))
        ));
        assert!(!root.join(".git").exists());
        assert!(matches!(
            git.attach(None),
            Err(SyncError::Config(config::Error::NoGitUrl))
        ));
    }

    #[test]
    fn a_wrong_address_leaves_no_half_made_repository() {
        let world = World::new();
        let root = world.base.join("empty");
        fs::create_dir_all(root.join(".kknow")).unwrap();
        let git = git(&root, &world.home);

        let missing = world.base.join("нет-такого.git");
        assert!(matches!(
            git.attach(Some(&missing.to_string_lossy())),
            Err(SyncError::Git(Error::RepoMissing(_)))
        ));
        assert!(!root.join(".git").exists());
    }

    #[test]
    fn a_sync_without_origin_says_so() {
        let world = World::new();
        let laptop = world.machine("laptop");
        laptop.run(&["remote", "remove", REMOTE]).unwrap();

        assert!(matches!(
            laptop.sync(),
            Err(SyncError::Git(Error::NoOrigin))
        ));
        assert_eq!(laptop.origin().unwrap(), None);
    }

    #[test]
    fn a_remote_that_is_gone_is_named() {
        let world = World::new();
        let laptop = world.machine("laptop");
        fs::remove_dir_all(&world.remote).unwrap();

        let error = laptop.sync().unwrap_err();
        assert!(
            matches!(error, SyncError::Git(Error::RepoMissing(_))),
            "{error:?}"
        );
        assert!(error.to_string().contains("not found"), "{error}");
    }

    #[test]
    fn network_and_sign_in_failures_are_named() {
        let cases = [
            (
                "fatal: unable to access 'https://no-such-host.invalid/r.git/': Could not resolve host: no-such-host.invalid",
                "unreachable",
            ),
            (
                "ssh: connect to host example.com port 22: Operation timed out\nfatal: Could not read from remote repository.",
                "unreachable",
            ),
            (
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
                "denied",
            ),
            (
                "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.",
                "denied",
            ),
            (
                "remote: Invalid username or token.\nfatal: Authentication failed for 'https://github.com/me/notes.git/'",
                "denied",
            ),
            (
                "ERROR: Repository not found.\nfatal: Could not read from remote repository.",
                "missing",
            ),
            (
                "remote: Repository not found.\nfatal: repository 'https://github.com/me/nope.git/' not found",
                "missing",
            ),
            (
                "fatal: 'origin' does not appear to be a git repository\nfatal: Could not read from remote repository.",
                "origin",
            ),
            ("CONFLICT (content): Merge conflict in a.md", "other"),
        ];
        for (text, expected) in cases {
            let found = match classify(text.to_string()) {
                Error::Unreachable(_) => "unreachable",
                Error::Denied(_) => "denied",
                Error::RepoMissing(_) => "missing",
                Error::NoOrigin => "origin",
                Error::Command(_) => "other",
                other => panic!("{other:?}"),
            };
            assert_eq!(found, expected, "{text}");
        }
        assert_eq!(
            first_line("remote: x\nfatal: Authentication failed for 'u'"),
            "Authentication failed for 'u'"
        );
    }

    #[test]
    fn the_token_reaches_git_through_the_helper_only() {
        use std::io::Write;

        let world = World::new();
        let laptop = world.machine("laptop").with_token(Some("ghp_тайна".into()));
        let mut child = laptop
            .command()
            .args(["credential", "fill"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"protocol=https\nhost=github.com\n\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let answer = String::from_utf8(out.stdout).unwrap();

        assert!(answer.contains("password=ghp_тайна"), "{answer}");
        let config = fs::read_to_string(laptop.root().join(".git/config")).unwrap();
        assert!(!config.contains("тайна"));
    }

    #[test]
    fn a_token_in_the_url_is_refused() {
        assert!(check_url("https://ghp_abc@github.com/me/notes.git").is_err());
        assert!(check_url("https://me:pass@example.com/notes.git").is_err());
        assert!(check_url("https://github.com/me/notes.git").is_ok());
        assert!(check_url("ssh://git@github.com/me/notes.git").is_ok());
        assert!(check_url("git@github.com:me/notes.git").is_ok());
    }

    #[test]
    fn a_missing_program_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new("/нет/такого/git", dir.path());

        assert!(matches!(
            git.version(),
            Err(SyncError::Git(Error::Missing(_)))
        ));
    }

    #[test]
    fn conflict_copies_keep_the_folder_and_the_extension() {
        let copy = conflict_copy("Папка/Заметка.md", b"x");
        assert!(copy.starts_with("Папка/Заметка (conflict "), "{copy}");
        assert!(copy.ends_with(").md"), "{copy}");
        assert_eq!(
            copy,
            conflict_copy("Папка/Заметка.md", b"x"),
            "стабильное имя"
        );

        assert!(conflict_copy("Makefile", b"x").ends_with(')'));
        assert!(conflict_copy(".kknow/bookmarks.json", b"x").ends_with(").json"));
    }

    #[test]
    fn status_parses_renames_and_spaces() {
        let out = "# branch.oid abc\0# branch.head main\0# branch.ab +2 -1\0\
                   2 R. N... 100644 100644 100644 a b R100 Новая папка/Имя с пробелом.md\0Старое имя.md\0\
                   ? картинки/фото 1.png\0";

        let status = parse_status(out);

        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
        assert_eq!(
            status.changes[0],
            GitChange {
                path: "Новая папка/Имя с пробелом.md".into(),
                from: Some("Старое имя.md".into()),
                staged: Some(ChangeKind::Renamed),
                unstaged: None,
            }
        );
        assert_eq!(status.changes[1].path, "картинки/фото 1.png");
    }
}
