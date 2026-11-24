import type { Channel } from "@tauri-apps/api/core";

export type AppSettings = { locale: string, theme: string, noteLabel: NoteLabel, sidebarDensity: SidebarDensity, viewMode: ViewMode, openLastNote: boolean, lineNumbers: boolean, noteScale: number, autoUpdate: boolean, gitPath?: string | null, };

export type BacklinkLineView = { id: string, title: string, line: number, before: string, text: string, after: string, };

export type Base = "light" | "dark";

export type BookmarkMark = { line: number, name?: string, };

export type BookmarkView = { note: string, title: string, folder: string | null, line: number, text: string, name?: string, };

export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflicted";

export type CommandCwd = "vault" | "target";

export type CommandList = { commands: Array<CommandView>, problems: Array<CommandProblem>, };

export type CommandManifest = { path: string, commands: Array<CommandSpec>, env: { [key in string]: string }, problems: Array<CommandProblem>, kinds: Array<string>, broken: string | null, };

export type CommandProblem = { "code": "file", path: string, detail: string, } | { "code": "noId" } | { "code": "duplicateId", id: string, } | { "code": "noName", id: string, } | { "code": "emptyScript", id: string, } | { "code": "unknownKind", id: string, kind: string, known: string, } | { "code": "noTargets", id: string, };

export type CommandSpec = { id: string, name: string, kind: string, script: string, targets: Array<TargetKind>, vaults: Array<string>, match: Array<string>, requires: Array<string>, login: boolean, program?: string | null, cwd: CommandCwd, timeoutSeconds: number, confirm: boolean, env?: { [key in string]: string }, };

export type CommandView = { id: string, name: string, kind: string, confirm: boolean, };

export type ConflictPolicy = "keepBoth" | "preferLocal" | "preferRemote";

export type EditState = "pending" | "accepted" | "rejected";

export type EncryptionOn = { recoveryCode: string, settings: SyncSettings, };

export type ErrorView = { code: string, params: { [key in string]: string }, message: string, };

export type FileEntry = { path: string, name: string, folder: string, };

export type FileSaveOutcome = { "status": "saved", note: FileView, } | { "status": "conflict", note: FileView, copy: string, };

export type FileView = { id: string, title: string, content: string, html: string, backlinks: Array<LinkRef>, outgoing: Array<LinkRef>, tags: Array<string>, file: boolean, };

export type GitAuth = "system" | "token" | "env";

export type GitChange = { path: string, from?: string | null, staged?: ChangeKind | null, unstaged?: ChangeKind | null, };

export type GitSettings = { url: string, auth: GitAuth, env?: string | null, };

export type GitStatus = { branch?: string | null, ahead: number, behind: number, changes: Array<GitChange>, };

export type GlobalResult = { vault: string, vaultName: string, color: string, id: string, title: string, folder: string | null, snippet: string, };

export type HeadingView = { level: number, text: string, line: number, };

export type KeyProblem = { "code": "file", path: string, detail: string, } | { "code": "notAList", action: string, } | { "code": "notAString", action: string, } | { "code": "noKey", action: string, key: string, } | { "code": "unknownModifier", action: string, key: string, modifier: string, } | { "code": "repeatedModifier", action: string, key: string, modifier: string, } | { "code": "unknownKey", action: string, key: string, name: string, };

export type Keymap = { [key in string]: Array<string> };

export type KeymapView = { keys: Keymap, file: string | null, problems: Array<KeyProblem>, };

export type LinkPreview = { id: string, title: string, html: string, line: number | null, partMissing: boolean, locked: boolean, };

export type LinkRef = { id: string, title: string, label: string | null, };

export type LinkTargetView = { id: string, line: number | null, headings: Array<HeadingView>, blocks: Array<string>, };

export type LockStateView = { password: boolean, unlocked: boolean, };

export type MarkdownAssociation = { supported: boolean, current: string | null, ours: boolean, };

export type McpStatus = { enabled: boolean, port: number, running: boolean, error: string | null, token: string, };

export type MentionView = { id: string, title: string, line: number, before: string, text: string, after: string, start: number, end: number, };

export type NoteLabel = "title" | "file";

export type NoteSummary = { id: string, title: string, folder: string | null, tags: Array<string>, locked: boolean, };

export type NoteView = { id: string, title: string, content: string, html: string, backlinks: Array<LinkRef>, outgoing: Array<LinkRef>, tags: Array<string>, bookmarks: Array<BookmarkMark>, threads: Array<Thread>, proposals: Array<ProposalView>, locked: boolean, closed: boolean, };

export type Opened = { "kind": "note", vault: string, id: string, } | { "kind": "file", path: string, };

export type ProposalEdit = { old: string, new: string, state: EditState, };

export type ProposalStatus = "pending" | "accepted" | "rejected" | "partial";

export type ProposalView = { status: ProposalStatus, stale: Array<boolean>, id: string, note: string, reason: string, author?: string, comment?: string, create: boolean, created: number, updated: number, edits: Array<ProposalEdit>, feedback?: string, };

export type Provider = { "kind": "s3" } & S3Settings | { "kind": "folder", path: string, } | { "kind": "git" } & GitSettings;

export type RemoteConfig = { provider: Provider, policy: ConflictPolicy, autoSyncMinutes: number, encrypted?: boolean, };

export type RenamedNote = { id: string, relinked: number, };

export type Review = { accept: Array<number>, reject: Array<number>, feedback?: string, };

export type RunEnd = "exited" | "timedOut" | "stopped";

export type RunLine = { stream: RunStream, text: string, };

export type RunOutcome = { command: string, code: number | null, stdout: string, stderr: string, end: RunEnd, truncated: boolean, millis: number, };

export type RunStream = "stdout" | "stderr";

export type S3Settings = { bucket: string, endpoint?: string | null, region?: string | null, prefix?: string | null, accessKeyId?: string | null, };

export type SaveOutcome = { "status": "saved", note: NoteView, } | { "status": "conflict", note: NoteView, copy: string, };

export type SearchResult = { id: string, title: string, folder: string | null, snippet: string, };

export type SidebarDensity = "compact" | "normal" | "comfortable";

export type SyncConflict = { id: string, copy: string, };

export type SyncReport = { uploaded: number, downloaded: number, removedLocal: number, removedRemote: number, pushed: number, conflicts: Array<SyncConflict>, };

export type SyncSettings = { location: string, hasSecret: boolean, needsSecret: boolean, hasKey: boolean, provider: Provider, policy: ConflictPolicy, autoSyncMinutes: number, encrypted?: boolean, };

export type SyncState = "off" | "idle" | "syncing" | "conflict" | "error";

export type SyncStatus = { vault: string, state: SyncState, done: number, total: number, at: number | null, message: ErrorView | null, conflicts: number, location: string | null, autoSyncMinutes: number, };

export type TargetKind = "note" | "folder" | "vault";

export type TaskView = { note: string, title: string, folder: string | null, line: number, text: string, done: boolean, };

export type Theme = { id: string, name: string, author: string | null, base: Base, source: ThemeSource, shadowsGlobal: boolean, variables: { [key in string]: string }, warnings: Array<ThemeWarning>, };

export type ThemeProblem = { "code": "file", path: string, detail: string, } | { "code": "noName", path: string, };

export type ThemeSet = { themes: Array<Theme>, problems: Array<ThemeProblem>, };

export type ThemeSource = "vault" | "global" | "builtIn";

export type ThemeWarning = { "code": "unknownVariable", variable: string, } | { "code": "emptyValue", variable: string, } | { "code": "tooLong", variable: string, max: number, } | { "code": "badCharacter", variable: string, character: string, } | { "code": "function", variable: string, function: string, };

export type Thread = { id: string, note: string, anchor: string, task: string, status: ThreadStatus, messages: Array<ThreadMessage>, proposal?: string, created: number, updated: number, };

export type ThreadEvent = { "code": "proposed", proposal: string, reason: string, } | { "code": "rejected", proposal: string, feedback: string, } | { "code": "accepted", proposal: string, partly: boolean, feedback: string, };

export type ThreadMessage = { from: string, text: string, at: number, event?: ThreadEvent, };

export type ThreadStatus = "open" | "inProgress" | "question" | "answered" | "review" | "done";

export type TrashEntryView = { id: string, path: string, deleted: number, notes: Array<string>, };

export type UnlockedBy = "password" | "recoveryCode";

export type UpdateStatus = { "state": "unavailable" } | { "state": "managed", manager: string, } | { "state": "idle" } | { "state": "checking" } | { "state": "downloading", version: string, } | { "state": "ready", version: string, } | { "state": "failed", message: string, };

export type VaultChanged = { vault: string, updated: Array<string>, removed: Array<string>, };

export type VaultInfo = { id: string, name: string, root: string, color: string, noteCount: number, };

export type VaultStats = { notes: number, folders: number, bytes: number, };

export type VaultView = { id: string, name: string, path: string, color: string, labels: Array<string>, active: boolean, available: boolean, git: boolean, };

export type ViewMode = "edit" | "split" | "read";

export const ATTACHMENT_NAME_HEADER = "kknow-name";
export const ATTACHMENT_FILE_HEADER = "kknow-file";

export type Commands = {
  activate_vault: { args: { id: string }; result: VaultInfo };
  add_vault: { args: { path: string }; result: VaultInfo };
  attach_file: { args: { source: string, file?: string | null }; result: string };
  change_lock_password: { args: { password: string, vault?: string | null }; result: void };
  check_update: { args: Record<never, never>; result: UpdateStatus };
  claim_markdown: { args: Record<never, never>; result: MarkdownAssociation };
  connect_git: { args: { vault?: string | null, url?: string | null, auth: GitAuth, env?: string | null, token?: string | null }; result: SyncSettings };
  create_folder: { args: { name: string, vault?: string | null }; result: void };
  create_note: { args: { name: string, vault?: string | null }; result: NoteView };
  current_vault: { args: Record<never, never>; result: VaultInfo | null };
  delete_folder: { args: { name: string, vault?: string | null }; result: void };
  delete_note: { args: { id: string, vault?: string | null }; result: void };
  delete_trash_entry: { args: { entry: string, vault?: string | null }; result: void };
  dismiss_proposal: { args: { proposal: string, vault?: string | null }; result: void };
  duplicate_note: { args: { id: string, vault?: string | null }; result: string };
  edit_commands: { args: Record<never, never>; result: CommandManifest };
  empty_trash: { args: { vault?: string | null }; result: number };
  file_image: { args: { path: string, target: string }; result: ArrayBuffer };
  forget_file: { args: { path: string }; result: Array<FileEntry> };
  forget_sync_settings: { args: { vault?: string | null }; result: void };
  get_keymap: { args: Record<never, never>; result: KeymapView };
  get_settings: { args: Record<never, never>; result: AppSettings };
  git_commit: { args: { vault?: string | null, message: string }; result: GitStatus };
  git_diff: { args: { vault?: string | null, path: string, from?: string | null, staged: boolean }; result: string };
  git_discard: { args: { vault?: string | null, path: string, from?: string | null, staged: boolean }; result: GitStatus };
  git_locate: { args: Record<never, never>; result: string | null };
  git_stage: { args: { vault?: string | null, paths: Array<string> }; result: GitStatus };
  git_status: { args: { vault?: string | null }; result: GitStatus };
  git_unstage: { args: { vault?: string | null, paths: Array<string> }; result: GitStatus };
  git_version: { args: { path: string }; result: string };
  link_mention: { args: { from: string, start: number, end: number, text: string, target: string, vault?: string | null }; result: Array<LinkRef> };
  link_preview: { args: { from: string, target: string, vault?: string | null }; result: LinkPreview | null };
  link_target: { args: { from: string, target: string, vault?: string | null }; result: LinkTargetView | null };
  list_bookmarks: { args: { vault?: string | null }; result: Array<BookmarkView> };
  list_commands: { args: { kind: string, target?: string | null, vault?: string | null, file?: string | null }; result: CommandList };
  list_files: { args: Record<never, never>; result: Array<FileEntry> };
  list_folders: { args: { vault?: string | null }; result: Array<string> };
  list_notes: { args: { vault?: string | null }; result: Array<NoteSummary> };
  list_tasks: { args: { vault?: string | null }; result: Array<TaskView> };
  list_themes: { args: { vault?: string | null }; result: ThemeSet };
  list_trash: { args: { vault?: string | null }; result: Array<TrashEntryView> };
  list_vaults: { args: Record<never, never>; result: Array<VaultView> };
  load_file: { args: { path: string }; result: FileView };
  load_note: { args: { id: string, vault?: string | null }; result: NoteView };
  lock_all: { args: { vault?: string | null }; result: void };
  lock_note: { args: { id: string, vault?: string | null }; result: NoteView };
  lock_state: { args: { vault?: string | null }; result: LockStateView };
  markdown_association: { args: Record<never, never>; result: MarkdownAssociation };
  mcp_status: { args: Record<never, never>; result: McpStatus };
  note_backlinks: { args: { id: string, vault?: string | null }; result: Array<BacklinkLineView> };
  note_image: { args: { id: string, target: string, vault?: string | null }; result: ArrayBuffer };
  open_file: { args: { path: string }; result: Opened };
  open_note: { args: { id: string, vault?: string | null }; result: void };
  open_settings: { args: { section?: string | null }; result: void };
  preview: { args: { id: string, content: string, vault?: string | null }; result: string };
  preview_file: { args: { path: string, content: string }; result: string };
  reload_vault: { args: { vault?: string | null }; result: VaultInfo };
  remove_lock: { args: { id: string, vault?: string | null }; result: NoteView };
  remove_vault: { args: { id: string }; result: Array<VaultView> };
  rename_folder: { args: { name: string, to: string, vault?: string | null }; result: number };
  rename_note: { args: { id: string, to: string, vault?: string | null }; result: RenamedNote };
  rename_vault: { args: { id: string, name: string }; result: Array<VaultView> };
  reply_thread: { args: { note: string, thread?: string | null, line?: number | null, text?: string | null, message: string, vault?: string | null }; result: NoteView };
  restart_to_update: { args: Record<never, never>; result: void };
  restore_trash: { args: { entry: string, vault?: string | null }; result: Array<string> };
  reveal_commands: { args: Record<never, never>; result: void };
  reveal_file: { args: { path: string }; result: void };
  reveal_folder: { args: { name: string, vault?: string | null }; result: void };
  reveal_keymap: { args: Record<never, never>; result: void };
  reveal_note: { args: { id?: string | null, vault?: string | null }; result: void };
  reveal_themes: { args: Record<never, never>; result: void };
  review_proposal: { args: { proposal: string, review: Review, vault?: string | null }; result: NoteView };
  run_command: { args: { command: string, kind: string, target?: string | null, vault?: string | null, file?: string | null, run: string, lines: Channel<RunLine> }; result: RunOutcome };
  save_attachment: { args: Uint8Array; result: string };
  save_commands: { args: { commands: Array<CommandSpec>, env: Record<string, string> }; result: CommandManifest };
  save_file: { args: { path: string, content: string, base?: string | null }; result: FileSaveOutcome };
  save_note: { args: { id: string, content: string, base?: string | null, bookmarks?: Array<BookmarkMark> | null, vault?: string | null }; result: SaveOutcome };
  save_sync_settings: { args: { settings: RemoteConfig, secret?: string | null, vault?: string | null }; result: SyncSettings };
  search: { args: { query: string, limit?: number | null, vault?: string | null }; result: Array<SearchResult> };
  search_all: { args: { query: string, limit?: number | null, vaults?: Array<string> | null }; result: Array<GlobalResult> };
  set_bookmarks: { args: { id: string, lines: Array<BookmarkMark>, content: string, vault?: string | null }; result: Array<BookmarkMark> };
  set_keymap: { args: { keys: Keymap }; result: KeymapView };
  set_lock_password: { args: { password: string, vault?: string | null }; result: string };
  set_mcp: { args: { enabled: boolean, port: number }; result: McpStatus };
  set_settings: { args: { settings: AppSettings }; result: AppSettings };
  set_task: { args: { id: string, line: number, done: boolean, vault?: string | null }; result: NoteView };
  set_vault_color: { args: { id: string, color: string }; result: Array<VaultView> };
  set_vault_labels: { args: { id: string, labels: Array<string> }; result: Array<VaultView> };
  share_theme: { args: { id: string, vault?: string | null }; result: void };
  stop_command: { args: { run: string }; result: void };
  sync_change_passphrase: { args: { current: string, new: string, vault?: string | null }; result: void };
  sync_encrypt: { args: { passphrase: string, vault?: string | null }; result: EncryptionOn };
  sync_now: { args: { vault?: string | null }; result: SyncReport };
  sync_settings: { args: { vault?: string | null }; result: SyncSettings | null };
  sync_status: { args: { vault?: string | null }; result: SyncStatus };
  sync_statuses: { args: Record<never, never>; result: Array<SyncStatus> };
  sync_unlock: { args: { secret: string, vault?: string | null }; result: SyncSettings };
  take_opened_file: { args: Record<never, never>; result: Opened | null };
  trash_note: { args: { entry: string, note: string, vault?: string | null }; result: string };
  unlinked_mentions: { args: { id: string, vault?: string | null }; result: Array<MentionView> };
  unlock_notes: { args: { secret: string, vault?: string | null }; result: UnlockedBy };
  update_status: { args: Record<never, never>; result: UpdateStatus };
  vault_by_hotkey: { args: { number: number }; result: string | null };
  vault_stats: { args: { vault?: string | null }; result: VaultStats };
};
