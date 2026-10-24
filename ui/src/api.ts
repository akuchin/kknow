import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** An open vault: what the window shows after a switch. */
export interface VaultInfo {
  id: string;
  name: string;
  root: string;
  /** Label color name; the theme sets the values (`--label-*`). */
  color: string;
  noteCount: number;
}

/** A vault in the switcher list. */
export interface VaultView {
  id: string;
  name: string;
  path: string;
  color: string;
  active: boolean;
  /** Whether the folder exists: an external disk can be unplugged. */
  available: boolean;
}

export interface NoteSummary {
  id: string;
  title: string;
  folder: string | null;
  tags: string[];
}

/** A moved note: its new ID and the number of notes with rewritten links. */
export interface RenamedNote {
  id: string;
  relinked: number;
}

export interface LinkRef {
  id: string;
  title: string;
  label: string | null;
}

export interface SearchResult {
  id: string;
  title: string;
  folder: string | null;
  snippet: string;
}

/** A global search hit: a normal hit plus the vault it comes from. */
export interface GlobalResult extends SearchResult {
  vault: string;
  vaultName: string;
  color: string;
}

/** Changes seen on disk. */
export interface VaultChanged {
  /** The vault. Changes in an inactive vault do not redraw the window. */
  vault: string;
  updated: string[];
  removed: string[];
}

/** A theme: values for the CSS variables in `:root`. */
export interface Theme {
  id: string;
  name: string;
  author: string | null;
  /** The built-in theme that the variables override. */
  base: "light" | "dark";
  /**
   * Built into the app, global theme folder or vault. A later one wins over an
   * earlier one with the same name: vault, then global, then built-in.
   */
  source: "builtIn" | "global" | "vault";
  /** A vault theme that has a global copy; this one hides the copy. */
  shadowsGlobal: boolean;
  /** Names without `--`. The core already checked the values. */
  variables: Record<string, string>;
  /** What the core dropped from the manifest and why. */
  warnings: string[];
}

export interface ThemeSet {
  themes: Theme[];
  /** Files that are not valid themes. */
  problems: string[];
}

/** How to resolve a conflict: keep both versions, the local one or the remote one. */
export type ConflictPolicy = "keepBoth" | "preferLocal" | "preferRemote";

/** Settings for S3-compatible storage. */
export interface S3Settings {
  bucket: string;
  endpoint: string | null;
  region: string | null;
  prefix: string | null;
  accessKeyId: string | null;
}

/**
 * The remote side of sync.
 *
 * Cloud drive services get no own kind: they sync a plain folder, so for
 * kknow they are a `folder` with a path.
 */
export type Provider =
  | ({ kind: "s3" } & S3Settings)
  | { kind: "folder"; path: string };

export interface SyncSettings {
  provider: Provider;
  policy: ConflictPolicy;
  /** Auto sync interval in minutes; 0 means manual only. */
  autoSyncMinutes: number;
  location: string;
  /** The secret is already in the keychain. */
  hasSecret: boolean;
  /** Whether a secret is needed at all: a folder never has one. */
  needsSecret: boolean;
}

/** Note mode: inline markup, markdown with a preview beside it, or read only. */
export type ViewMode = "edit" | "split" | "read";

/** What the note tree shows: the first heading or the file name. */
export type NoteLabel = "title" | "file";

/** App settings: language, theme and editor behaviour. */
export interface AppSettings {
  locale: string;
  theme: string;
  noteLabel: NoteLabel;
  /** The mode for a note that has no mode of its own yet. */
  viewMode: ViewMode;
  openLastNote: boolean;
  /** Line numbers in the editor gutter. */
  lineNumbers: boolean;
  /** Font size in percents for a file without its own size. */
  noteScale: number;
  /** Check for a new version in the background and download it. */
  autoUpdate: boolean;
}

/** User keys from `keybindings.json`: action ID → its keys. */
export interface KeymapView {
  keys: Record<string, string[]>;
  /** The file path; `null` if there is no settings folder. */
  file: string | null;
  /** Skipped entries of a hand-edited file. */
  problems: string[];
}

/**
 * Auto-update state, the same as `UpdateStatus` in `updater.rs`.
 * `unavailable` is a developer build: it never checks.
 */
export type UpdateStatus =
  | { state: "unavailable" }
  | { state: "idle" }
  | { state: "checking" }
  | { state: "downloading"; version: string }
  | { state: "ready"; version: string }
  | { state: "failed"; message: string };

/** Vault totals for the settings window. */
export interface VaultStats {
  notes: number;
  folders: number;
  bytes: number;
}

/** Sync state of one vault. */
export type SyncState = "off" | "idle" | "syncing" | "conflict" | "error";

export interface SyncStatus {
  vault: string;
  state: SyncState;
  /** Sync progress: actions done out of the total. */
  done: number;
  total: number;
  /** End of the last successful sync, in ms since the epoch. */
  at: number | null;
  message: string | null;
  /** Number of conflict copies in the folder. */
  conflicts: number;
  location: string | null;
  autoSyncMinutes: number;
}

export interface SyncConflict {
  id: string;
  copy: string;
}

export interface SyncReport {
  uploaded: number;
  downloaded: number;
  removedLocal: number;
  removedRemote: number;
  conflicts: SyncConflict[];
}

export interface NoteView {
  id: string;
  title: string;
  content: string;
  html: string;
  backlinks: LinkRef[];
  outgoing: LinkRef[];
  tags: string[];
  /** Bookmarks of the text. Standalone files have no bookmarks. */
  bookmarks?: BookmarkMark[];
  /**
   * A standalone file, not a vault note. Then `id` is an absolute path, and
   * the `*_file` commands read and write it.
   */
  file?: boolean;
}

/** A bookmark as the editor sees it: a line of the open text and a name. */
export interface BookmarkMark {
  /** Line number, starting at 1. */
  line: number;
  /** The name the user gave it. Never empty: no name means no field. */
  name?: string;
}

/** A bookmark in the palette: a note of the active vault and a line in it. */
export interface BookmarkView {
  note: string;
  title: string;
  folder: string | null;
  /** Line number, starting at 1. */
  line: number;
  /** Line text at the time the bookmark was set or the note was saved. */
  text: string;
  name?: string;
}

/** A task list item of a vault note: `- [ ] text`. */
export interface TaskView {
  note: string;
  title: string;
  folder: string | null;
  /** Line number, starting at 1. */
  line: number;
  text: string;
  done: boolean;
}

/** A standalone file in the sidebar: markdown opened without a vault. */
export interface FileEntry {
  path: string;
  /** File name with extension. */
  name: string;
  /** The parent folder. */
  folder: string;
}

/**
 * What was opened. A file inside a known vault is a note of that vault; other
 * files are standalone files.
 */
export type Opened =
  | { kind: "note"; vault: string; id: string }
  | { kind: "file"; path: string };

/** The system default app for `.md` files. */
export interface MarkdownAssociation {
  /** Whether the app can set the default app (macOS only). */
  supported: boolean;
  /** ID of the app that opens `.md` now. */
  current: string | null;
  /** The current app is kknow. */
  ours: boolean;
}

/** Save result: saved, or the file was changed outside the app first. */
export type SaveOutcome =
  | { status: "saved"; note: NoteView }
  | { status: "conflict"; note: NoteView; copy: string };

/** What a command attaches to. */
export type TargetKind = "note" | "folder" | "vault";

/**
 * Command target.
 *
 * It has no path on purpose. The core computes the path from the note ID or
 * folder name, so the window cannot point a command at an arbitrary file. The
 * reveal-in-file-manager command uses the same approach.
 */
export interface CommandTarget {
  kind: TargetKind;
  /** Note ID or folder path from the root; empty for a vault. */
  name?: string;
  /** The vault; the active one if empty. */
  vault?: string;
  /**
   * Absolute path of a standalone file. It is a note without a vault: it gets
   * the commands with no `vaults` and no `match`.
   */
  file?: string;
}

/** A command in the target menu. */
export interface CommandView {
  id: string;
  name: string;
  /** The runner: `shell`, `fish`, `python`, and so on. */
  kind: string;
  /** Ask before the run. */
  confirm: boolean;
}

export interface CommandList {
  commands: CommandView[];
  /** Problems in the command file: a typo must be reported, not ignored. */
  problems: string[];
}

/** Working folder of a command: the vault root or the target folder. */
export type CommandCwd = "vault" | "target";

/**
 * A command as the settings form edits it: the full manifest entry.
 *
 * Unlike `CommandView` (what the menu needs), it has all fields, also those
 * that can be absent from the file. The core does not write empty `program`
 * and `env`, so the parsed entry can lack them.
 */
export interface CommandSpec {
  id: string;
  name: string;
  kind: string;
  script: string;
  /** Where the command attaches; if empty, the command appears nowhere. */
  targets: TargetKind[];
  /** Vaults it applies to; empty means all. */
  vaults: string[];
  /** Path patterns from the vault root; empty means any path. */
  match: string[];
  /** Note fields that must exist for the command to appear. */
  requires: string[];
  /** Load the shell profile before the run. */
  login: boolean;
  /** Program to use instead of the default one. */
  program?: string | null;
  cwd: CommandCwd;
  timeoutSeconds: number;
  /** Ask before the run. */
  confirm: boolean;
  /** Extra environment variables for the process. */
  env?: Record<string, string>;
}

/** The full manifest that the settings form edits. */
export interface CommandManifest {
  /** File location. The form shows it and can reveal it in the file manager. */
  path: string;
  /** All entries, also invalid ones: the user fixes them in the same form. */
  commands: CommandSpec[];
  /** Environment shared by all commands in the file. */
  env: Record<string, string>;
  problems: string[];
  /** Allowed runners; the core owns the list. */
  kinds: string[];
  /** Parse error. The form cannot edit a broken file; only manual edits work. */
  broken: string | null;
}

/** The result of a command run. */
export interface RunOutcome {
  command: string;
  /** `null`: the process was killed by the timeout or by a signal. */
  code: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  /** The output is cut; only the start is shown. */
  truncated: boolean;
  millis: number;
}

/**
 * Core commands.
 *
 * Commands do not name the vault: they work with the active one. An explicit
 * name is needed only when the action is about another vault; the settings
 * window does that.
 */
export const api = {
  listVaults: () => invoke<VaultView[]>("list_vaults"),
  addVault: (path: string) => invoke<VaultInfo>("add_vault", { path }),
  removeVault: (id: string) => invoke<VaultView[]>("remove_vault", { id }),
  activateVault: (id: string) => invoke<VaultInfo>("activate_vault", { id }),
  /** The vault for ⌘1…⌘9; a number can have no vault. */
  vaultByHotkey: (number: number) =>
    invoke<string | null>("vault_by_hotkey", { number }),

  renameVault: (id: string, name: string) =>
    invoke<VaultView[]>("rename_vault", { id, name }),
  setVaultColor: (id: string, color: string) =>
    invoke<VaultView[]>("set_vault_color", { id, color }),
  vaultStats: (vault?: string) =>
    invoke<VaultStats>("vault_stats", { vault: vault ?? null }),
  /** Opens the settings window at the given section. */
  openSettings: (section?: string) =>
    invoke<void>("open_settings", { section: section ?? null }),

  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (settings: AppSettings) => invoke<AppSettings>("set_settings", { settings }),

  /** The changed keys. The core reads the file each time, so a hand edit shows. */
  getKeymap: () => invoke<KeymapView>("get_keymap"),
  setKeymap: (keys: Record<string, readonly string[]>) =>
    invoke<KeymapView>("set_keymap", { keys }),
  /** Shows `keybindings.json` in Finder; creates it empty if needed. */
  revealKeymap: () => invoke<void>("reveal_keymap"),

  currentVault: () => invoke<VaultInfo | null>("current_vault"),
  reload: () => invoke<VaultInfo>("reload_vault"),
  /** Notes of a vault, the active one by default. This opens another vault if needed. */
  listNotes: (vault?: string) =>
    invoke<NoteSummary[]>("list_notes", { vault: vault ?? null }),
  loadNote: (id: string) => invoke<NoteView>("load_note", { id }),
  /**
   * `base` is the text the edit started from. With it the core does not
   * overwrite an outside change; it saves our text as a copy instead.
   */
  saveNote: (id: string, content: string, base: string | null, bookmarks?: BookmarkMark[]) =>
    invoke<SaveOutcome>("save_note", { id, content, base, bookmarks: bookmarks ?? null }),
  /**
   * Sets note bookmarks on these lines of `content` without saving the note.
   * Returns the lines as the core understood them.
   */
  setBookmarks: (id: string, lines: BookmarkMark[], content: string) =>
    invoke<BookmarkMark[]>("set_bookmarks", { id, lines, content }),
  /** Bookmarks of the active vault, in the order they were set. */
  listBookmarks: () => invoke<BookmarkView[]>("list_bookmarks"),
  /** Tasks of the active vault, sorted by note and line. */
  listTasks: () => invoke<TaskView[]>("list_tasks"),
  /**
   * Marks the task on a note line as done or open. The core writes the file
   * and refuses a line that is no longer a task.
   */
  setTask: (id: string, line: number, done: boolean) =>
    invoke<NoteView>("set_task", { id, line, done }),
  preview: (id: string, content: string) =>
    invoke<string>("preview", { id, content }),
  /** Bytes of an image that the note links to. The core resolves the path. */
  noteImage: (id: string, target: string) =>
    invoke<ArrayBuffer | number[]>("note_image", { id, target }),
  /**
   * Saves a pasted image into `attachments/`: at the vault root, or next to
   * the standalone `file`. The body is raw bytes, so the name goes in a
   * header. Returns the path for the link.
   */
  saveAttachment: (name: string, bytes: Uint8Array, file: string | null) =>
    invoke<string>("save_attachment", bytes, {
      headers: {
        "kknow-name": encodeURIComponent(name),
        ...(file === null ? {} : { "kknow-file": encodeURIComponent(file) }),
      },
    }),
  /** Copies an image file dropped on the editor into `attachments/`, like `saveAttachment`. */
  attachFile: (source: string, file: string | null) =>
    invoke<string>("attach_file", { source, file }),
  createNote: (name: string) => invoke<NoteView>("create_note", { name }),
  deleteNote: (id: string, vault?: string) =>
    invoke<void>("delete_note", { id, vault: vault ?? null }),
  /** Vault folders, also empty ones that no note uses. */
  listFolders: (vault?: string) =>
    invoke<string[]>("list_folders", { vault: vault ?? null }),
  createFolder: (name: string) => invoke<void>("create_folder", { name }),
  deleteFolder: (name: string, vault?: string) =>
    invoke<void>("delete_folder", { name, vault: vault ?? null }),
  /** Reveal in the file manager; without `id` reveals the vault folder. */
  revealNote: (id?: string, vault?: string) =>
    invoke<void>("reveal_note", { id: id ?? null, vault: vault ?? null }),
  revealFolder: (name: string, vault?: string) =>
    invoke<void>("reveal_folder", { name, vault: vault ?? null }),
  /** Opens the note file in the system default app for `.md`. */
  openExternally: (id: string, vault?: string) =>
    invoke<void>("open_note", { id, vault: vault ?? null }),
  /**
   * Renames or moves a note: `to` is the full path from the root.
   * Returns the new ID and the number of notes whose links followed it.
   */
  renameNote: (id: string, to: string, vault?: string) =>
    invoke<RenamedNote>("rename_note", { id, to, vault: vault ?? null }),
  /** Makes a copy next to the note and returns the copy ID. */
  duplicateNote: (id: string, vault?: string) =>
    invoke<string>("duplicate_note", { id, vault: vault ?? null }),
  /** Returns the number of notes whose links followed the folder. */
  renameFolder: (name: string, to: string, vault?: string) =>
    invoke<number>("rename_folder", { name, to, vault: vault ?? null }),
  search: (query: string, limit?: number) =>
    invoke<SearchResult[]>("search", { query, limit }),
  /** Searches all vaults at once; each hit knows its vault. */
  searchAll: (query: string, limit?: number) =>
    invoke<GlobalResult[]>("search_all", { query, limit }),

  listThemes: (vault?: string) => invoke<ThemeSet>("list_themes", { vault: vault ?? null }),
  /** Copies a vault theme to the global folder, so all vaults see it. */
  shareTheme: (id: string, vault?: string) =>
    invoke<void>("share_theme", { id, vault: vault ?? null }),
  /** Reveals the global theme folder in the file manager; creates it if missing. */
  revealThemes: () => invoke<void>("reveal_themes"),

  /** Commands that apply to the target, for its menu. */
  listCommands: (target: CommandTarget) =>
    invoke<CommandList>("list_commands", {
      kind: target.kind,
      target: target.name ?? null,
      vault: target.vault ?? null,
      file: target.file ?? null,
    }),
  /** Runs a command and waits for it. The core runs it off the main thread. */
  runCommand: (command: string, target: CommandTarget) =>
    invoke<RunOutcome>("run_command", {
      command,
      kind: target.kind,
      target: target.name ?? null,
      vault: target.vault ?? null,
      file: target.file ?? null,
    }),
  /** The full manifest for the settings form. */
  editCommands: () => invoke<CommandManifest>("edit_commands"),
  /** Writes the full manifest and returns it with its problems. */
  saveCommands: (commands: CommandSpec[], env: Record<string, string>) =>
    invoke<CommandManifest>("save_commands", { commands, env }),
  /** Reveals the command file in the file manager; creates it from a template if missing. */
  revealCommands: () => invoke<void>("reveal_commands"),

  syncSettings: (vault?: string) =>
    invoke<SyncSettings | null>("sync_settings", { vault: vault ?? null }),
  /** `secret: null` keeps the secret stored in the keychain. */
  saveSyncSettings: (settings: SyncSettingsForm, secret: string | null, vault?: string) =>
    invoke<SyncSettings>("save_sync_settings", { settings, secret, vault: vault ?? null }),
  forgetSyncSettings: (vault?: string) =>
    invoke<void>("forget_sync_settings", { vault: vault ?? null }),
  /** Standalone files. The core also forgets files that are gone from disk. */
  listFiles: () => invoke<FileEntry[]>("list_files"),
  /** Opens a file picked in the dialog or dropped on the window. */
  openFile: (path: string) => invoke<Opened>("open_file", { path }),
  /** An "open this" request from the system that came before the window existed. */
  takeOpenedFile: () => invoke<Opened | null>("take_opened_file"),
  loadFile: (path: string) => invoke<NoteView>("load_file", { path }),
  /** Like `saveNote`: with `base` the core does not overwrite an outside change. */
  saveFile: (path: string, content: string, base: string | null) =>
    invoke<SaveOutcome>("save_file", { path, content, base }),
  previewFile: (path: string, content: string) =>
    invoke<string>("preview_file", { path, content }),
  fileImage: (path: string, target: string) =>
    invoke<ArrayBuffer | number[]>("file_image", { path, target }),
  /** Removes the file from the list; the file stays on disk. */
  forgetFile: (path: string) => invoke<FileEntry[]>("forget_file", { path }),
  revealFile: (path: string) => invoke<void>("reveal_file", { path }),

  markdownAssociation: () => invoke<MarkdownAssociation>("markdown_association"),
  /** Makes kknow the default app for `.md`. */
  claimMarkdown: () => invoke<MarkdownAssociation>("claim_markdown"),

  syncNow: (vault?: string) => invoke<SyncReport>("sync_now", { vault: vault ?? null }),
  /** Sync state of the active vault. */
  syncStatus: () => invoke<SyncStatus>("sync_status"),
  /** Sync states of all open vaults: the chip also shows conflicts in other vaults. */
  syncStatuses: () => invoke<SyncStatus[]>("sync_statuses"),

  updateStatus: () => invoke<UpdateStatus>("update_status"),
  /** Checks now and downloads a new version; resolves with the final state. */
  checkUpdate: () => invoke<UpdateStatus>("check_update"),
  /** Installs the downloaded version and restarts. Save the open note first. */
  restartToUpdate: () => invoke<void>("restart_to_update"),
};

/** What the form knows about sync: everything except the secret. */
export type SyncSettingsForm = Omit<
  SyncSettings,
  "location" | "hasSecret" | "needsSecret"
>;

/** Subscribes to changes made outside the app. */
export function onVaultChanged(handler: (changed: VaultChanged) => void) {
  return listen<VaultChanged>("vault-changed", (event) => handler(event.payload));
}

/**
 * A folder opened from outside: `open -a kknow ~/notes` or "Open With".
 *
 * The core has already added the vault and made it active; the window only
 * shows it. The payload is the vault ID.
 */
export function onVaultOpened(handler: (id: string) => void) {
  return listen<string>("vault-opened", (event) => handler(event.payload));
}

/**
 * A file opened from outside: a double click on `.md`, `open -a kknow README.md`.
 *
 * The event is empty. The core keeps the request, and the window takes it with
 * `take_opened_file`. So the request is not lost if it comes before the window
 * loads.
 */
export function onFileOpened(handler: () => void) {
  return listen<null>("file-opened", () => handler());
}

/** Subscribes to settings: there are two windows, and they must agree. */
export function onSettingsChanged(handler: (settings: AppSettings) => void) {
  return listen<AppSettings>("settings-changed", (event) => handler(event.payload));
}

/** Subscribes to keys: the settings window changes them for both windows. */
export function onKeymapChanged(handler: (keymap: KeymapView) => void) {
  return listen<KeymapView>("keymap-changed", (event) => handler(event.payload));
}

/** The current drag and drop state. */
export interface DragDrop {
  /** Files are over the window: show the hint. */
  over: boolean;
  /** Dropped paths; empty while dragging. */
  paths: string[];
  /** The drop point in CSS pixels of the page; only with `paths`. */
  point?: { x: number; y: number };
}

/**
 * Drag and drop from the file manager.
 *
 * The event comes from the shell, not from the DOM. The browser `drop` gives
 * file contents, but a vault needs the folder path, and only the shell knows
 * it. The subscription is on the webview, not global: `tauri://drag-*` events
 * go to one webview, and `listen` without a target does not see them.
 */
export function onDragDrop(handler: (state: DragDrop) => void) {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "enter" || payload.type === "over") {
      handler({ over: true, paths: [] });
    } else if (payload.type === "drop") {
      // The shell gives physical pixels; `elementFromPoint` needs CSS pixels.
      const scale = window.devicePixelRatio || 1;
      const point = { x: payload.position.x / scale, y: payload.position.y / scale };
      handler({ over: false, paths: payload.paths, point });
    } else {
      handler({ over: false, paths: [] });
    }
  });
}

/**
 * Runs `handler` before the window closes and waits for it.
 *
 * Tauri keeps the window open while a JS listener runs, then closes it.
 */
export function onCloseRequested(handler: () => Promise<void>) {
  return getCurrentWindow().onCloseRequested(() => handler());
}

/** Subscribes to sync state: progress, conflicts, errors. */
export function onSyncStatus(handler: (status: SyncStatus) => void) {
  return listen<SyncStatus>("sync-status", (event) => handler(event.payload));
}

/** Auto-update progress: from the timer, the settings button and the menu. */
export function onUpdateStatus(handler: (status: UpdateStatus) => void) {
  return listen<UpdateStatus>("update-status", (event) => handler(event.payload));
}

/** Parses a `kknow://note/<id>` link made by the renderer. */
export function parseLink(href: string): { kind: string; value: string } | null {
  const prefix = "kknow://";
  if (!href.startsWith(prefix)) return null;

  const rest = href.slice(prefix.length);
  const slash = rest.indexOf("/");
  if (slash < 0) return null;

  return {
    kind: rest.slice(0, slash),
    value: decodeURIComponent(rest.slice(slash + 1)),
  };
}
