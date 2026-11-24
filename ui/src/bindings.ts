export type AppSettings = { locale: string, theme: string, noteLabel: NoteLabel, viewMode: ViewMode, openLastNote: boolean, lineNumbers: boolean, noteScale: number, autoUpdate: boolean, gitPath?: string | null, };

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

export type RunLine = { stream: RunStream, text: string, };

export type RunOutcome = { command: string, code: number | null, stdout: string, stderr: string, timedOut: boolean, stopped: boolean, truncated: boolean, millis: number, };

export type RunStream = "stdout" | "stderr";

export type S3Settings = { bucket: string, endpoint?: string | null, region?: string | null, prefix?: string | null, accessKeyId?: string | null, };

export type SaveOutcome = { "status": "saved", note: NoteView, } | { "status": "conflict", note: NoteView, copy: string, };

export type SearchResult = { id: string, title: string, folder: string | null, snippet: string, };

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
