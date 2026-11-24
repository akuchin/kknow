use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use kknow_run::TargetKind;
use kknow_sync::ConflictPolicy;

#[derive(Parser)]
#[command(
    name = "kknow",
    version,
    about = "Markdown knowledge base: links, tags, structure"
)]
pub(crate) struct Cli {
    #[arg(help = "Notes directory or vault name from the list (`kknow vaults`)")]
    #[arg(long, short, global = true, env = "KKNOW_VAULT", default_value = ".")]
    pub(crate) vault: String,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(about = "List notes")]
    List,
    #[command(about = "Show vault statistics")]
    Stats,
    #[command(about = "Show outgoing links of a note")]
    Links { note: String },
    #[command(about = "Show notes that link to a note")]
    Backlinks { note: String },
    #[command(about = "List tags and their counts")]
    Tags,
    #[command(about = "List links to notes that do not exist")]
    Broken,
    #[command(about = "List notes with no incoming links")]
    Orphans,
    #[command(about = "Show the parsed structure of a note")]
    Show { note: String },
    #[command(about = "Render a note to HTML")]
    Render { note: String },
    #[command(about = "Create an empty note")]
    New { name: String },
    #[command(
        about = "Lock a note with the vault password: the file becomes ciphertext",
        long_about = "Lock a note with the vault password: the file becomes ciphertext.\n\nThe password comes from `KKNOW_LOCK_PASSPHRASE` or a prompt. A vault without a password gets one, and kknow prints the recovery code once."
    )]
    Lock { note: String },
    #[command(
        about = "Remove the lock of a note: the file becomes plain text again",
        long_about = "Remove the lock of a note: the file becomes plain text again.\n\nThe password or the recovery code comes from `KKNOW_LOCK_PASSPHRASE` or a prompt."
    )]
    Unlock { note: String },
    #[command(about = "Full-text search: `kknow search \"tag:rust notes\"`")]
    Search {
        query: Vec<String>,
        #[arg(help = "Number of results to show")]
        #[arg(long, short, default_value_t = 10)]
        limit: usize,
    },
    #[command(about = "List themes found in the directory and their problems")]
    Themes,
    #[command(about = "List vault bookmarks: `note:line  «name»  line text`")]
    Bookmarks,
    #[command(
        about = "Add or remove a bookmark on a note line: `kknow bookmark plan 12`",
        long_about = "Add or remove a bookmark on a note line: `kknow bookmark plan 12`.\n\nWith `--name`, names the bookmark instead and sets it if needed: `kknow bookmark plan 12 --name \"summary\"`. An empty name removes the name."
    )]
    Bookmark {
        note: String,
        #[arg(help = "Line number, starting from 1")]
        line: usize,
        #[arg(help = "Bookmark name")]
        #[arg(long)]
        name: Option<String>,
    },
    #[command(about = "List open tasks (`- [ ]`) of all notes: `note:line  text`")]
    Tasks {
        #[arg(help = "Also list done tasks, with `[x]` and `[ ]` before the text")]
        #[arg(long)]
        all: bool,
    },
    #[command(about = "Edits that agents proposed through MCP and that wait for review")]
    Proposals {
        #[command(subcommand)]
        action: Option<ProposalAction>,
    },
    #[command(about = "Vaults that kknow knows: the same list as in the window")]
    Vaults {
        #[command(subcommand)]
        action: Option<VaultAction>,
    },
    #[command(
        about = "Commands attached to notes, folders, and vaults",
        long_about = "Commands attached to notes, folders, and vaults.\n\nWithout a target, shows the whole file with the scope of each command. With a target, shows only the commands for it."
    )]
    Commands {
        #[arg(help = "Note or folder. Without it, the target is the whole vault")]
        target: Option<String>,
        #[arg(help = "Target kind, if the name matches both a note and a folder")]
        #[arg(long, value_enum)]
        kind: Option<TargetChoice>,
        #[arg(help = "A markdown file outside any vault, as a note target")]
        #[arg(long, conflicts_with_all = ["target", "kind"])]
        file: Option<PathBuf>,
        #[arg(help = "Write the shipped commands to the file if it does not exist yet")]
        #[arg(long)]
        example: bool,
    },
    #[command(
        about = "Run a command on a note, folder, or vault",
        long_about = "Run a command on a note, folder, or vault.\n\nThe script gets the target path in `KKNOW_PATH`, and also `KKNOW_NOTE`, `KKNOW_RELATIVE`, `KKNOW_VAULT`, and others (`kknow commands`). The exit code of the command becomes the exit code of kknow."
    )]
    Run {
        #[arg(help = "Command ID from `kknow commands`")]
        command: String,
        #[arg(help = "Note or folder. Without it, the target is the whole vault")]
        target: Option<String>,
        #[arg(help = "Target kind, if the name matches both a note and a folder")]
        #[arg(long, value_enum)]
        kind: Option<TargetChoice>,
        #[arg(help = "A markdown file outside any vault, as a note target")]
        #[arg(long, conflicts_with_all = ["target", "kind"])]
        file: Option<PathBuf>,
    },
    #[command(
        about = "Deleted notes and folders: list, restore, empty",
        long_about = "Deleted notes and folders: list, restore, empty.\n\nEntries older than 30 days go away on the next delete."
    )]
    Trash {
        #[command(subcommand)]
        action: Option<TrashAction>,
    },
    #[command(about = "Watch the directory and print changes")]
    Watch,
    #[command(
        about = "Serve vaults to an agent over MCP on stdin and stdout",
        long_about = "Serve vaults to an agent over MCP on stdin and stdout.\n\nThe agent starts the process itself, for example `claude mcp add kknow -- kknow mcp --vault notes`. With `--label`, the server takes all vaults with the label instead of `--vault`: `kknow mcp --label work`."
    )]
    Mcp {
        #[arg(
            help = "Vault label from `kknow vaults`. Repeat it for several labels: a vault needs one of them"
        )]
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    #[command(
        about = "Sync the vault with S3-compatible storage, a shared folder or a git repository"
    )]
    Sync {
        #[arg(help = "Show the plan and change nothing")]
        #[arg(long)]
        dry_run: bool,
        #[arg(
            help = "Which version wins if both sides changed a note. The default comes from the directory settings"
        )]
        #[arg(long, value_enum)]
        on_conflict: Option<OnConflict>,
        #[arg(help = "Sync through a shared folder, for example a cloud drive folder")]
        #[arg(long, env = "KKNOW_FOLDER", conflicts_with = "bucket")]
        folder: Option<PathBuf>,
        #[command(flatten)]
        s3: S3Args,
        #[command(subcommand)]
        action: Option<SyncAction>,
    },
}

#[derive(Subcommand)]
pub(crate) enum SyncAction {
    #[command(
        about = "Turn on encryption: a new key, then all notes in the storage are encrypted. Prints the recovery code one time"
    )]
    Encrypt,
    #[command(
        about = "Set a new passphrase. The notes and the recovery code stay as they are. The new passphrase comes from `KKNOW_SYNC_NEW_PASSPHRASE` or a prompt"
    )]
    Passphrase,
}

#[derive(Subcommand)]
pub(crate) enum VaultAction {
    #[command(about = "List vaults. The same as `kknow vaults` without a subcommand")]
    List,
    #[command(about = "Add a directory to the list")]
    Add {
        path: PathBuf,
        #[arg(help = "Vault name. The default is the directory name")]
        #[arg(long)]
        name: Option<String>,
    },
    #[command(about = "Remove a vault from the list. The files on disk stay")]
    Rm { vault: String },
    #[command(about = "Make a vault active: the window opens with it")]
    Use { vault: String },
    #[command(
        about = "Set the labels of a vault: `kknow vaults label Personal work ai`",
        long_about = "Set the labels of a vault: `kknow vaults label Personal work ai`.\n\nThe list replaces the old one; without labels, the vault has none. `kknow mcp --label work` serves all vaults with the label."
    )]
    Label { vault: String, labels: Vec<String> },
}

#[derive(Subcommand)]
pub(crate) enum ProposalAction {
    #[command(about = "List proposals. The same as `kknow proposals` without a subcommand")]
    List {
        #[arg(help = "Also list reviewed proposals")]
        #[arg(long)]
        all: bool,
    },
    #[command(about = "Apply the edits of a proposal to the note")]
    Accept {
        #[arg(help = "Proposal ID from `kknow proposals`")]
        id: String,
        #[arg(
            help = "Only these edits, by number: `--only 1,3`. The other waiting edits are rejected"
        )]
        #[arg(long, value_delimiter = ',')]
        only: Vec<usize>,
        #[arg(help = "A note for the agent")]
        #[arg(long)]
        feedback: Option<String>,
    },
    #[command(about = "Reject all waiting edits of a proposal")]
    Reject {
        id: String,
        #[arg(help = "Why: the agent reads it and tries again")]
        #[arg(long)]
        feedback: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum TrashAction {
    #[command(about = "List entries. The same as `kknow trash` without a subcommand")]
    List,
    #[command(about = "Put an entry back. Overwrites no note")]
    Restore {
        #[arg(help = "Entry ID from `kknow trash`")]
        entry: String,
    },
    #[command(about = "Remove all entries for good")]
    Empty,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum TargetChoice {
    #[value(help = "A note: `KKNOW_PATH` is its file")]
    Note,
    #[value(help = "A folder inside the notes directory")]
    Folder,
    #[value(help = "The whole notes directory")]
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
pub(crate) enum OnConflict {
    #[value(help = "Keep both versions: the remote one goes next to it as a copy")]
    Both,
    #[value(help = "Keep the version on this device")]
    Local,
    #[value(help = "Keep the version from the storage")]
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

#[derive(Args)]
pub(crate) struct S3Args {
    #[arg(help = "S3 bucket")]
    #[arg(long, env = "KKNOW_S3_BUCKET")]
    pub(crate) bucket: Option<String>,
    #[arg(help = "Storage address. For MinIO, this is http://localhost:9000")]
    #[arg(long, env = "KKNOW_S3_ENDPOINT")]
    pub(crate) endpoint: Option<String>,
    #[arg(help = "Region. The default is us-east-1")]
    #[arg(long, env = "KKNOW_S3_REGION")]
    pub(crate) region: Option<String>,
    #[arg(help = "Prefix inside the bucket, so one bucket can hold several vaults")]
    #[arg(long, env = "KKNOW_S3_PREFIX")]
    pub(crate) prefix: Option<String>,
}
