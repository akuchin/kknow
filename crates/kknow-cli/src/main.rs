//! A thin wrapper over the core: open a vault, parse notes, show links.
//!
//! The CLI exists so that the core can grow and be tried without the GUI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use clap::{Args, Parser, Subcommand, ValueEnum};
use kknow_app::Workspace;
use kknow_app::bookmarks::clean_name;
use kknow_app::theme::{self, Base, Source};
use kknow_app::vaults::VaultRegistry;
use kknow_core::NoteId;
use kknow_index::{Edge, EdgeKind, Subpath, Task};
use kknow_run::{CommandSet, CommandSpec, Outcome, Target, TargetKind};
use kknow_sync::{ConflictPolicy, Provider, RemoteConfig, S3Settings};

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
    /// `kknow bookmark plan 12 --name "итоги"`. An empty name removes the name.
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
        /// Write an example file if it does not exist yet.
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
    /// Sync the vault with S3-compatible storage or a shared folder.
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
    },
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
        .ok_or_else(|| "не задано хранилище: --bucket, --folder или настройка каталога".to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ошибка: {error}");
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
            println!("создана заметка {id}");
        }
        Command::Search { query, limit } => search(&workspace, &query.join(" "), limit)?,
        Command::Themes => themes(&workspace),
        Command::Bookmarks => bookmarks(&workspace),
        Command::Bookmark { note, line, name } => {
            bookmark(&mut workspace, &note, line, name.as_deref())?
        }
        Command::Tasks { all } => tasks(&workspace, all),
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
        Command::Sync {
            dry_run,
            on_conflict,
            folder,
            s3,
        } => {
            let mut config = destination(&workspace, folder, s3)?;
            // The flag overrides the directory setting, but it is not set by default.
            if let Some(choice) = on_conflict {
                config.policy = choice.into();
            }
            sync(&mut workspace, config, dry_run)?
        }
    }

    Ok(code)
}

/// The shared vault list and its path. The window reads the same file.
fn load_registry() -> Result<(VaultRegistry, PathBuf), String> {
    let dir = kknow_app::vaults::config_dir()
        .ok_or("не найден каталог настроек: в окружении нет HOME")?;
    let loaded = kknow_app::vaults::open(&dir).map_err(|error| error.to_string())?;
    if let Some(problem) = loaded.problem {
        eprintln!("предупреждение: {problem}");
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
        .ok_or_else(|| format!("не каталог и не хранилище из списка: {wanted}"))
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
            println!("добавлено хранилище {} ({})", added.name, added.id);
            registry.save(&path).map_err(|error| error.to_string())?;
        }
        VaultAction::Rm { vault } => {
            let id = id_of(&registry, &vault)?;
            let removed = registry.remove(&id).map_err(|error| error.to_string())?;
            println!(
                "хранилище {} забыто; файлы остались в {}",
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
    }

    show_vaults(&registry);
    Ok(())
}

fn id_of(registry: &VaultRegistry, wanted: &str) -> Result<String, String> {
    registry
        .find(wanted)
        .map(|entry| entry.id.clone())
        .ok_or_else(|| format!("хранилище не найдено: {wanted}"))
}

fn show_vaults(registry: &VaultRegistry) {
    if registry.is_empty() {
        println!("список хранилищ пуст: kknow vaults add <каталог>");
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
            "  (каталог недоступен)"
        };

        println!(
            "{mark} {hotkey} {:<20} {}{state}",
            entry.name,
            entry.path.display()
        );
    }
}

fn find<'a>(workspace: &'a Workspace, raw: &str) -> Result<&'a NoteId, String> {
    workspace
        .find(raw)
        .ok_or_else(|| format!("заметка не найдена: {raw}"))
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
    println!("заметок:        {}", graph.note_count());
    println!("папок:          {}", stats.folders);
    println!("ссылок:         {}", graph.edge_count());
    println!("битых ссылок:   {}", graph.broken().len());
    println!("тегов:          {tag_count}");
    println!("без бэклинков:  {}", graph.orphans().count());
    println!("размер:         {}", human_size(stats.bytes));

    let mut popular: Vec<(&NoteId, usize)> = workspace
        .notes()
        .map(|(id, _)| (id, graph.backlinks(id).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    popular.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    if !popular.is_empty() {
        println!("\nчаще всего ссылаются:");
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
    const UNITS: [&str; 3] = ["КБ", "МБ", "ГБ"];

    if (bytes as f64) < STEP {
        return format!("{bytes} Б");
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
        println!("закладок нет");
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
            "задач нет"
        } else {
            "открытых задач нет"
        };
        println!("{empty}");
        return;
    }
    for (id, task) in tasks {
        println!("{}", task_row(id, task, all));
    }
}

/// `note:line  text`. With `--all`, the box shows which tasks are done.
fn task_row(id: &NoteId, task: &Task, all: bool) -> String {
    let place = format!("{id}:{}", task.line);
    if !all {
        return format!("{place}  {}", task.text);
    }
    let mark = if task.done { 'x' } else { ' ' };
    format!("{place}  [{mark}] {}", task.text)
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
        return Err(format!("в {id} нет строки {line}: строк всего {lines}"));
    }
    if let Some(name) = name {
        return name_bookmark(workspace, &id, line, name);
    }
    let set = workspace
        .toggle_bookmark(&id, line)
        .map_err(|error| error.to_string())?;
    if set {
        println!("закладка поставлена: {id}:{line}");
    } else {
        println!("закладка снята: {id}:{line}");
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
        Some(name) => println!("закладка {id}:{line} названа «{name}»"),
        None => println!("у закладки {id}:{line} больше нет имени"),
    }
    Ok(())
}

fn links(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;

    let edges = workspace.outgoing(id);
    if edges.is_empty() {
        println!("{id}: исходящих ссылок нет");
        return Ok(());
    }

    println!("{id} ссылается на:");
    for edge in edges {
        println!("  {}", describe(edge, &edge.to));
    }
    Ok(())
}

fn backlinks(workspace: &Workspace, note: &str) -> Result<(), String> {
    let id = find(workspace, note)?;

    let edges = workspace.backlinks(id);
    if edges.is_empty() {
        println!("{id}: на эту заметку никто не ссылается");
        return Ok(());
    }

    println!("на {id} ссылаются:");
    for edge in edges {
        println!("  {}", describe(edge, &edge.from));
    }
    Ok(())
}

fn describe(edge: &Edge, other: &NoteId) -> String {
    let mut line = other.to_string();

    match &edge.subpath {
        Some(Subpath::Heading(heading)) => line.push_str(&format!("#{heading}")),
        Some(Subpath::Block(block)) => line.push_str(&format!("#^{block}")),
        None => {}
    }
    if let Some(label) = &edge.label {
        line.push_str(&format!("  «{label}»"));
    }
    match edge.kind {
        EdgeKind::Embed => line.push_str("  [embed]"),
        EdgeKind::Markdown => line.push_str("  [md]"),
        EdgeKind::Wiki => {}
    }

    line
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
        .expect("заметка разобрана при открытии");

    println!("id:        {id}");
    println!("заголовок: {}", parsed.title().unwrap_or("—"));
    println!("файл:      {}", workspace.path_for(id).display());

    if parsed.frontmatter.is_some() {
        println!("frontmatter: есть");
    }

    if !parsed.headings.is_empty() {
        println!("\nструктура:");
        for heading in &parsed.headings {
            let indent = "  ".repeat(heading.level.saturating_sub(1) as usize);
            println!("  {indent}{}", heading.text);
        }
    }

    if !parsed.wiki_links.is_empty() {
        println!("\nссылки:");
        for link in &parsed.wiki_links {
            println!("  [[{}]]", link.display_text());
        }
    }

    if !parsed.tags.is_empty() {
        println!("\nтеги:");
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
        println!("ничего не найдено");
        return Ok(());
    }

    for hit in hits {
        println!("{:.2}  {}  ({})", hit.score, hit.title, hit.id);
        // The snippet has HTML highlighting, which the terminal does not need.
        let snippet = hit.snippet.replace("<b>", "«").replace("</b>", "»");
        if !snippet.is_empty() {
            println!("      {}", snippet.replace('\n', " "));
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
                "тем нет: положите манифест в {} или в {}",
                theme::THEMES_DIR,
                global.display()
            ),
            None => println!("тем нет: положите манифест в {}", theme::THEMES_DIR),
        }
        return;
    }

    for theme in &set.themes {
        let base = match theme.base {
            Base::Light => "светлая",
            Base::Dark => "тёмная",
        };
        let source = match theme.source {
            Source::Vault => "хранилище",
            Source::Global => "общая",
            Source::BuiltIn => "встроенная",
        };
        println!(
            "{}  «{}» ({base}, {source}), переменных: {}",
            theme.id,
            theme.name,
            theme.variables.len()
        );
        for warning in &theme.warnings {
            println!("     ⚠ {warning}");
        }
    }

    for problem in &set.problems {
        println!("не тема: {problem}");
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
        .ok_or("не найден каталог настроек: в окружении нет HOME")?;

    if example {
        if path.exists() {
            return Err(format!("файл уже есть: {}", path.display()));
        }
        kknow_run::spec::example()
            .save(&path)
            .map_err(|error| error.to_string())?;
        println!("заготовка записана: {}", path.display());
    }

    let set = CommandSet::load(&path).map_err(|error| error.to_string())?;

    let picked: Vec<&CommandSpec> = if let Some(target) = &target {
        let kind = if target.standalone() {
            "файл вне хранилища"
        } else {
            of(target.kind)
        };
        println!("цель: {} ({kind})", target.path.display());
        set.for_target(target)
    } else {
        set.commands.iter().collect()
    };

    // "No commands" and "nothing applies to this target" are different cases.
    // In the second case the file exists, `--example` does not help, and the
    // user must check the command scope.
    if picked.is_empty() && set.problems.is_empty() {
        if set.is_empty() {
            println!("команд нет: положите манифест в {}", path.display());
            println!("заготовку запишет `kknow commands --example`");
        } else {
            println!(
                "к этой цели не относится ни одна из {} команд файла",
                set.commands.len()
            );
            println!("что и куда навешено, покажет `kknow commands` без цели");
        }
        return Ok(());
    }

    for spec in picked {
        println!("{}  «{}» ({})", spec.id, spec.name, spec.kind);
        println!("     {}", scope(spec));
    }

    for problem in &set.problems {
        println!("не команда: {problem}");
    }
    Ok(())
}

/// Command scope as one line.
fn scope(spec: &CommandSpec) -> String {
    let kinds: Vec<&str> = spec.targets.iter().map(|kind| of(*kind)).collect();
    let mut parts = vec![format!("цели: {}", kinds.join(", "))];

    if !spec.vaults.is_empty() {
        parts.push(format!("хранилища: {}", spec.vaults.join(", ")));
    }
    if !spec.patterns.is_empty() {
        parts.push(format!("пути: {}", spec.patterns.join(", ")));
    }
    if !spec.requires.is_empty() {
        parts.push(format!("требует полей: {}", spec.requires.join(", ")));
    }
    parts.join(" · ")
}

fn of(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Note => "заметка",
        TargetKind::Folder => "папка",
        TargetKind::Vault => "хранилище",
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
            "команда не найдена: {command}; список — `kknow commands`"
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
        eprintln!("(вывод обрезан: показано начало)");
    }
    if outcome.timed_out {
        // 124 is the code of `timeout(1)`: calling scripts already can tell
        // "timed out" from "failed".
        eprintln!("команда не уложилась в отпущенное время и была прервана");
        return Ok(ExitCode::from(124));
    }

    match outcome.code {
        Some(0) => Ok(ExitCode::SUCCESS),
        // An exit code is a byte: `exit 256` in a shell also becomes zero, and
        // a negative code is not possible here.
        Some(code) => Ok(ExitCode::from(u8::try_from(code).unwrap_or(1))),
        None => Err("команда прервана сигналом".to_string()),
    }
}

fn sync(workspace: &mut Workspace, config: RemoteConfig, dry_run: bool) -> Result<(), String> {
    println!("хранилище: {}", config.location());

    // The app keeps the secret key in the OS keychain, but the terminal cannot
    // get it from there: here it comes from an environment variable. A folder
    // needs no secret.
    let secret = match &config.provider {
        Provider::S3(_) => std::env::var("KKNOW_S3_SECRET_KEY").ok(),
        Provider::Folder { .. } => None,
    };

    let engine = kknow_sync::engine(&config, secret).map_err(|error| error.to_string())?;

    if dry_run {
        let plan = workspace
            .sync_plan(&engine)
            .map_err(|error| error.to_string())?;
        if plan.is_empty() {
            println!("всё синхронизировано");
        } else {
            print!("{plan}");
        }
        return Ok(());
    }

    let report = workspace.sync(&engine).map_err(|error| error.to_string())?;
    if report.is_empty() {
        println!("всё синхронизировано");
        return Ok(());
    }

    let counts = [
        ("выгружено", report.uploaded.len()),
        ("загружено", report.downloaded.len()),
        ("перемещено в корзину", report.removed_local.len()),
        ("удалено в облаке", report.removed_remote.len()),
    ];
    for (label, count) in counts.iter().filter(|(_, count)| *count > 0) {
        println!("{label}: {count}");
    }

    for conflict in &report.conflicts {
        println!(
            "конфликт: {} — чужая версия сохранена как {}",
            conflict.id, conflict.copy
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
                println!("корзина пуста");
            }
            let now = SystemTime::now();
            for entry in entries {
                let age = now.duration_since(entry.deleted).unwrap_or_default();
                println!(
                    "{}  {}  ({}, заметок: {})",
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
                println!("восстановлена {id}");
            }
        }
        TrashAction::Empty => {
            let count = workspace
                .trash()
                .empty()
                .map_err(|error| error.to_string())?;
            println!("удалено из корзины навсегда: {count}");
        }
    }
    Ok(())
}

/// Age of a trash entry in whole days: the trash keeps entries for 30 days,
/// so hours do not matter.
fn days_ago(age: Duration) -> String {
    match age.as_secs() / (24 * 60 * 60) {
        0 => "сегодня".to_string(),
        days => format!("{days} дн. назад"),
    }
}

fn watch(workspace: &mut Workspace) -> Result<(), String> {
    let watcher = workspace.watch().map_err(|error| error.to_string())?;
    println!(
        "слежу за {} — Ctrl+C чтобы выйти",
        workspace.root().display()
    );

    while let Some(events) = watcher.recv() {
        let changes = workspace
            .apply(&events)
            .map_err(|error| error.to_string())?;
        for id in &changes.updated {
            println!("изменена  {id}");
        }
        for id in &changes.removed {
            println!("удалена   {id}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_written_the_way_a_file_manager_writes_them() {
        assert_eq!(human_size(0), "0 Б");
        assert_eq!(human_size(512), "512 Б");
        assert_eq!(human_size(1024), "1.0 КБ");
        assert_eq!(human_size(1536), "1.5 КБ");
        assert_eq!(human_size(12_900_000), "12.3 МБ");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 ГБ");
    }

    #[test]
    fn a_task_row_shows_the_box_only_with_all() {
        let id = NoteId::parse("Проекты/План").unwrap();
        let note = kknow_index::parse("# План\n\n- [x] начать\n");
        let task = &note.tasks[0];

        assert_eq!(task_row(&id, task, false), "Проекты/План:3  начать");
        assert_eq!(task_row(&id, task, true), "Проекты/План:3  [x] начать");
    }

    #[test]
    fn trash_age_is_in_whole_days() {
        assert_eq!(days_ago(Duration::from_secs(60)), "сегодня");
        assert_eq!(
            days_ago(Duration::from_secs(3 * 24 * 60 * 60 + 5)),
            "3 дн. назад"
        );
    }
}
