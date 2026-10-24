// Generated from the Rust types by `src-tauri/src/bindings.rs`. Do not edit:
// run `KKNOW_BLESS=1 cargo test -p kknow-desktop bindings` instead.

export type AppSettings = { 
/**
 * UI language.
 */
locale: string, 
/**
 * Theme ID: built-in `light`/`dark` or a theme from the vault.
 */
theme: string, 
/**
 * What the note tree shows for a note: its title or its file name.
 */
noteLabel: NoteLabel, 
/**
 * Mode for a note that does not have its own mode yet.
 */
viewMode: ViewMode, 
/**
 * Open the last note at startup.
 */
openLastNote: boolean, 
/**
 * Line numbers in the editor gutter.
 */
lineNumbers: boolean, 
/**
 * Font size in percents for a note that does not have its own size yet.
 */
noteScale: number, 
/**
 * Check for a new version in the background and download it. The new
 * version starts after a restart. A developer build never checks.
 */
autoUpdate: boolean, };

/**
 * Built-in theme that the manifest goes on top of.
 *
 * A theme can set only a few variables, and the rest must come from somewhere:
 * a dark theme with a default white background is not a theme.
 */
export type Base = "light" | "dark";

/**
 * A bookmark as the editor sees it: a line of the open text and a name.
 * The window gets these with the note and sends them back on save.
 */
export type BookmarkMark = { 
/**
 * Line number, starting from 1.
 */
line: number, name?: string, };

/**
 * A bookmark in the palette list.
 */
export type BookmarkView = { note: string, title: string, folder: string | null, line: number, text: string, name?: string, };

/**
 * The working directory of a command.
 */
export type CommandCwd = "vault" | "target";

/**
 * What to show in the target menu.
 */
export type CommandList = { commands: Array<CommandView>, 
/**
 * Problems in the commands file. Users edit it by hand, so we must report
 * a typo. Otherwise the command silently does not appear, and kknow looks
 * broken.
 */
problems: Array<string>, };

/**
 * The whole manifest, as the settings form edits it.
 *
 * Unlike [`list_commands`], this includes **all** entries, also invalid ones.
 * Users fix them in the same form. Losing them on save is worse than showing
 * them with a problem.
 */
export type CommandManifest = { 
/**
 * File location: the form shows the path and can open it in Finder.
 */
path: string, commands: Array<CommandSpec>, 
/**
 * Environment shared by all commands in the file.
 */
env: { [key in string]: string }, problems: Array<string>, 
/**
 * Available runners. The core owns this list. If the window kept its own
 * copy, the "Run it with" field could one day offer what the core cannot
 * run.
 */
kinds: Array<string>, 
/**
 * The file did not parse. The form does not offer edits then: a save
 * would erase what we could not parse.
 */
broken: string | null, };

/**
 * One command as written in the manifest.
 */
export type CommandSpec = { 
/**
 * Stable name. The CLI and the app call the command by it.
 */
id: string, 
/**
 * Menu label.
 */
name: string, 
/**
 * Runner kind: `shell`, `bash`, `fish`, `zsh`, `python`.
 */
kind: string, 
/**
 * Script text.
 *
 * The target path is **not** inserted into it. It comes as an environment
 * variable. See [`crate::run`] for the reason.
 */
script: string, 
/**
 * Target kinds for the command. The default is all kinds: a manifest with
 * four fields must work, and the user can narrow the scope later.
 */
targets: Array<TargetKind>, 
/**
 * Vault IDs; empty means all vaults.
 */
vaults: Array<string>, 
/**
 * Path patterns from the vault root; empty means any path.
 */
match: Array<string>, 
/**
 * Note metadata fields that the command needs to appear.
 *
 * This binds a command parameter to a file, not to the manifest. A note
 * with `confluence: 123456789` (in frontmatter or a comment) gets the
 * "Send to Confluence" item, and other notes do not. An empty value counts
 * as missing: `confluence:` without a number does not match.
 */
requires: Array<string>, 
/**
 * Read the shell profile before the run.
 *
 * An app opened from Finder gets `PATH` from launchd, not from a shell,
 * and that `PATH` is minimal. The flag uses `sh -lc` instead of `sh -c`.
 * The default is off: the profile takes time and can print text into the
 * same stream that we show as command output.
 */
login: boolean, 
/**
 * Overrides the default program: python from a virtual environment, a
 * custom build tool, any program from `PATH` or a full path.
 */
program?: string | null, cwd: CommandCwd, timeoutSeconds: number, 
/**
 * Ask before the run. Only the app does this; the CLI has nobody to ask.
 */
confirm: boolean, 
/**
 * Extra environment variables for the process.
 */
env?: { [key in string]: string }, };

/**
 * A menu command: the label and what the window needs to decide.
 */
export type CommandView = { id: string, name: string, 
/**
 * What runs it. The window shows this as the second line of the menu item.
 */
kind: string, 
/**
 * Ask before the run.
 */
confirm: boolean, };

/**
 * Which version wins when both sides changed the same note.
 */
export type ConflictPolicy = "keepBoth" | "preferLocal" | "preferRemote";

export type ErrorView = { code: string, params: { [key in string]: string }, message: string, };

/**
 * A loose file in the sidebar.
 */
export type FileEntry = { path: string, 
/**
 * File name with extension. The title needs a disk read, and the list
 * must show at once.
 */
name: string, 
/**
 * The parent folder: it tells two `README.md` files apart.
 */
folder: string, };

/**
 * Result of saving a loose file. Same as for a note, but the copy is a path.
 */
export type FileSaveOutcome = { "status": "saved", note: FileView, } | { "status": "conflict", note: FileView, copy: string, };

/**
 * An open file. It has the same shape as a vault note, so the window shows
 * both in one editor. Here `id` is the absolute path, and `file` tells them
 * apart.
 */
export type FileView = { id: string, title: string, content: string, html: string, backlinks: Array<LinkRef>, outgoing: Array<LinkRef>, tags: Array<string>, file: boolean, };

/**
 * A global search result: a normal result plus the vault it came from.
 */
export type GlobalResult = { vault: string, vaultName: string, color: string, id: string, title: string, folder: string | null, snippet: string, };

/**
 * Action ID → its keys.
 */
export type Keymap = { [key in string]: Array<string> };

/**
 * The keys for the windows.
 */
export type KeymapView = { keys: Keymap, 
/**
 * The file path, for the hint on the keys page.
 */
file: string | null, 
/**
 * Skipped entries of a hand-edited file.
 */
problems: Array<string>, };

export type LinkRef = { id: string, title: string, label: string | null, };

/**
 * Which app opens markdown now.
 */
export type MarkdownAssociation = { 
/**
 * The platform lets the app set the default handler from here.
 */
supported: boolean, 
/**
 * Bundle ID of the app that opens `.md` now.
 */
current: string | null, 
/**
 * The current handler is kknow.
 */
ours: boolean, };

/**
 * What the note tree shows for a note.
 */
export type NoteLabel = "title" | "file";

export type NoteSummary = { id: string, title: string, folder: string | null, tags: Array<string>, };

export type NoteView = { id: string, title: string, content: string, html: string, backlinks: Array<LinkRef>, outgoing: Array<LinkRef>, tags: Array<string>, 
/**
 * Bookmarks in this text: 1-based lines with optional names.
 */
bookmarks: Array<BookmarkMark>, };

/**
 * What was opened from outside or from the window, and how to show it.
 */
export type Opened = { "kind": "note", vault: string, id: string, } | { "kind": "file", path: string, };

/**
 * The kind of storage on the other end.
 *
 * Cloud drive services do not get their own variants: they sync a plain
 * folder, so for kknow they are [`Provider::Folder`] with a path. Git does not
 * fit here: it has commits and its own conflict model, that is, a different
 * algorithm, not a different trait implementation.
 */
export type Provider = { "kind": "s3" } & S3Settings | { "kind": "folder", path: string, };

/**
 * Storage address and conflict policy.
 *
 * Unlike the manifest, this file has no format version: only kknow on the
 * same machine writes and reads it. The old format (S3 fields at the root)
 * is detected by the shape of the file, see [`Stored`].
 */
export type RemoteConfig = { provider: Provider, policy: ConflictPolicy, 
/**
 * Auto sync interval in minutes. `0` turns auto sync off.
 *
 * This is a directory setting, not an app setting: a work vault can sync
 * every five minutes, and an archive on an external disk never.
 */
autoSyncMinutes: number, };

/**
 * A moved note: its new ID and the number of notes whose links changed.
 */
export type RenamedNote = { id: string, relinked: number, };

/**
 * The result of a command run.
 */
export type RunOutcome = { 
/**
 * Command ID, so the app knows which command the report is for.
 */
command: string, 
/**
 * Exit code; `None` if the process was killed (timeout or signal).
 */
code: number | null, stdout: string, stderr: string, 
/**
 * The command exceeded `timeoutSeconds` and was stopped.
 */
timedOut: boolean, 
/**
 * The output is cut: only its start is kept.
 */
truncated: boolean, 
/**
 * Run time; the app shows it next to the exit code.
 */
millis: number, };

/**
 * Settings of S3-compatible storage: AWS, MinIO, R2.
 */
export type S3Settings = { bucket: string, 
/**
 * Storage address. AWS does not need it.
 */
endpoint?: string | null, region?: string | null, 
/**
 * Prefix inside the bucket, so one bucket can hold several vaults.
 */
prefix?: string | null, 
/**
 * Access key ID. It is not a secret: it is the login for the secret key,
 * and AWS keeps it in plain text in `~/.aws/credentials`.
 */
accessKeyId?: string | null, };

/**
 * Save result for the window.
 */
export type SaveOutcome = { "status": "saved", note: NoteView, } | { "status": "conflict", note: NoteView, copy: string, };

export type SearchResult = { id: string, title: string, folder: string | null, 
/**
 * Snippet with highlighted matches; its HTML is already escaped.
 */
snippet: string, };

/**
 * A conflict: both sides edited the same note.
 */
export type SyncConflict = { 
/**
 * The note that keeps the local version.
 */
id: string, 
/**
 * The copy that holds the remote version.
 */
copy: string, };

/**
 * What the sync did.
 */
export type SyncReport = { uploaded: number, downloaded: number, removedLocal: number, removedRemote: number, conflicts: Array<SyncConflict>, };

/**
 * Sync settings as the UI sees them.
 */
export type SyncSettings = { 
/**
 * Remote storage address as a string, to show to the user.
 */
location: string, 
/**
 * The secret key is already in the keyring. The form cannot show it, but
 * it must say that the key exists.
 */
hasSecret: boolean, 
/**
 * Whether a secret is needed at all. A folder has none, so the form hides
 * the key fields for it.
 */
needsSecret: boolean, provider: Provider, policy: ConflictPolicy, 
/**
 * Auto sync interval in minutes. `0` turns auto sync off.
 *
 * This is a directory setting, not an app setting: a work vault can sync
 * every five minutes, and an archive on an external disk never.
 */
autoSyncMinutes: number, };

export type SyncState = "off" | "idle" | "syncing" | "conflict" | "error";

/**
 * What to show for one vault.
 *
 * The whole struct is comparable. Windows redraw on each event, so sending an
 * unchanged status makes them redraw for nothing.
 */
export type SyncStatus = { vault: string, state: SyncState, 
/**
 * Sync progress: how many actions of the total are done.
 */
done: number, total: number, 
/**
 * End of the last successful sync, in ms since the epoch. The window
 * formats the time ("2 min ago") because the format depends on the
 * language.
 */
at: number | null, 
/**
 * What went wrong: a code with values, like all other errors.
 */
message: ErrorView | null, 
/**
 * Number of conflict copies in the folder.
 */
conflicts: number, 
/**
 * Sync target: a bucket or a folder.
 */
location: string | null, 
/**
 * Auto-sync interval in minutes; `0` means manual only.
 */
autoSyncMinutes: number, };

/**
 * The kind of target a command is attached to.
 */
export type TargetKind = "note" | "folder" | "vault";

/**
 * A task in the palette list.
 */
export type TaskView = { note: string, title: string, folder: string | null, line: number, text: string, done: boolean, };

/**
 * A parsed and checked theme.
 */
export type Theme = { 
/**
 * File name without extension; settings store the theme by it.
 */
id: string, name: string, author: string | null, base: Base, source: ThemeSource, 
/**
 * A vault theme that hides a shared theme with the same ID. For the UI
 * this means "Make shared" is already done, and the button now updates
 * the shared copy.
 */
shadowsGlobal: boolean, 
/**
 * Only the values that passed the check.
 */
variables: { [key in string]: string }, 
/**
 * What was dropped and why. The theme still works: one bad value must not
 * leave the user without a theme.
 */
warnings: Array<string>, };

/**
 * Themes of a directory and problems with files that are not themes.
 */
export type ThemeSet = { themes: Array<Theme>, 
/**
 * Files that did not parse. A theme must not disappear silently: its author
 * expects a result and will look for the cause.
 */
problems: Array<string>, };

/**
 * Where the theme comes from.
 *
 * This is for the user, not the UI: a shared theme and a vault theme are edited
 * in different places, and "Make shared" makes sense only for a vault theme.
 * A built-in theme has no file at all.
 */
export type ThemeSource = "vault" | "global" | "builtIn";

/**
 * One deletion in the trash of a vault.
 */
export type TrashEntryView = { 
/**
 * Entry name: restore takes it.
 */
id: string, path: string, 
/**
 * Deletion time in Unix seconds.
 */
deleted: number, 
/**
 * Notes in the entry: paths from the vault root, `Folder/note.md`.
 */
notes: Array<string>, };

/**
 * What the windows show.
 */
export type UpdateStatus = { "state": "unavailable" } | { "state": "idle" } | { "state": "checking" } | { "state": "downloading", version: string, } | { "state": "ready", version: string, } | { "state": "failed", message: string, };

export type VaultChanged = { 
/**
 * The vault ID: the window must not redraw for edits in an inactive vault.
 */
vault: string, updated: Array<string>, removed: Array<string>, };

export type VaultInfo = { 
/**
 * Vault ID: later commands and events refer to the vault by it.
 */
id: string, name: string, root: string, color: string, noteCount: number, };

/**
 * Vault totals for the settings window.
 */
export type VaultStats = { notes: number, folders: number, bytes: number, };

/**
 * A vault in the switcher list.
 *
 * There is no note count here: it needs the vault to be opened and parsed,
 * and the list must show at once on the first click.
 */
export type VaultView = { id: string, name: string, path: string, color: string, active: boolean, 
/**
 * The folder exists. An external disk can be disconnected.
 */
available: boolean, };

/**
 * How a note is shown: where the markup is drawn and if it can be edited.
 *
 * The window remembers the mode per note. This value is for notes that do not
 * have a mode yet.
 */
export type ViewMode = "edit" | "split" | "read";
