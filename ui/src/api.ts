import { Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { call } from "./command";
import { commandName } from "./core-text";

import type {
  AppSettings,
  BookmarkMark,
  BookmarkView,
  ChangeKind,
  CommandCwd,
  CommandList,
  CommandManifest,
  CommandProblem,
  CommandSpec,
  CommandView,
  ConflictPolicy,
  FileEntry,
  GitAuth,
  GitChange,
  GitStatus,
  GlobalResult,
  KeymapView,
  KeyProblem,
  LinkRef,
  MarkdownAssociation,
  McpStatus,
  NoteLabel,
  NoteSummary,
  Opened,
  Provider,
  RenamedNote,
  RunLine,
  RunOutcome,
  S3Settings,
  SearchResult,
  EncryptionOn,
  SyncConflict,
  SyncReport,
  SyncSettings,
  SyncState,
  SyncStatus,
  TargetKind,
  TaskView,
  LinkTargetView,
  BacklinkLineView,
  MentionView,
  LinkPreview,
  LockStateView,
  UnlockedBy,
  HeadingView,
  Theme,
  ThemeProblem,
  ThemeSet,
  ThemeWarning,
  TrashEntryView,
  UpdateStatus,
  VaultChanged,
  VaultInfo,
  VaultStats,
  VaultView,
  ViewMode,
  FileView,
  NoteView as VaultNoteView,
  ProposalEdit,
  ProposalView,
  Review,
  Thread,
  ThreadEvent,
  ThreadMessage,
  ThreadStatus,
  RemoteConfig,
} from "./bindings";

export type {
  AppSettings,
  BookmarkMark,
  BookmarkView,
  ChangeKind,
  CommandCwd,
  CommandList,
  CommandManifest,
  CommandProblem,
  CommandSpec,
  CommandView,
  ConflictPolicy,
  FileEntry,
  GitAuth,
  GitChange,
  GitStatus,
  GlobalResult,
  KeymapView,
  KeyProblem,
  LinkRef,
  MarkdownAssociation,
  McpStatus,
  NoteLabel,
  NoteSummary,
  Opened,
  Provider,
  RenamedNote,
  RunLine,
  RunOutcome,
  S3Settings,
  SearchResult,
  EncryptionOn,
  SyncConflict,
  SyncReport,
  SyncSettings,
  SyncState,
  SyncStatus,
  TargetKind,
  TaskView,
  LinkTargetView,
  BacklinkLineView,
  MentionView,
  LinkPreview,
  LockStateView,
  UnlockedBy,
  HeadingView,
  Theme,
  ThemeProblem,
  ThemeSet,
  ThemeWarning,
  TrashEntryView,
  UpdateStatus,
  VaultChanged,
  ProposalEdit,
  ProposalView,
  Review,
  Thread,
  ThreadEvent,
  ThreadMessage,
  ThreadStatus,
  VaultInfo,
  VaultStats,
  VaultView,
  ViewMode,
};

export type NoteView = Omit<
  VaultNoteView,
  "bookmarks" | "threads" | "proposals" | "locked" | "closed"
> & {
  bookmarks?: BookmarkMark[];
  locked?: boolean;
  closed?: boolean;
  threads?: Thread[];
  proposals?: ProposalView[];
  file?: boolean;
};

export const fileIsNote = (file: FileView): NoteView => file;

export type SaveOutcome =
  | { status: "saved"; note: NoteView }
  | { status: "conflict"; note: NoteView; copy: string };

export type SyncSettingsForm = RemoteConfig;

export interface CommandTarget {
  kind: TargetKind;
  name?: string;
  vault?: string;
  file?: string;
}

export const api = {
  listVaults: () => call("list_vaults"),
  addVault: (path: string) => call("add_vault", { path }),
  removeVault: (id: string) => call("remove_vault", { id }),
  activateVault: (id: string) => call("activate_vault", { id }),
  vaultByHotkey: (number: number) =>
    call("vault_by_hotkey", { number }),

  renameVault: (id: string, name: string) =>
    call("rename_vault", { id, name }),
  setVaultColor: (id: string, color: string) =>
    call("set_vault_color", { id, color }),
  setVaultLabels: (id: string, labels: string[]) =>
    call("set_vault_labels", { id, labels }),
  vaultStats: (vault?: string) =>
    call("vault_stats", { vault: vault ?? null }),
  openSettings: (section?: string) =>
    call("open_settings", { section: section ?? null }),

  getSettings: () => call("get_settings"),
  setSettings: (settings: AppSettings) => call("set_settings", { settings }),

  getKeymap: () => call("get_keymap"),
  setKeymap: (keys: Record<string, readonly string[]>) =>
    call("set_keymap", {
      keys: Object.fromEntries(Object.entries(keys).map(([action, chords]) => [action, [...chords]])),
    }),
  revealKeymap: () => call("reveal_keymap"),

  currentVault: () => call("current_vault"),
  reload: () => call("reload_vault"),
  listNotes: (vault?: string) =>
    call("list_notes", { vault: vault ?? null }),
  loadNote: (id: string) => call("load_note", { id }),
  saveNote: (id: string, content: string, base: string | null, bookmarks?: BookmarkMark[]) =>
    call("save_note", { id, content, base, bookmarks: bookmarks ?? null }),
  setBookmarks: (id: string, lines: BookmarkMark[], content: string) =>
    call("set_bookmarks", { id, lines, content }),
  listBookmarks: () => call("list_bookmarks"),
  listTasks: () => call("list_tasks"),
  setTask: (id: string, line: number, done: boolean) =>
    call("set_task", { id, line, done }),
  reviewProposal: (proposal: string, review: Review) =>
    call("review_proposal", { proposal, review }),
  dismissProposal: (proposal: string) => call("dismiss_proposal", { proposal }),
  replyThread: (
    note: string,
    target: { thread: string } | { line: number; text: string },
    message: string,
  ) =>
    call("reply_thread", {
      note,
      thread: "thread" in target ? target.thread : null,
      line: "line" in target ? target.line : null,
      text: "text" in target ? target.text : null,
      message,
    }),
  preview: (id: string, content: string) =>
    call("preview", { id, content }),
  noteImage: (id: string, target: string) =>
    call("note_image", { id, target }),
  saveAttachment: (name: string, bytes: Uint8Array, file: string | null) =>
    call("save_attachment", bytes, {
      headers: {
        "kknow-name": encodeURIComponent(name),
        ...(file === null ? {} : { "kknow-file": encodeURIComponent(file) }),
      },
    }),
  attachFile: (source: string, file: string | null) =>
    call("attach_file", { source, file }),
  createNote: (name: string) => call("create_note", { name }),
  linkTarget: (from: string, target: string) =>
    call("link_target", { from, target }),
  linkPreview: (from: string, target: string) =>
    call("link_preview", { from, target }),
  noteBacklinks: (id: string) => call("note_backlinks", { id }),
  unlinkedMentions: (id: string) => call("unlinked_mentions", { id }),
  linkMention: (mention: MentionView, target: string) =>
    call("link_mention", {
      from: mention.id,
      start: mention.start,
      end: mention.end,
      text: mention.text,
      target,
    }),
  deleteNote: (id: string, vault?: string) =>
    call("delete_note", { id, vault: vault ?? null }),
  listTrash: (vault: string) => call("list_trash", { vault }),
  restoreTrash: (entry: string, vault: string) =>
    call("restore_trash", { entry, vault }),
  trashNote: (entry: string, note: string, vault: string) =>
    call("trash_note", { entry, note, vault }),
  deleteTrashEntry: (entry: string, vault: string) =>
    call("delete_trash_entry", { entry, vault }),
  emptyTrash: (vault: string) => call("empty_trash", { vault }),
  listFolders: (vault?: string) =>
    call("list_folders", { vault: vault ?? null }),
  createFolder: (name: string) => call("create_folder", { name }),
  deleteFolder: (name: string, vault?: string) =>
    call("delete_folder", { name, vault: vault ?? null }),
  revealNote: (id?: string, vault?: string) =>
    call("reveal_note", { id: id ?? null, vault: vault ?? null }),
  revealFolder: (name: string, vault?: string) =>
    call("reveal_folder", { name, vault: vault ?? null }),
  openExternally: (id: string, vault?: string) =>
    call("open_note", { id, vault: vault ?? null }),
  renameNote: (id: string, to: string, vault?: string) =>
    call("rename_note", { id, to, vault: vault ?? null }),
  duplicateNote: (id: string, vault?: string) =>
    call("duplicate_note", { id, vault: vault ?? null }),
  renameFolder: (name: string, to: string, vault?: string) =>
    call("rename_folder", { name, to, vault: vault ?? null }),
  search: (query: string, limit?: number) =>
    call("search", { query, limit }),
  searchAll: (query: string, limit?: number, vaults?: string[]) =>
    call("search_all", { query, limit, vaults: vaults ?? null }),

  listThemes: (vault?: string) => call("list_themes", { vault: vault ?? null }),
  shareTheme: (id: string, vault?: string) =>
    call("share_theme", { id, vault: vault ?? null }),
  revealThemes: () => call("reveal_themes"),
  mcpStatus: () => call("mcp_status"),
  setMcp: (enabled: boolean, port: number) => call("set_mcp", { enabled, port }),

  listCommands: (target: CommandTarget) =>
    call("list_commands", {
      kind: target.kind,
      target: target.name ?? null,
      vault: target.vault ?? null,
      file: target.file ?? null,
    }).then((list) => ({
      ...list,
      commands: list.commands.map((command) => ({ ...command, name: commandName(command) })),
    })),
  runCommand: (
    command: string,
    target: CommandTarget,
    run: string,
    onLine: (line: RunLine) => void,
  ) => {
    const lines = new Channel<RunLine>();
    lines.onmessage = onLine;
    return call("run_command", {
      command,
      kind: target.kind,
      target: target.name ?? null,
      vault: target.vault ?? null,
      file: target.file ?? null,
      run,
      lines,
    });
  },
  stopCommand: (run: string) => call("stop_command", { run }),
  editCommands: () => call("edit_commands"),
  saveCommands: (commands: CommandSpec[], env: Record<string, string>) =>
    call("save_commands", { commands, env }),
  revealCommands: () => call("reveal_commands"),

  syncSettings: (vault?: string) =>
    call("sync_settings", { vault: vault ?? null }),
  saveSyncSettings: (settings: SyncSettingsForm, secret: string | null, vault?: string) =>
    call("save_sync_settings", { settings, secret, vault: vault ?? null }),
  forgetSyncSettings: (vault?: string) =>
    call("forget_sync_settings", { vault: vault ?? null }),
  syncEncrypt: (passphrase: string, vault?: string) =>
    call("sync_encrypt", { passphrase, vault: vault ?? null }),
  syncUnlock: (secret: string, vault?: string) =>
    call("sync_unlock", { secret, vault: vault ?? null }),
  syncChangePassphrase: (current: string, next: string, vault?: string) =>
    call("sync_change_passphrase", { current, new: next, vault: vault ?? null }),
  lockState: (vault?: string) => call("lock_state", { vault: vault ?? null }),
  setLockPassword: (password: string, vault?: string) =>
    call("set_lock_password", { password, vault: vault ?? null }),
  unlockNotes: (secret: string, vault?: string) =>
    call("unlock_notes", { secret, vault: vault ?? null }),
  changeLockPassword: (password: string, vault?: string) =>
    call("change_lock_password", { password, vault: vault ?? null }),
  lockAll: (vault?: string) => call("lock_all", { vault: vault ?? null }),
  lockNote: (id: string, vault?: string) =>
    call("lock_note", { id, vault: vault ?? null }),
  removeLock: (id: string, vault?: string) =>
    call("remove_lock", { id, vault: vault ?? null }),
  listFiles: () => call("list_files"),
  openFile: (path: string) => call("open_file", { path }),
  takeOpenedFile: () => call("take_opened_file"),
  loadFile: (path: string) => call("load_file", { path }),
  saveFile: (path: string, content: string, base: string | null) =>
    call("save_file", { path, content, base }),
  previewFile: (path: string, content: string) =>
    call("preview_file", { path, content }),
  fileImage: (path: string, target: string) =>
    call("file_image", { path, target }),
  forgetFile: (path: string) => call("forget_file", { path }),
  revealFile: (path: string) => call("reveal_file", { path }),

  markdownAssociation: () => call("markdown_association"),
  claimMarkdown: () => call("claim_markdown"),

  syncNow: (vault?: string) => call("sync_now", { vault: vault ?? null }),

  gitLocate: () => call("git_locate"),
  gitVersion: (path: string) => call("git_version", { path }),
  connectGit: (
    vault: string,
    url: string | null,
    auth: GitAuth,
    env: string | null,
    token: string | null,
  ) => call("connect_git", { vault, url, auth, env, token }),
  gitStatus: (vault?: string) => call("git_status", { vault: vault ?? null }),
  gitStage: (paths: string[], vault?: string) =>
    call("git_stage", { paths, vault: vault ?? null }),
  gitUnstage: (paths: string[], vault?: string) =>
    call("git_unstage", { paths, vault: vault ?? null }),
  gitDiscard: (path: string, from: string | null, staged: boolean, vault?: string) =>
    call("git_discard", { path, from, staged, vault: vault ?? null }),
  gitDiff: (path: string, from: string | null, staged: boolean, vault?: string) =>
    call("git_diff", { path, from, staged, vault: vault ?? null }),
  gitCommit: (message: string, vault?: string) =>
    call("git_commit", { message, vault: vault ?? null }),
  syncStatus: () => call("sync_status"),
  syncStatuses: () => call("sync_statuses"),

  updateStatus: () => call("update_status"),
  checkUpdate: () => call("check_update"),
  restartToUpdate: () => call("restart_to_update"),
};

export function onVaultChanged(handler: (changed: VaultChanged) => void) {
  return listen<VaultChanged>("vault-changed", (event) => handler(event.payload));
}

export function onVaultOpened(handler: (id: string) => void) {
  return listen<string>("vault-opened", (event) => handler(event.payload));
}

export function onFileOpened(handler: () => void) {
  return listen<null>("file-opened", () => handler());
}

export function onSettingsChanged(handler: (settings: AppSettings) => void) {
  return listen<AppSettings>("settings-changed", (event) => handler(event.payload));
}

export function onKeymapChanged(handler: (keymap: KeymapView) => void) {
  return listen<KeymapView>("keymap-changed", (event) => handler(event.payload));
}

export interface DragDrop {
  over: boolean;
  paths: string[];
  point?: { x: number; y: number };
}

export function onDragDrop(handler: (state: DragDrop) => void) {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "enter" || payload.type === "over") {
      handler({ over: true, paths: [] });
    } else if (payload.type === "drop") {
      const scale = window.devicePixelRatio || 1;
      const point = { x: payload.position.x / scale, y: payload.position.y / scale };
      handler({ over: false, paths: payload.paths, point });
    } else {
      handler({ over: false, paths: [] });
    }
  });
}

export function onCloseRequested(handler: () => Promise<void>) {
  return getCurrentWindow().onCloseRequested(() => handler());
}

export function onSyncStatus(handler: (status: SyncStatus) => void) {
  return listen<SyncStatus>("sync-status", (event) => handler(event.payload));
}

export function onUpdateStatus(handler: (status: UpdateStatus) => void) {
  return listen<UpdateStatus>("update-status", (event) => handler(event.payload));
}

export function parseLink(
  href: string,
): { kind: string; value: string; sub?: string } | null {
  const prefix = "kknow://";
  if (!href.startsWith(prefix)) return null;

  const rest = href.slice(prefix.length);
  const slash = rest.indexOf("/");
  if (slash < 0) return null;

  const kind = rest.slice(0, slash);
  const [value, sub] = rest.slice(slash + 1).split(/#(.*)/s);
  const link: { kind: string; value: string; sub?: string } = {
    kind,
    value: decodeURIComponent(value),
  };
  if (kind === "note" && sub) link.sub = decodeURIComponent(sub);
  return link;
}

export function linkText(link: { kind: string; value: string; sub?: string }): string | null {
  if (link.kind === "note") return link.sub ? `${link.value}#${link.sub}` : link.value;
  if (link.kind === "heading") return `#${link.value}`;
  if (link.kind === "missing") return link.value;
  return null;
}
