//! A git repository as the other end of sync.
//!
//! Git does not fit [`SyncProvider`](crate::SyncProvider): it has commits,
//! branches and its own merge, so it is a different algorithm, not one more
//! implementation of the trait. It also is not ours to write again: kknow runs
//! the `git` program of the user. Thus SSH keys, credential helpers,
//! `~/.gitconfig` and LFS work as they work in the terminal.
//!
//! The rules:
//!
//! - one vault is one repository, and the vault root is the repository root;
//! - the person stages and commits by hand; sync is fetch, merge and push;
//! - a conflict leaves no `<<<<<<<` marks in a note. The local version stays
//!   in place and the remote one goes to a copy next to it, as with the other
//!   providers ([`CONFLICT_MARK`]);
//! - the settings of this machine (`.kknow/remote.json`, `.kknow/sync.json`)
//!   and the trash go to `.git/info/exclude`. The rest of `.kknow` (bookmarks,
//!   themes, commands) is part of the repository.
//!
//! All operations are synchronous, as the rest of the sync core. The network
//! steps ([`Git::fetch`], [`Git::push`]) do not touch the files, and
//! [`Git::merge`] does not use the network. So the window holds the vault lock
//! only for the merge.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::plan::CONFLICT_MARK;
use crate::{Error, Result};

/// The environment variable that carries the token to the credential helper.
/// The token never goes into the arguments, the URL or `.git/config`.
const TOKEN_VARIABLE: &str = "KKNOW_GIT_TOKEN";

/// A credential helper for one run of git: it answers with the token.
///
/// The empty helper before it removes the helpers of the user for this run.
/// Otherwise git would also save our token in the keychain of the user. The
/// user name does not matter for GitHub and GitLab tokens, but it must not be
/// empty.
const TOKEN_HELPER: &str = "credential.helper=!f() { echo username=x-access-token; echo \"password=$KKNOW_GIT_TOKEN\"; }; f";

/// The lines that kknow adds to `.git/info/exclude`.
///
/// This file is local: kknow does not change the `.gitignore` of the user,
/// and the rule does not travel to other machines.
const EXCLUDE: &[&str] = &[
    "/.kknow/remote.json",
    "/.kknow/sync.json",
    "/.kknow/*.tmp",
    "/.trash/",
];

const EXCLUDE_HEADER: &str = "# kknow: the settings of this machine and the trash";

/// Names that do not make a folder "not empty" for a clone: the service
/// folders of kknow and the file that Finder leaves everywhere.
const SERVICE_NAMES: &[&str] = &[".kknow", ".trash", ".DS_Store"];

/// The name of the remote. kknow works with one remote and does not ask.
const REMOTE: &str = "origin";

/// The branch for a new repository when the remote one is empty.
const DEFAULT_BRANCH: &str = "main";

/// The identity for a commit when git has none. Without it, `git commit`
/// fails on a fresh machine, and the message about `user.email` does not
/// help a person who only writes notes.
const FALLBACK_NAME: &str = "kknow";
const FALLBACK_EMAIL: &str = "kknow@localhost";

/// The `git` program and the vault it works in.
#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    root: PathBuf,
    token: Option<String>,
    env: Vec<(String, String)>,
}

/// What changed in one file, separately for the index and the work tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    /// A new file that git does not track yet. It is only in the work tree.
    Untracked,
    /// A merge stopped in this file. kknow resolves conflicts itself, so this
    /// shows only after a merge that someone ran outside kknow.
    Conflicted,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitChange {
    /// The path from the vault root with `/`.
    pub path: String,
    /// The old path of a renamed file.
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub from: Option<String>,
    /// The change in the index: what the next commit takes.
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub staged: Option<ChangeKind>,
    /// The change in the work tree that is not in the index.
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub unstaged: Option<ChangeKind>,
}

/// The state of the repository for the changes panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitStatus {
    /// The current branch; `None` for a detached HEAD.
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub branch: Option<String>,
    /// Local commits that the remote does not have yet, as of the last fetch.
    pub ahead: usize,
    /// Remote commits that are not here yet, as of the last fetch.
    pub behind: usize,
    pub changes: Vec<GitChange>,
}

/// A conflict that the merge resolved with a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConflict {
    /// The file that keeps the local version.
    pub path: String,
    /// The copy with the remote version.
    pub copy: String,
}

/// What a merge did to the files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merged {
    /// Files that the merge changed, added or removed.
    pub changed: Vec<String>,
    pub conflicts: Vec<GitConflict>,
}

/// What a whole sync did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitReport {
    pub merged: Merged,
    /// Commits that went to the remote.
    pub pushed: usize,
}

impl Git {
    pub fn new(program: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Git {
            program: program.into(),
            root: root.into(),
            token: None,
            env: Vec::new(),
        }
    }

    /// The token for HTTPS. `None` leaves the sign-in to git: its credential
    /// helper or the SSH agent.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|token| !token.is_empty());
        self
    }

    /// One more variable for each run of git. The tests use it to hide the
    /// configuration of the person who runs them.
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

    /// `git version`: the check that the path in the settings is git.
    pub fn version(&self) -> Result<String> {
        let out = self.text(&["version"])?;
        Ok(out.trim().to_string())
    }

    /// Whether the vault is already a repository.
    pub fn is_repository(root: &Path) -> bool {
        root.join(".git").exists()
    }

    /// The URL of `origin`, if the repository has it.
    pub fn origin(&self) -> Result<Option<String>> {
        match self.text(&["remote", "get-url", REMOTE]) {
            Ok(url) => Ok(Some(url.trim().to_string())),
            Err(Error::Git(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Connects the vault to a repository and gives its URL.
    ///
    /// There are two cases, and all others are an error:
    ///
    /// - the vault is already a repository: the URL comes from `origin`, and
    ///   `url` matters only if there is no `origin` yet;
    /// - the vault is empty: kknow makes a repository and checks out the
    ///   default branch of `url`. An empty remote repository is also fine.
    ///
    /// A folder with notes and no `.git` is refused. Two histories of the same
    /// notes cannot merge without questions that kknow cannot ask well.
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
                (None, None) => return Err(Error::GitNoOrigin),
            };
            self.exclude()?;
            return Ok(url);
        }

        let url = url.ok_or(Error::NoGitUrl)?;
        if !is_empty(&self.root)? {
            return Err(Error::GitNotEmpty);
        }

        // Ask the remote first. If the address or the token is wrong, the
        // vault stays as it was, without a half-made `.git`.
        let branch = self.remote_default_branch(url)?;

        self.run(&["init", "-q"])?;
        let head = format!("refs/heads/{}", branch.as_deref().unwrap_or(DEFAULT_BRANCH));
        self.run(&["symbolic-ref", "HEAD", &head])?;
        self.run(&["remote", "add", REMOTE, url])?;
        self.exclude()?;

        if let Some(branch) = branch {
            self.fetch()?;
            let tracked = format!("{REMOTE}/{branch}");
            // `-f`: the service files of kknow can be in the folder already,
            // and the remote versions win. The folder had no notes.
            self.run(&["checkout", "-q", "-f", "-B", &branch, "--track", &tracked])?;
        }
        Ok(url.to_string())
    }

    /// The vault must be the root of its repository: one vault, one
    /// repository. A vault inside a code repository would commit the code too.
    fn check_root(&self) -> Result<()> {
        let top = self.text(&["rev-parse", "--show-toplevel"])?;
        let top = fs::canonicalize(top.trim()).map_err(|source| Error::io(top.trim(), source))?;
        let root = fs::canonicalize(&self.root).map_err(|source| Error::io(&self.root, source))?;
        if top == root {
            Ok(())
        } else {
            Err(Error::GitNotRoot(top))
        }
    }

    /// The default branch of a remote repository; `None` if it is empty.
    ///
    /// Usually the remote `HEAD` names it. A bare repository made by hand can
    /// have a `HEAD` that points to a branch nobody pushed; then `main`,
    /// `master` or the first branch, in this order.
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

    /// Adds the kknow lines to `.git/info/exclude`, once.
    pub fn exclude(&self) -> Result<()> {
        let path = self.root.join(".git/info/exclude");
        let current = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(source) => return Err(Error::io(&path, source)),
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
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        fs::write(&path, text).map_err(|source| Error::io(&path, source))
    }

    /// Changed files, the branch and the distance to the remote.
    pub fn status(&self) -> Result<GitStatus> {
        let out = self.run(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ])?;
        let mut status = parse_status(&String::from_utf8_lossy(&out));

        // Before the first push the branch has no upstream, and git gives no
        // distance. All local commits are then unpushed.
        if let Some(branch) = &status.branch
            && !self.exists(&format!("refs/remotes/{REMOTE}/{branch}"))
            && self.head().is_some()
        {
            status.ahead = self.count("HEAD")?;
        }
        Ok(status)
    }

    /// Adds files to the index. A deleted file goes in as a deletion.
    pub fn stage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["add", "-A", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    /// Takes files out of the index. The work tree stays as it is.
    ///
    /// `git reset` and not `git restore --staged`: the second one fails before
    /// the first commit, because there is no HEAD to restore from.
    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["reset", "-q", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.run(&args).map(drop)
    }

    /// The unified diff of one file: of the index against HEAD when `staged`,
    /// else of the work tree against the index. `from` is the old path of a
    /// rename, so the diff shows the rename and not a new file.
    ///
    /// An untracked file is not in the index, and `git diff` does not see it.
    /// For it the diff compares the file with nothing (`--no-index`).
    pub fn diff(&self, path: &str, from: Option<&str>, staged: bool) -> Result<String> {
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
        if staged {
            args.push("--cached");
        } else if self.text(&["ls-files", "--", path])?.is_empty() {
            args.extend(["--no-index", "--", "/dev/null", path]);
            return self.text_of_diff(&args);
        }
        args.push("--");
        args.extend(from);
        args.push(path);
        self.text(&args)
    }

    /// `git diff --no-index` exits with 1 when the files differ, and that is
    /// the usual case here, not an error.
    fn text_of_diff(&self, args: &[&str]) -> Result<String> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|source| Error::io(&self.program, source))?;
        match output.status.code() {
            Some(0 | 1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
            _ => Err(Error::Git(failure_text(&output.stderr))),
        }
    }

    /// Commits the index.
    pub fn commit(&self, message: &str) -> Result<()> {
        let message = message.trim();
        if message.is_empty() {
            return Err(Error::GitEmptyMessage);
        }
        let mut args = self.identity();
        args.extend(["commit", "-q", "-m", message].map(String::from));
        self.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
            .map(drop)
    }

    /// `-c user.name=… -c user.email=…` when git has no identity of its own.
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

    /// Network, part one: gets the remote commits. The files do not change.
    pub fn fetch(&self) -> Result<()> {
        self.run(&["fetch", "-q", "--prune", REMOTE]).map(drop)
    }

    /// Merges the remote branch into the current one. No network.
    ///
    /// git merges edits in different parts of a note by itself. Only the
    /// parts that both sides changed are a conflict, and then the whole local
    /// file stays and the remote one goes to a copy.
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
        // A merge commit needs an identity, as a commit does.
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

    /// Network, part two: sends the local commits. Gives their count.
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
            // Someone pushed between our fetch and our push.
            Err(Error::Git(message)) if message.contains("rejected") => Err(Error::RemoteChanged),
            Err(error) => Err(error),
        }
    }

    /// The whole sync for the terminal: fetch, merge, push. The window runs
    /// the three steps itself, so that it holds the vault only for the merge.
    pub fn sync(&self) -> Result<GitReport> {
        self.fetch()?;
        let merged = self.merge()?;
        let pushed = self.push()?;
        Ok(GitReport { merged, pushed })
    }

    /// Writes both versions of each conflicted file and adds them to the index.
    fn resolve(&self, paths: &[String]) -> Result<Vec<GitConflict>> {
        let mut conflicts = Vec::new();
        for path in paths {
            let ours = self.stage_blob(2, path)?;
            let theirs = self.stage_blob(3, path)?;
            let file = self.root.join(path);

            match (ours, theirs) {
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
                // One side deleted the file and the other changed it. The
                // changed version stays: nothing is lost.
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

    /// The content of a file at a merge stage: 2 is ours, 3 is theirs.
    fn stage_blob(&self, stage: u8, path: &str) -> Result<Option<Vec<u8>>> {
        match self.run(&["show", &format!(":{stage}:{path}")]) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(Error::Git(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn unmerged(&self) -> Result<Vec<String>> {
        self.paths(&["diff", "--name-only", "-z", "--diff-filter=U"])
    }

    fn branch(&self) -> Result<String> {
        self.text(&["symbolic-ref", "-q", "--short", "HEAD"])
            .map(|name| name.trim().to_string())
            .map_err(|_| Error::GitDetached)
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

    /// Runs git and gives its standard output.
    fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = self
            .command()
            .args(args)
            .output()
            .map_err(|source| match source.kind() {
                ErrorKind::NotFound | ErrorKind::PermissionDenied => {
                    Error::GitMissing(self.program.clone())
                }
                _ => Error::io(&self.program, source),
            })?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Error::Git(failure_text(&output.stderr)))
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(&self.root)
            .stdin(Stdio::null())
            // No questions: there is nobody at a terminal to answer them, and
            // git would wait forever.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            // A stalled connection fails after half a minute instead of never.
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

/// A token in the URL would go to `.git/config` and `.kknow/remote.json` as
/// plain text. The token has its own field for this reason.
fn check_url(url: &str) -> Result<()> {
    let Some((_, rest)) = url.split_once("://") else {
        return Ok(());
    };
    let authority = rest.split('/').next().unwrap_or_default();
    match authority.split_once('@') {
        Some((user, _)) if user.contains(':') || url.starts_with("http") => {
            Err(Error::GitUrlCredentials)
        }
        _ => Ok(()),
    }
}

/// Whether a folder has nothing but service files.
fn is_empty(root: &Path) -> Result<bool> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(source) => return Err(Error::io(root, source)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| Error::io(root, source))?;
        let name = entry.file_name();
        if !SERVICE_NAMES.iter().any(|service| name == *service) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The path of the copy for the remote version: `dir/Note (конфликт 3f9a1c2b).md`.
///
/// The same mark as the other providers use, so the window finds the copies in
/// the same way. The hash makes the name stable: the same version gives the
/// same copy.
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
        fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    fs::write(path, bytes).map_err(|source| Error::io(path, source))
}

/// The error text of git without its hints: a hint is advice for the terminal,
/// not for a person in the window.
fn failure_text(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with("hint:"))
        .collect();
    if lines.is_empty() {
        "git завершился с ошибкой".to_string()
    } else {
        lines.join("\n")
    }
}

/// A merge that did not start: uncommitted edits are in files that the merge
/// must change. The file names come from the git message, one per line with a
/// tab before it.
fn local_changes(error: Error) -> Error {
    let Error::Git(message) = &error else {
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
    Error::GitLocalChanges(files)
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

/// Reads `git status --porcelain=v2 -z --branch`.
///
/// The format is for programs: fields split by spaces, records by NUL, and
/// the path is the last field, so a space in a name does not break it. A
/// rename has its old path in the next record.
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

    /// git with the configuration of the person who runs the tests hidden:
    /// no global identity, no credential helper, no default branch name.
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

        /// A vault on one more machine, connected to the remote.
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

        // Different lines on two machines: git merges them itself.
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
        assert!(conflict.copy.starts_with("Дневник/Сегодня (конфликт "));
        assert!(conflict.copy.ends_with(").md"));
        assert_eq!(read(&desk, "Дневник/Сегодня.md"), "со стола\n");
        assert_eq!(read(&desk, &conflict.copy), "с ноутбука\n");
        assert!(
            desk.status().unwrap().changes.is_empty(),
            "слияние закоммичено"
        );

        // The other machine gets both versions with the next sync.
        laptop.sync().unwrap();
        assert_eq!(read(&laptop, &conflict.copy), "с ноутбука\n");
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
            matches!(&error, Error::GitLocalChanges(files) if files == &["Index.md".to_string()]),
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

        let staged = laptop.diff("Index.md", None, true).unwrap();
        assert!(staged.contains("+два") && !staged.contains("+три"), "{staged}");
        let unstaged = laptop.diff("Index.md", None, false).unwrap();
        assert!(unstaged.contains("+три") && !unstaged.contains("+два"), "{unstaged}");
        let new = laptop.diff("Новая.md", None, false).unwrap();
        assert!(new.contains("new file") && new.contains("+текст"), "{new}");
    }

    #[test]
    fn a_commit_works_without_an_identity_and_refuses_an_empty_message() {
        let world = World::new();
        let laptop = world.machine("laptop");
        put(&laptop, "Index.md", "a\n");
        laptop.stage(&["Index.md".to_string()]).unwrap();

        assert!(matches!(laptop.commit("  "), Err(Error::GitEmptyMessage)));
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

        let paths: Vec<String> = laptop
            .status()
            .unwrap()
            .changes
            .into_iter()
            .map(|change| change.path)
            .collect();

        assert_eq!(paths, vec![".kknow/bookmarks.json".to_string()]);

        // A second attach does not repeat the lines.
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
            Err(Error::GitNotEmpty)
        ));
        assert!(!root.join(".git").exists());
        assert!(matches!(git.attach(None), Err(Error::NoGitUrl)));
    }

    #[test]
    fn a_wrong_address_leaves_no_half_made_repository() {
        let world = World::new();
        let root = world.base.join("empty");
        fs::create_dir_all(root.join(".kknow")).unwrap();
        let git = git(&root, &world.home);

        let missing = world.base.join("нет-такого.git");
        assert!(git.attach(Some(&missing.to_string_lossy())).is_err());
        assert!(!root.join(".git").exists());
    }

    /// The helper is shell text inside a `-c` value: a wrong quote would give
    /// git an empty password. `git credential fill` asks it as a push would.
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

        assert!(matches!(git.version(), Err(Error::GitMissing(_))));
    }

    #[test]
    fn conflict_copies_keep_the_folder_and_the_extension() {
        let copy = conflict_copy("Папка/Заметка.md", b"x");
        assert!(copy.starts_with("Папка/Заметка (конфликт "), "{copy}");
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
