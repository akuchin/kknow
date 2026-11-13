//! A thin wrapper over the core: open a vault, parse notes, show links.
//!
//! The CLI exists so that the core can grow and be tried without the GUI.

use kknow_app::proposals::{EditState, Review, Status as ProposalStatus};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use clap::{Args, Parser, Subcommand, ValueEnum};
use kknow_app::Workspace;
use kknow_app::bookmarks::clean_name;
use kknow_app::text::{backlink_row, describe, plain_snippet, task_row};
use kknow_app::theme::{self, Base, Source};
use kknow_app::vaults::VaultRegistry;
use kknow_core::NoteId;
use kknow_index::Task;
use kknow_run::{CommandSet, CommandSpec, Outcome, Target, TargetKind};
use kknow_sync::{ConflictPolicy, Git, GitAuth, GitSettings, Provider, RemoteConfig, S3Settings};

mod mcp;

#[derive(Parser)]
#[command(
    name = "kknow",
    version,
    about = "Markdown knowledge base: links, tags, structure"
)]
struct Cli {
    /// Notes directory or vault name from the list (`kknow vaults`).
    #[arg(long, short, global = true, env = "KKNOW_VAULT", default_value = ".")]
    vault: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List notes.
    List,
    /// Show vault statistics.
    Stats,
    /// Show outgoing links of a note.
    Links { note: String },
    /// Show notes that link to a note.
    Backlinks { note: String },
    /// List tags and their counts.
    Tags,
    /// List links to notes that do not exist.
    Broken,
    /// List notes with no incoming links.
    Orphans,
    /// Show the parsed structure of a note.
    Show { note: String },
    /// Render a note to HTML.
    Render { note: String },
    /// Create an empty note.
    New { name: String },
    /// Lock a note with the vault password: the file becomes ciphertext.
    ///
    /// The password comes from `KKNOW_LOCK_PASSPHRASE` or a prompt. A vault
    /// without a password gets one, and kknow prints the recovery code once.
    Lock { note: String },
    /// Remove the lock of a note: the file becomes plain text again.
    ///
    /// The password or the recovery code comes from `KKNOW_LOCK_PASSPHRASE`
    /// or a prompt.
    Unlock { note: String },
    /// Full-text search: `kknow search "tag:rust notes"`.
    Search {
        query: Vec<String>,
        /// Number of results to show.
        #[arg(long, short, default_value_t = 10)]
        limit: usize,
    },
    /// List themes found in the directory and their problems.
    Themes,
    /// List vault bookmarks: `note:line  «name»  line text`.
    Bookmarks,
    /// Add or remove a bookmark on a note line: `kknow bookmark plan 12`.
    ///
    /// With `--name`, names the bookmark instead and sets it if needed:
    /// `kknow bookmark plan 12 --name "summary"`. An empty name removes the name.
    Bookmark {
        note: String,
        /// Line number, starting from 1.
        line: usize,
        /// Bookmark name.
        #[arg(long)]
        name: Option<String>,
    },
    /// List open tasks (`- [ ]`) of all notes: `note:line  text`.
    Tasks {
        /// Also list done tasks, with `[x]` and `[ ]` before the text.
        #[arg(long)]
        all: bool,
    },
    /// Edits that agents proposed through MCP and that wait for review.
    Proposals {
        #[command(subcommand)]
        action: Option<ProposalAction>,
    },
    /// Vaults that kknow knows: the same list as in the window.
    Vaults {
        #[command(subcommand)]
        action: Option<VaultAction>,
    },
    /// Commands attached to notes, folders, and vaults.
    ///
    /// Without a target, shows the whole file with the scope of each command.
    /// With a target, shows only the commands for it.
    Commands {
        /// Note or folder. Without it, the target is the whole vault.
        target: Option<String>,
        /// Target kind, if the name matches both a note and a folder.
        #[arg(long, value_enum)]
        kind: Option<TargetChoice>,
        /// A markdown file outside any vault, as a note target.
        #[arg(long, conflicts_with_all = ["target", "kind"])]
        file: Option<PathBuf>,
        /// Write the shipped commands to the file if it does not exist yet.
        #[arg(long)]
        example: bool,
    },
    /// Run a command on a note, folder, or vault.
    ///
    /// The script gets the target path in `KKNOW_PATH`, and also `KKNOW_NOTE`,
    /// `KKNOW_RELATIVE`, `KKNOW_VAULT`, and others (`kknow commands`).
    /// The exit code of the command becomes the exit code of kknow.
    Run {
        /// Command ID from `kknow commands`.
        command: String,
        /// Note or folder. Without it, the target is the whole vault.
        target: Option<String>,
        /// Target kind, if the name matches both a note and a folder.
        #[arg(long, value_enum)]
        kind: Option<TargetChoice>,
        /// A markdown file outside any vault, as a note target.
        #[arg(long, conflicts_with_all = ["target", "kind"])]
        file: Option<PathBuf>,
    },
    /// Deleted notes and folders: list, restore, empty.
    ///
    /// Entries older than 30 days go away on the next delete.
    Trash {
        #[command(subcommand)]
        action: Option<TrashAction>,
    },
    /// Watch the directory and print changes.
    Watch,
    /// Serve vaults to an agent over MCP on stdin and stdout.
    ///
    /// The agent starts the process itself, for example
    /// `claude mcp add kknow -- kknow mcp --vault notes`. With `--label`, the
    /// server takes all vaults with the label instead of `--vault`:
    /// `kknow mcp --label work`.
    Mcp {
        /// Vault label from `kknow vaults`. Repeat it for several labels: a
        /// vault needs one of them.
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    /// Sync the vault with S3-compatible storage, a shared folder or a git
    /// repository.
    Sync {
        /// Show the plan and change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Which version wins if both sides changed a note.
        /// The default comes from the directory settings.
        #[arg(long, value_enum)]
        on_conflict: Option<OnConflict>,
        /// Sync through a shared folder, for example a cloud drive folder.
        #[arg(long, env = "KKNOW_FOLDER", conflicts_with = "bucket")]
        folder: Option<PathBuf>,
        #[command(flatten)]
        s3: S3Args,
        /// Encryption of the storage. Without a subcommand, sync. An
        /// encrypted storage needs the passphrase: `KKNOW_SYNC_PASSPHRASE`
        /// or a prompt.
        #[command(subcommand)]
        action: Option<SyncAction>,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// Turn on encryption: a new key, then all notes in the storage are
    /// encrypted. Prints the recovery code one time.
    Encrypt,
    /// Set a new passphrase. The notes and the recovery code stay as they
    /// are. The new passphrase comes from `KKNOW_SYNC_NEW_PASSPHRASE` or a
    /// prompt.
    Passphrase,
}

#[derive(Subcommand)]
enum VaultAction {
    /// List vaults. The same as `kknow vaults` without a subcommand.
    List,
    /// Add a directory to the list.
    Add {
        path: PathBuf,
        /// Vault name. The default is the directory name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a vault from the list. The files on disk stay.
    Rm { vault: String },
    /// Make a vault active: the window opens with it.
    Use { vault: String },
    /// Set the labels of a vault: `kknow vaults label Personal work ai`.
    ///
    /// The list replaces the old one; without labels, the vault has none.
    /// `kknow mcp --label work` serves all vaults with the label.
    Label { vault: String, labels: Vec<String> },
}

#[derive(Subcommand)]
enum ProposalAction {
    /// List proposals. The same as `kknow proposals` without a subcommand.
    List {
        /// Also list reviewed proposals.
        #[arg(long)]
        all: bool,
    },
    /// Apply the edits of a proposal to the note.
    Accept {
        /// Proposal ID from `kknow proposals`.
        id: String,
        /// Only these edits, by number: `--only 1,3`. The other waiting edits
        /// are rejected.
        #[arg(long, value_delimiter = ',')]
        only: Vec<usize>,
        /// A note for the agent.
        #[arg(long)]
        feedback: Option<String>,
    },
    /// Reject all waiting edits of a proposal.
    Reject {
        id: String,
        /// Why: the agent reads it and tries again.
        #[arg(long)]
        feedback: Option<String>,
    },
}

#[derive(Subcommand)]
enum TrashAction {
    /// List entries. The same as `kknow trash` without a subcommand.
    List,
    /// Put an entry back. Overwrites no note.
    Restore {
        /// Entry ID from `kknow trash`.
        entry: String,
    },
    /// Remove all entries for good.
    Empty,
}

/// What to run a command on. The same as `TargetKind` in the core, but with
/// words for the terminal.
#[derive(Clone, Copy, ValueEnum)]
enum TargetChoice {
    /// A note: `KKNOW_PATH` is its file.
    Note,
    /// A folder inside the notes directory.
    Folder,
    /// The whole notes directory.
    Vault,
}

impl From<TargetChoice> for TargetKind {
    fn from(choice: TargetChoice) -> Self {
        match choice {
            TargetChoice::Note => TargetKind::Note,
            TargetChoice::Folder => TargetKind::Folder,
            TargetChoice::Vault => TargetKind::Vault,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum OnConflict {
    /// Keep both versions: the remote one goes next to it as a copy.
    Both,
    /// Keep the version on this device.
    Local,
    /// Keep the version from the storage.
    Remote,
}

impl From<OnConflict> for ConflictPolicy {
    fn from(choice: OnConflict) -> Self {
        match choice {
            OnConflict::Both => ConflictPolicy::KeepBoth,
            OnConflict::Local => ConflictPolicy::PreferLocal,
            OnConflict::Remote => ConflictPolicy::PreferRemote,
        }
    }
}

/// Where to sync.
///
/// There are no flags for access keys on purpose: `ps` shows the command
/// line, and the shell history keeps it. kknow reads the keys from
/// `KKNOW_S3_ACCESS_KEY` and `KKNOW_S3_SECRET_KEY`. If they are not set, it
/// reads the usual `AWS_*` variables, like any other AWS tool.
#[derive(Args)]
struct S3Args {
    /// S3 bucket.
    #[arg(long, env = "KKNOW_S3_BUCKET")]
    bucket: Option<String>,
    /// Storage address. For MinIO, this is http://localhost:9000.
    #[arg(long, env = "KKNOW_S3_ENDPOINT")]
    endpoint: Option<String>,
    /// Region. The default is us-east-1.
    #[arg(long, env = "KKNOW_S3_REGION")]
    region: Option<String>,
    /// Prefix inside the bucket, so one bucket can hold several vaults.
    #[arg(long, env = "KKNOW_S3_PREFIX")]
    prefix: Option<String>,
}

/// Where to sync: from the flags. If no bucket or folder is given, from the
/// directory settings, which the window also reads.
fn destination(
    workspace: &Workspace,
    folder: Option<PathBuf>,
    s3: S3Args,
) -> Result<RemoteConfig, String> {
    if let Some(path) = folder {
        return Ok(RemoteConfig::folder(path));
    }

    if let Some(bucket) = s3.bucket {
        return Ok(RemoteConfig::s3(S3Settings {
            bucket,
            endpoint: s3.endpoint,
            region: s3.region,
            prefix: s3.prefix,
            access_key_id: std::env::var("KKNOW_S3_ACCESS_KEY").ok(),
        }));
    }

    RemoteConfig::load(&RemoteConfig::path(workspace.root()))
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no storage is set: use --bucket, --folder or the folder settings".to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// kknow has its own exit code, except for `kknow run`: there the command owns it.
fn run(cli: Cli) -> Result<ExitCode, String> {
    // The vault list is outside the notes directories, so handle it before
    // anything opens: `kknow vaults` works where there is no vault.
    if let Command::Vaults { action } = cli.command {
        vaults(action.unwrap_or(VaultAction::List))?;
        return Ok(ExitCode::SUCCESS);
    }
    // The server can serve several vaults, so it opens them itself.
    if let Command::Mcp { labels } = &cli.command {
        serve_mcp(&cli.vault, labels)?;
        return Ok(ExitCode::SUCCESS);
    }

    // A standalone file is a note without a vault, so its commands need none.
    // `README.md` in a repository must not need `--vault`.
    match &cli.command {
        Command::Commands {
            file: Some(file),
            example,
            ..
        } => {
            let target =
                kknow_app::commands::file_target(file).map_err(|error| error.to_string())?;
            commands(Some(target), *example)?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Run {
            command,
            file: Some(file),
            ..
        } => {
            let target =
                kknow_app::commands::file_target(file).map_err(|error| error.to_string())?;
            return run_command(command, &target);
        }
        _ => {}
    }

    let path = vault_path(&cli.vault)?;
    // Commands need the vault ID and name: the ID limits their scope (`vaults`
    // in the manifest), and the script sees the name.
    let (id, name) = vault_identity(&path);
    let mut workspace = Workspace::open(&path).map_err(|error| error.to_string())?;

    let mut code = ExitCode::SUCCESS;
    match cli.command {
        Command::List => list(&workspace),
        Command::Stats => stats(&workspace),
        Command::Links { note } => links(&workspace, &note)?,
        Command::Backlinks { note } => backlinks(&workspace, &note)?,
        Command::Tags => tags(&workspace),
        Command::Broken => broken(&workspace),
        Command::Orphans => orphans(&workspace),
        Command::Show { note } => show(&workspace, &note)?,
        Command::Render { note } => render(&workspace, &note)?,
        Command::New { name } => {
            let id = workspace.create(&name).map_err(|error| error.to_string())?;
            println!("created note {id}");
        }
        Command::Lock { note } => lock(&mut workspace, &note)?,
        Command::Unlock { note } => unlock(&mut workspace, &note)?,
        Command::Search { query, limit } => search(&workspace, &query.join(" "), limit)?,
        Command::Themes => themes(&workspace),
        Command::Bookmarks => bookmarks(&workspace),
        Command::Bookmark { note, line, name } => {
            bookmark(&mut workspace, &note, line, name.as_deref())?
        }
        Command::Tasks { all } => tasks(&workspace, all),
        Command::Proposals { action } => proposals(&mut workspace, action)?,
        Command::Commands {
            target,
            kind,
            example,
            ..
        } => {
            // With a target, show what applies. Without one, show the whole
            // file: otherwise commands for notes are not visible until you
            // name a note.
            let target = if target.is_some() || kind.is_some() {
                let target = kknow_app::commands::resolve(
                    &workspace,
                    &id,
                    &name,
                    kind.map(Into::into),
                    target.as_deref(),
                )
                .map_err(|error| error.to_string())?;
                Some(target)
            } else {
                None
            };
            commands(target, example)?
        }
        Command::Run {
            command,
            target,
            kind,
            ..
        } => {
            let target = kknow_app::commands::resolve(
                &workspace,
                &id,
                &name,
                kind.map(Into::into),
                target.as_deref(),
            )
            .map_err(|error| error.to_string())?;
            code = run_command(&command, &target)?
        }
        // Handled above: this command needs no vault.
        Command::Vaults { .. } => unreachable!(),
        Command::Trash { action } => trash(&mut workspace, action.unwrap_or(TrashAction::List))?,
        Command::Watch => watch(&mut workspace)?,
        // Handled above: these commands open their vaults themselves.
        Command::Mcp { .. } => unreachable!(),
        Command::Sync {
            dry_run,
            on_conflict,
            folder,
            s3,
            action,
        } => {
            let mut config = destination(&workspace, folder, s3)?;
            // The flag overrides the directory setting, but it is not set by default.
            if let Some(choice) = on_conflict {
                config.policy = choice.into();
            }
            match action {
                None => sync(&mut workspace, config, dry_run)?,
                Some(SyncAction::Encrypt) => encrypt(&workspace, config)?,
                Some(SyncAction::Passphrase) => change_passphrase(&config)?,
            }
        }
    }

    Ok(code)
}

/// The shared vault list and its path. The window reads the same file.
fn load_registry() -> Result<(VaultRegistry, PathBuf), String> {
    let dir = kknow_app::vaults::config_dir()
        .ok_or("cannot find the settings folder: HOME is not set")?;
    let loaded = kknow_app::vaults::open(&dir).map_err(|error| error.to_string())?;
    if let Some(problem) = loaded.problem {
        eprintln!("warning: {problem}");
    }
    Ok((loaded.registry, VaultRegistry::path(dir)))
}

/// The notes directory: a path if it exists, otherwise a name from the vault list.
///
/// The order matters: `--vault .` and `--vault ~/notes` must work even when
/// there is no vault list yet.
fn vault_path(wanted: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(wanted);
    if path.is_dir() {
        return Ok(path);
    }

    let (registry, _) = load_registry()?;
    registry
        .find(wanted)
        .map(|entry| entry.path.clone())
        .ok_or_else(|| format!("not a folder and not a vault from the list: {wanted}"))
}

/// ID and name of the open directory.
///
/// The directory can be a path that is not in the vault list. Then the name
/// comes from the folder, and the ID is the same name. Such a directory cannot
/// limit a command scope (`vaults` in the manifest), and this is correct: a
/// vault that is not in the list has no ID to refer to.
fn vault_identity(path: &Path) -> (String, String) {
    let name = path
        .canonicalize()
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let found = load_registry().ok().and_then(|(registry, _)| {
        let canonical = path.canonicalize().ok()?;
        registry
            .entries()
            .iter()
            .find(|entry| entry.path == canonical)
            .map(|entry| (entry.id.clone(), entry.name.clone()))
    });

    found.unwrap_or((name.clone(), name))
}

fn vaults(action: VaultAction) -> Result<(), String> {
    let (mut registry, path) = load_registry()?;

    match action {
        VaultAction::List => {}
        VaultAction::Add { path: folder, name } => {
            let added = registry
                .add(&folder, name)
                .map_err(|error| error.to_string())?;
            println!("added vault {} ({})", added.name, added.id);
            registry.save(&path).map_err(|error| error.to_string())?;
        }
        VaultAction::Rm { vault } => {
            let id = id_of(&registry, &vault)?;
            let removed = registry.remove(&id).map_err(|error| error.to_string())?;
            println!(
                "vault {} is removed from the list; the files stay in {}",
                removed.name,
                removed.path.display()
            );
            registry.save(&path).map_err(|error| error.to_string())?;
        }
        VaultAction::Use { vault } => {
            let id = id_of(&registry, &vault)?;
            registry
                .set_active(&id)
                .map_err(|error| error.to_string())?;
            registry.save(&path).map_err(|error| error.to_string())?;
        }
        VaultAction::Label { vault, labels } => {
            let id = id_of(&registry, &vault)?;
            registry
                .set_labels(&id, labels.iter().map(String::as_str))
                .map_err(|error| error.to_string())?;
            registry.save(&path).map_err(|error| error.to_string())?;
        }
    }

    show_vaults(&registry);
    Ok(())
}

/// `kknow mcp`: one vault from `--vault`, or all vaults with one of the labels.
///
/// The server takes the vaults when it starts. A vault that gets the label
/// later comes in when the agent starts the server again.
fn serve_mcp(vault: &str, labels: &[String]) -> Result<(), String> {
    let entries: Vec<(String, String, Vec<String>, PathBuf)> = if labels.is_empty() {
        let path = vault_path(vault)?;
        let (id, name) = vault_identity(&path);
        let labels = load_registry()
            .ok()
            .and_then(|(registry, _)| registry.get(&id).map(|entry| entry.labels.clone()))
            .unwrap_or_default();
        vec![(id, name, labels, path)]
    } else {
        let (registry, _) = load_registry()?;
        let found = registry.with_labels(labels.iter().map(String::as_str));
        if found.is_empty() {
            return Err(format!(
                "no vaults with label {}: kknow vaults label <vault> <label>",
                labels.join(", ")
            ));
        }
        found
            .into_iter()
            // An external disk can be disconnected: the rest still work.
            .filter(|entry| {
                let available = entry.available();
                if !available {
                    eprintln!("warning: the folder is not available: {}", entry.name);
                }
                available
            })
            .map(|entry| {
                (
                    entry.id.clone(),
                    entry.name.clone(),
                    entry.labels.clone(),
                    entry.path.clone(),
                )
            })
            .collect()
    };

    let mut served = Vec::new();
    for (id, name, labels, path) in entries {
        let workspace = Workspace::open(&path).map_err(|error| format!("{name}: {error}"))?;
        let watcher = workspace
            .watch()
            .inspect_err(|error| eprintln!("warning: {name}: {error}"))
            .ok();
        served.push(mcp::Served {
            vault: kknow_app::mcp::VaultRef { id, name, labels },
            workspace,
            watcher,
        });
    }
    let names: Vec<&str> = served
        .iter()
        .map(|vault| vault.vault.name.as_str())
        .collect();
    // stderr: stdout belongs to the protocol.
    eprintln!("kknow mcp: {}", names.join(", "));

    let mut vaults = mcp::Vaults::new(served);
    mcp::serve(
        &mut vaults,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
    .map_err(|error| error.to_string())
}

fn id_of(registry: &VaultRegistry, wanted: &str) -> Result<String, String> {
    registry
        .find(wanted)
        .map(|entry| entry.id.clone())
        .ok_or_else(|| format!("vault is not found: {wanted}"))
}

fn show_vaults(registry: &VaultRegistry) {
    if registry.is_empty() {
        println!("the vault list is empty: kknow vaults add <folder>");
        return;
    }

    let active = registry.active().map(|entry| entry.id.clone());
    for (index, entry) in registry.entries().iter().enumerate() {
        let mark = if active.as_deref() == Some(entry.id.as_str()) {
            "*"
        } else {
            " "
        };
        let hotkey = if index < kknow_app::vaults::HOTKEYS {
            format!("⌘{}", index + 1)
        } else {
            "  ".to_string()
        };
        // An unavailable directory stays in the list: an external disk can be disconnected.
        let state = if entry.available() {
            ""
        } else {
            "  (folder not available)"
        };

        let labels: String = entry
            .labels
            .iter()
            .map(|label| format!("  #{label}"))
            .collect();
        println!(
            "{mark} {hotkey} {:<20} {}{labels}{state}",
            entry.name,
            entry.path.display()
        );
    }
}

fn find<'a>(workspace: &'a Workspace, raw: &str) -> Result<&'a NoteId, String> {
    workspace
        .find(raw)
        .ok_or_else(|| format!("note is not found: {raw}"))
}

fn list(workspace: &Workspace) {
    for (id, parsed) in workspace.notes() {
        match parsed.title() {
            Some(title) if title != id.name() => println!("{id}  ({title})"),
            _ => println!("{id}"),
        }
    }
}

fn stats(workspace: &Workspace) {
    let graph = workspace.graph();
    let tag_count: usize = workspace.notes().map(|(_, parsed)| parsed.tags.len()).sum();
    let stats = workspace.stats();

    println!("vault:          {}", workspace.root().display());
    println!("notes:            {}", graph.note_count());
    println!("folders:          {}", stats.folders);
    println!("links:            {}", graph.edge_count());
    println!("broken links:     {}", graph.broken().len());
    println!("tags:             {tag_count}");
    println!("no backlinks:     {}", graph.orphans().count());
    println!("size:             {}", human_size(stats.bytes));

    let mut popular: Vec<(&NoteId, usize)> = workspace
        .notes()
        .map(|(id, _)| (id, graph.backlinks(id).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    popular.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    if !popular.is_empty() {
        println!("\nmost linked:");
        for (id, count) in popular.iter().take(5) {
            println!("  {count:>3}  {id}");
        }
    }
}

/// Vault size in human-readable form.
///
/// Nobody needs the exact byte count: people compare it with the file
/// manager, which shows "12.3 MB". The divisor is 1024, as in Linux and older
/// macOS: a kilobyte here is the memory kilobyte, not the marketing one.
fn human_size(bytes: u64) -> String {
    const STEP: f64 = 1024.0;
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];

    if (bytes as f64) < STEP {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64 / STEP;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < STEP {
            break;
        }
        value /= STEP;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

fn bookmarks(workspace: &Workspace) {
    let bookmarks = workspace.bookmarks();
    if bookmarks.is_empty() {
        println!("no bookmarks");
        return;
    }
    for bookmark in bookmarks {
        let place = format!("{}:{}", bookmark.note, bookmark.line);
        let text = bookmark.text.trim();
        match &bookmark.name {
            Some(name) => println!("{place}  «{name}»  {text}"),
            None => println!("{place}  {text}"),
        }
    }
}

fn tasks(workspace: &Workspace, all: bool) {
    let tasks: Vec<(&NoteId, &Task)> = workspace
        .tasks()
        .into_iter()
        .filter(|(_, task)| all || !task.done)
        .collect();
    if tasks.is_empty() {
        let empty = if all {
            "no tasks"
        } else {
            "no open tasks"
        };
        println!("{empty}");
        return;
    }
    for (id, task) in tasks {
        println!("{}", task_row(id, task, all));
    }
}

fn proposals(workspace: &mut Workspace, action: Option<ProposalAction>) -> Result<(), String> {
    let failure = |error: kknow_app::Error| error.to_string();
    match action.unwrap_or(ProposalAction::List { all: false }) {
        ProposalAction::List { all } => {
            let list: Vec<_> = workspace
                .proposals()
                .map_err(failure)?
                .into_iter()
                .filter(|proposal| all || proposal.status() == ProposalStatus::Pending)
                .collect();
            if list.is_empty() {
                println!("no proposals");
            }
            for proposal in list {
                println!("{}", proposal.describe());
            }
        }
        ProposalAction::Accept { id, only, feedback } => {
            let proposal = workspace.proposal(&id).map_err(failure)?;
            let waiting: Vec<usize> = proposal
                .edits
                .iter()
                .enumerate()
                .filter(|(_, edit)| edit.state == EditState::Pending)
                .map(|(index, _)| index)
                .collect();
            let (accept, reject): (Vec<usize>, Vec<usize>) = if only.is_empty() {
                (waiting, Vec::new())
            } else {
                waiting
                    .into_iter()
                    .partition(|index| only.contains(&(index + 1)))
            };
            let review = Review {
                accept,
                reject,
                feedback,
            };
            let (proposal, _) = workspace.review_proposal(&id, &review).map_err(failure)?;
            println!("{}", proposal.describe());
        }
        ProposalAction::Reject { id, feedback } => {
            let proposal = workspace.proposal(&id).map_err(failure)?;
            let review = Review {
                accept: Vec::new(),
                reject: (0..proposal.edits.len()).collect(),
                feedback,
            };
            let (proposal, _) = workspace.review_proposal(&id, &review).map_err(failure)?;
            println!("{}", proposal.describe());
        }
    }
    Ok(())
}

fn bookmark(
    workspace: &mut Workspace,
    note: &str,
    line: usize,
    name: Option<&str>,
) -> Result<(), String> {
    let id = find(workspace, note)?.clone();
    let lines = workspace
        .read(&id)
        .map_err(|error| error.to_string())?
        .lines()
        .count();
    if line == 0 || line > lines {
        return Err(format!("{id} has no line {line}: it has {lines} lines"));
    }
    if let Some(name) = name {
        return name_bookmark(workspace, &id, line, name);
    }
    let set = workspace
        .toggle_bookmark(&id, line)
        .map_err(|error| error.to_string())?;
    if set {
        println!("bookmark added: {id}:{line}");
    } else {
        println!("bookmark removed: {id}:{line}");
    }
    Ok(())
}

/// `--name`: a name for the bookmark on the line. The bookmark is set first if
/// there is none, so one command is enough.
fn name_bookmark(
    workspace: &mut Workspace,
    id: &NoteId,
    line: usize,
    name: &str,
) -> Result<(), String> {
    let error = |error: kknow_app::Error| error.to_string();
    if !workspace
        .name_bookmark(id, line, Some(name))
        .map_err(error)?
    {
        workspace.toggle_bookmark(id, line).map_err(error)?;
        workspace
            .name_bookmark(id, line, Some(name))
            .map_err(error)?;
    }
    match clean_name(Some(name)) {
        Some(name) => println!("bookmark {id}:{line} is named \"{name}\""),
        None => println!("bookmark {id}:{line} no longer has a name"),
    }
    Ok(())
}

fn links(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;

    let edges = workspace.outgoing(id);
    if edges.is_empty() {
        println!("{id}: no outgoing links");
        return Ok(());
    }

    println!("{id} links to:");
    for edge in edges {
        println!("  {}", describe(edge, &edge.to));
    }
    Ok(())
}

fn backlinks(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;

    let lines = workspace.backlink_lines(id);
    if lines.is_empty() {
        println!("{id}: no notes link here");
        return Ok(());
    }

    println!("notes that link to {id}:");
    for line in &lines {
        println!("  {}", backlink_row(line));
    }
    Ok(())
}

fn tags(workspace: &Workspace) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, parsed) in workspace.notes() {
        for tag in &parsed.tags {
            *counts.entry(tag.name.as_str()).or_default() += 1;
        }
    }

    let mut sorted: Vec<(&str, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    for (name, count) in sorted {
        println!("{count:>3}  #{name}");
    }
}

fn broken(workspace: &Workspace) {
    for link in workspace.graph().broken() {
        println!("{}  ->  {}", link.from, link.target);
    }
}

fn orphans(workspace: &Workspace) {
    for id in workspace.graph().orphans() {
        println!("{id}");
    }
}

fn show(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;
    let parsed = workspace
        .parsed(id)
        .expect("the note is parsed on open");

    println!("id:        {id}");
    println!("title: {}", parsed.title().unwrap_or("—"));
    println!("file:  {}", workspace.path_for(id).display());

    if parsed.frontmatter.is_some() {
        println!("frontmatter: yes");
    }

    if !parsed.headings.is_empty() {
        println!("\noutline:");
        for heading in &parsed.headings {
            let indent = "  ".repeat(heading.level.saturating_sub(1) as usize);
            println!("  {indent}{}", heading.text);
        }
    }

    if !parsed.wiki_links.is_empty() {
        println!("\nlinks:");
        for link in &parsed.wiki_links {
            println!("  [[{}]]", link.display_text());
        }
    }

    if !parsed.tags.is_empty() {
        println!("\ntags:");
        for tag in &parsed.tags {
            println!("  #{}", tag.name);
        }
    }

    Ok(())
}

fn render(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;
    let source = workspace.read(id).map_err(|error| error.to_string())?;

    println!("{}", workspace.render(id, &source));
    Ok(())
}

fn search(workspace: &Workspace, query: &str, limit: usize) -> Result<(), String> {
    let hits = workspace
        .search(query, limit)
        .map_err(|error| error.to_string())?;

    if hits.is_empty() {
        println!("nothing found");
        return Ok(());
    }

    for hit in hits {
        println!("{:.2}  {}  ({})", hit.score, hit.title, hit.id);
        let snippet = plain_snippet(&hit.snippet);
        if !snippet.is_empty() {
            println!("      {snippet}");
        }
    }
    Ok(())
}

/// Vault themes and global themes. The terminal cannot show them, but it tells
/// why a theme does not work: the core checks the values, not the window.
fn themes(workspace: &Workspace) {
    let global = kknow_app::vaults::config_dir().map(theme::global_dir);
    let set = workspace.themes(global.as_deref());

    if set.themes.is_empty() && set.problems.is_empty() {
        match &global {
            Some(global) => println!(
                "no themes: put a manifest in {} or in {}",
                theme::THEMES_DIR,
                global.display()
            ),
            None => println!("no themes: put a manifest in {}", theme::THEMES_DIR),
        }
        return;
    }

    for theme in &set.themes {
        let base = match theme.base {
            Base::Light => "light",
            Base::Dark => "dark",
        };
        let source = match theme.source {
            Source::Vault => "vault",
            Source::Global => "shared",
            Source::BuiltIn => "built-in",
        };
        println!(
            "{}  \"{}\" ({base}, {source}), variables: {}",
            theme.id,
            theme.name,
            theme.variables.len()
        );
        for warning in &theme.warnings {
            println!("     ⚠ {warning}");
        }
    }

    for problem in &set.problems {
        println!("not a theme: {problem}");
    }
}

/// The commands of this machine and their targets.
///
/// There is one file per app, next to the vault registry, not in the notes
/// directory: a command manifest is executable code, and it must not arrive
/// through sync. So it is edited by hand, like themes, not in a form in the
/// window.
fn commands(target: Option<Target>, example: bool) -> Result<(), String> {
    let path = kknow_app::commands::config_path()
        .ok_or("cannot find the settings folder: HOME is not set")?;

    if example {
        if path.exists() {
            return Err(format!("the file already exists: {}", path.display()));
        }
        kknow_run::spec::defaults()
            .save(&path)
            .map_err(|error| error.to_string())?;
        println!("default commands written: {}", path.display());
    }

    let set = CommandSet::load(&path).map_err(|error| error.to_string())?;

    let picked: Vec<&CommandSpec> = if let Some(target) = &target {
        let kind = if target.standalone() {
            "file outside a vault"
        } else {
            of(target.kind)
        };
        println!("target: {} ({kind})", target.path.display());
        set.for_target(target)
    } else {
        set.commands.iter().collect()
    };

    // "No commands" and "nothing applies to this target" are different cases.
    // In the second case the file exists, `--example` does not help, and the
    // user must check the command scope.
    if picked.is_empty() && set.problems.is_empty() {
        if set.is_empty() {
            println!("no commands: file {} is empty", path.display());
        } else {
            println!(
                "none of the {} commands in the file apply to this target",
                set.commands.len()
            );
            println!("`kknow commands` without a target shows which command applies where");
        }
        return Ok(());
    }

    for spec in picked {
        println!("{}  «{}» ({})", spec.id, spec.name, spec.kind);
        println!("     {}", scope(spec));
    }

    for problem in &set.problems {
        println!("not a command: {problem}");
    }
    Ok(())
}

/// Command scope as one line.
fn scope(spec: &CommandSpec) -> String {
    let kinds: Vec<&str> = spec.targets.iter().map(|kind| of(*kind)).collect();
    let mut parts = vec![format!("targets: {}", kinds.join(", "))];

    if !spec.vaults.is_empty() {
        parts.push(format!("vaults: {}", spec.vaults.join(", ")));
    }
    if !spec.patterns.is_empty() {
        parts.push(format!("paths: {}", spec.patterns.join(", ")));
    }
    if !spec.requires.is_empty() {
        parts.push(format!("requires fields: {}", spec.requires.join(", ")));
    }
    parts.join(" · ")
}

fn of(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Note => "note",
        TargetKind::Folder => "folder",
        TargetKind::Vault => "vault",
    }
}

/// Runs a command and returns its exit code.
///
/// The command output goes to the same streams as in the command:
/// `kknow run … | jq` must work. kknow writes its own messages to stderr so
/// they do not mix with the command output.
fn run_command(command: &str, target: &Target) -> Result<ExitCode, String> {
    let set = kknow_app::commands::load().map_err(|error| error.to_string())?;
    if set.get(command).is_none() {
        return Err(format!(
            "command is not found: {command}; `kknow commands` lists them"
        ));
    }

    // Run through the set, not directly: only the set knows the shared
    // environment of the file.
    let outcome = set
        .run(command, target)
        .map_err(|error| error.to_string())?;
    report(&outcome)
}

fn report(outcome: &Outcome) -> Result<ExitCode, String> {
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);

    if outcome.truncated {
        eprintln!("(output is cut: only the start is shown)");
    }
    if outcome.timed_out {
        // 124 is the code of `timeout(1)`: calling scripts already can tell
        // "timed out" from "failed".
        eprintln!("the command ran out of time and was stopped");
        return Ok(ExitCode::from(124));
    }

    match outcome.code {
        Some(0) => Ok(ExitCode::SUCCESS),
        // An exit code is a byte: `exit 256` in a shell also becomes zero, and
        // a negative code is not possible here.
        Some(code) => Ok(ExitCode::from(u8::try_from(code).unwrap_or(1))),
        None => Err("the command was stopped by a signal".to_string()),
    }
}

fn sync(workspace: &mut Workspace, mut config: RemoteConfig, dry_run: bool) -> Result<(), String> {
    println!("storage: {}", config.location());

    if let Provider::Git(settings) = &config.provider {
        return sync_git(workspace, settings, dry_run);
    }

    let secret = s3_secret(&config);

    // The terminal has no keychain, so it opens the key with the passphrase
    // on each run. It also finds an encrypted storage that the settings do
    // not mark: a storage connected with flags, or encrypted by the window
    // of another vault.
    let storage =
        kknow_sync::storage(&config, secret.clone()).map_err(|error| error.to_string())?;
    let encrypted = config.encrypted
        || kknow_sync::encrypted::is_encrypted(&*storage).map_err(|error| error.to_string())?;
    let key = if encrypted {
        let passphrase = passphrase("KKNOW_SYNC_PASSPHRASE", "Passphrase or recovery code: ")?;
        config.encrypted = true;
        Some(
            kknow_sync::encrypted::unlock(&*storage, &passphrase)
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    let engine = kknow_app::merge::sync_engine(&config, secret, key.as_ref())
        .map_err(|error| error.to_string())?;

    if dry_run {
        let plan = workspace
            .sync_plan(&engine)
            .map_err(|error| error.to_string())?;
        if plan.is_empty() {
            println!("everything is in sync");
        } else {
            print!("{plan}");
        }
        return Ok(());
    }

    let report = workspace.sync(&engine).map_err(|error| error.to_string())?;
    if report.is_empty() {
        println!("everything is in sync");
        return Ok(());
    }

    let counts = [
        ("uploaded", report.uploaded.len()),
        ("downloaded", report.downloaded.len()),
        ("moved to trash", report.removed_local.len()),
        ("deleted in the cloud", report.removed_remote.len()),
    ];
    for (label, count) in counts.iter().filter(|(_, count)| *count > 0) {
        println!("{label}: {count}");
    }

    for conflict in &report.conflicts {
        println!(
            "conflict: {}; the other version is saved as {}",
            conflict.id, conflict.copy
        );
    }

    Ok(())
}

/// The app keeps the secret key in the OS keychain, but the terminal cannot
/// get it from there: here it comes from an environment variable. A folder
/// needs no secret.
fn s3_secret(config: &RemoteConfig) -> Option<String> {
    match &config.provider {
        Provider::S3(_) => std::env::var("KKNOW_S3_SECRET_KEY").ok(),
        Provider::Folder { .. } | Provider::Git(_) => None,
    }
}

/// A secret from the variable, or from a prompt without an echo.
///
/// There is no flag on purpose, as for the S3 keys: `ps` shows the command
/// line, and the shell history keeps it.
fn passphrase(variable: &str, prompt: &str) -> Result<String, String> {
    secret(variable, prompt, "the storage is encrypted")
}

/// [`passphrase`] with the reason for the error when there is no terminal.
fn secret(variable: &str, prompt: &str, reason: &str) -> Result<String, String> {
    if let Ok(value) = std::env::var(variable) {
        return Ok(value);
    }
    if !std::io::stdin().is_terminal() {
        return Err(format!(
            "{reason}: set {variable} or run the command in a terminal"
        ));
    }
    rpassword::prompt_password(prompt).map_err(|error| error.to_string())
}

/// A new passphrase: from the variable, or typed two times.
fn new_passphrase(variable: &str) -> Result<String, String> {
    new_secret(variable, "passphrase", "the storage is encrypted")
}

/// A new secret: from the variable, or typed two times.
fn new_secret(variable: &str, noun: &str, reason: &str) -> Result<String, String> {
    if let Ok(value) = std::env::var(variable) {
        return Ok(value);
    }
    let first = secret(variable, &format!("New {noun}: "), reason)?;
    let second = secret(variable, &format!("The same {noun} again: "), reason)?;
    if first != second {
        return Err(format!("the two {noun}s are different"));
    }
    Ok(first)
}

/// The variable of the password of locked notes.
const LOCK_VARIABLE: &str = "KKNOW_LOCK_PASSPHRASE";

/// Opens the locked notes of the vault: with a new password for a vault
/// without one, else with the password or the recovery code.
fn open_lock(workspace: &mut Workspace) -> Result<(), String> {
    if workspace.lock_state().password {
        let typed = secret(
            LOCK_VARIABLE,
            "Vault password or recovery code: ",
            "the note needs the vault password",
        )?;
        workspace
            .unlock_notes(&typed)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    println!("The vault has no password for locked notes yet.");
    let password = new_secret(LOCK_VARIABLE, "password", "the vault needs a password")?;
    let code = workspace
        .set_lock_password(&password)
        .map_err(|error| error.to_string())?;
    println!();
    println!("Recovery code: {}", code.display().as_str());
    println!();
    println!("Write it down and keep it in a safe place. It opens the locked notes if");
    println!("you lose the password. kknow shows it only now. Without the password and");
    println!("the code, nobody can read the locked notes, and neither can kknow.");
    println!();
    Ok(())
}

/// `kknow lock`.
fn lock(workspace: &mut Workspace, raw: &str) -> Result<(), String> {
    let id = find(workspace, raw)?.clone();
    if workspace.is_locked(&id) {
        return Err(format!("the note {id} is already locked"));
    }
    open_lock(workspace)?;
    workspace
        .lock_note(&id)
        .map_err(|error| error.to_string())?;
    println!("locked: {id}");
    Ok(())
}

/// `kknow unlock`.
fn unlock(workspace: &mut Workspace, raw: &str) -> Result<(), String> {
    let id = find(workspace, raw)?.clone();
    if !workspace.is_locked(&id) {
        return Err(format!("the note {id} is not locked"));
    }
    open_lock(workspace)?;
    workspace
        .remove_lock(&id)
        .map_err(|error| error.to_string())?;
    println!("unlocked: {id}");
    Ok(())
}

/// `kknow sync encrypt`.
fn encrypt(workspace: &Workspace, mut config: RemoteConfig) -> Result<(), String> {
    println!("storage: {}", config.location());
    let storage =
        kknow_sync::storage(&config, s3_secret(&config)).map_err(|error| error.to_string())?;
    let passphrase = new_passphrase("KKNOW_SYNC_PASSPHRASE")?;
    let (_, code) =
        kknow_sync::encrypted::init(&*storage, &passphrase).map_err(|error| error.to_string())?;

    // Mark the directory settings, if they are for this storage. A storage
    // from flags has no settings: the next sync finds the key file anyway.
    let path = RemoteConfig::path(workspace.root());
    let saved = RemoteConfig::load(&path).map_err(|error| error.to_string())?;
    if saved.is_some_and(|saved| saved.location() == config.location()) {
        config.encrypted = true;
        config.save(&path).map_err(|error| error.to_string())?;
    }

    println!("The storage is encrypted.");
    println!();
    println!("Recovery code: {}", code.display().as_str());
    println!();
    println!("Write it down and keep it in a safe place. It opens the storage if you");
    println!("lose the passphrase. kknow shows it only now. Without the passphrase and");
    println!("the code, nobody can read the notes, and neither can kknow.");
    Ok(())
}

/// `kknow sync passphrase`.
fn change_passphrase(config: &RemoteConfig) -> Result<(), String> {
    println!("storage: {}", config.location());
    let storage =
        kknow_sync::storage(config, s3_secret(config)).map_err(|error| error.to_string())?;
    let current = passphrase(
        "KKNOW_SYNC_PASSPHRASE",
        "Current passphrase or recovery code: ",
    )?;
    let new = new_passphrase("KKNOW_SYNC_NEW_PASSPHRASE")?;
    kknow_sync::encrypted::change_passphrase(&*storage, &current, &new)
        .map_err(|error| error.to_string())?;
    println!("The passphrase is changed. Other devices keep working.");
    Ok(())
}

/// A git sync from the terminal: fetch, merge, push.
///
/// The program is `KKNOW_GIT` or `git` from `PATH`: the terminal has the
/// `PATH` of the user, so there is nothing to ask. The keychain token of the
/// window is not reachable here, so the "token" way reads `KKNOW_GIT_TOKEN`.
/// Commits stay a decision of the person: this command does not commit.
fn sync_git(
    workspace: &mut Workspace,
    settings: &GitSettings,
    dry_run: bool,
) -> Result<(), String> {
    let program = std::env::var("KKNOW_GIT").unwrap_or_else(|_| "git".to_string());
    let stored = std::env::var("KKNOW_GIT_TOKEN").ok();
    let token = kknow_app::git_token(settings, stored);
    if settings.auth != GitAuth::System && token.is_none() {
        return Err(match settings.auth {
            GitAuth::Env => format!(
                "variable {} is not set",
                settings.env.as_deref().unwrap_or("with the token")
            ),
            _ => "no token is set: KKNOW_GIT_TOKEN".to_string(),
        });
    }
    let git = Git::new(program, workspace.root())
        .with_token(token)
        .with_merge(kknow_app::merge::merge_shared);
    let failure = |error: kknow_sync::Error| error.to_string();

    if dry_run {
        let status = git.status().map_err(failure)?;
        if let Some(branch) = &status.branch {
            println!("branch: {branch}");
        }
        println!("commits not pushed: {}", status.ahead);
        if status.changes.is_empty() {
            println!("no uncommitted changes");
        }
        for change in &status.changes {
            let place = if change.staged.is_some() {
                "staged"
            } else {
                "not staged"
            };
            println!("{place}: {}", change.path);
        }
        return Ok(());
    }

    let report = workspace
        .git_sync(&git)
        .map_err(|error| error.to_string())?;
    if report.merged.changed.is_empty() && report.pushed == 0 {
        println!("everything is in sync");
    }
    if !report.merged.changed.is_empty() {
        println!("files changed: {}", report.merged.changed.len());
    }
    if report.pushed > 0 {
        println!("commits pushed: {}", report.pushed);
    }
    for conflict in &report.merged.conflicts {
        println!(
            "conflict: {}; the other version is saved as {}",
            conflict.path, conflict.copy
        );
    }
    Ok(())
}

fn trash(workspace: &mut Workspace, action: TrashAction) -> Result<(), String> {
    match action {
        TrashAction::List => {
            let entries = workspace
                .trash()
                .entries()
                .map_err(|error| error.to_string())?;
            if entries.is_empty() {
                println!("the trash is empty");
            }
            let now = SystemTime::now();
            for entry in entries {
                let age = now.duration_since(entry.deleted).unwrap_or_default();
                println!(
                    "{}  {}  ({}, notes: {})",
                    entry.id,
                    entry.path,
                    days_ago(age),
                    entry.notes.len()
                );
            }
        }
        TrashAction::Restore { entry } => {
            let restored = workspace
                .restore(&entry)
                .map_err(|error| error.to_string())?;
            for id in &restored {
                println!("restored {id}");
            }
        }
        TrashAction::Empty => {
            let count = workspace
                .trash()
                .empty()
                .map_err(|error| error.to_string())?;
            println!("deleted from the trash forever: {count}");
        }
    }
    Ok(())
}

/// Age of a trash entry in whole days: the trash keeps entries for 30 days,
/// so hours do not matter.
fn days_ago(age: Duration) -> String {
    match age.as_secs() / (24 * 60 * 60) {
        0 => "today".to_string(),
        days => format!("{days} d ago"),
    }
}

fn watch(workspace: &mut Workspace) -> Result<(), String> {
    let watcher = workspace.watch().map_err(|error| error.to_string())?;
    println!(
        "watching {}; press Ctrl+C to stop",
        workspace.root().display()
    );

    while let Some(events) = watcher.recv() {
        let changes = workspace
            .apply(&events)
            .map_err(|error| error.to_string())?;
        for id in &changes.updated {
            println!("changed  {id}");
        }
        for id in &changes.removed {
            println!("deleted  {id}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_written_the_way_a_file_manager_writes_them() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(12_900_000), "12.3 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn trash_age_is_in_whole_days() {
        assert_eq!(days_ago(Duration::from_secs(60)), "today");
        assert_eq!(
            days_ago(Duration::from_secs(3 * 24 * 60 * 60 + 5)),
            "3 d ago"
        );
    }
}
