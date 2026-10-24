# Window code refactor: plan

This plan splits `ui/src/main.ts` (4160 lines) and `ui/src/settings/main.ts`
(2138 lines) into modules. Each module owns its state and has an explicit API.
The behaviour of the app must not change. The work rules are in
[CLAUDE.md](../CLAUDE.md), and the design is in
[ARCHITECTURE.md](../ARCHITECTURE.md).

## Why

- `main.ts` has about 180 functions and 19 module variables that change
  (`current`, `vault`, `vaults`, `files`, `mode`, `appSettings`, `syncing`,
  `trashViewing`, `noteCommands`, and others). Any handler can change any of
  them. This is the main source of future bugs.
- Almost all functions read `current` and `vault`. Only a few functions write
  them:
  - `current`: `applyNote`, `closeNote`, `useVault`, `forgetVault`;
  - `vaults`: `refreshVaults`, `renameVault`, `recolorVault`, `forgetVault`;
  - `files`: `refreshFiles`, `forgetFile`;
  - `appSettings`: `applySettings`, `saveSettings`, `boot`.

  So one store with setters and events is enough. The readers stay simple.
- `session.ts` and `navigation.ts` already show the pattern (commit
  `0e0fe2d`). This plan applies it to the whole window.

## Decisions

1. **One store, not signals.** ARCHITECTURE.md says that the frontend has no
   reactivity layer on purpose. A store with plain events (`on("note", …)`)
   keeps that rule. A signals library would be a new concept for all of the
   window code.
2. **No framework.** The decision "TypeScript without a framework" stays.
3. **Behaviour does not change.** A commit moves code or changes its
   structure. Each bug that we find goes into a separate commit with its own
   test.
4. **Safety net first.** The window has no DOM tests now. Only pure modules
   have tests (`node --test`). Before we move code, we add smoke tests that run
   in a browser against `pnpm ui:mock`.
5. **Dependency direction.** The modules make layers. A module imports only
   from the layers below it. A lower layer learns about a change through
   store events, not through a call to a higher module. A test checks that
   the imports have no cycles.

## Target layout

All main window code goes into `ui/src/window/`. `ui/src/main.ts` becomes
the entry point only.

| Layer | Module | What it owns | Lines now (approx.) |
|---|---|---|---|
| 0 | `window/state.ts` | the store: open note, active vault, vault list, files, mode, settings, sync statuses; setters and events | new |
| 0 | `window/dom.ts` | the markup of the window and `el` | 88–345 |
| 1 | `window/status.ts` | `setStatus`, `fail`, the status bar, the update button | 507–545 |
| 1 | `tree-model.ts` (pure) | `foldersOf`, `descend`, `buildTree`, `sortFolder`, `fileName` | 1006–1110 |
| 1 | `sync-text.ts` (pure) | `timeAgo`, `shortLocation`, `describeReport`, `describeRemote` | 3184–3322 |
| 2 | `window/note.ts` | open, show and close a note: `applyNote`, `closeNote`, `saveNow`, `syncWithDisk`, the preview, the heading, positions, scale, backlinks, mode | 546–930, 1702–1726 |
| 2 | `window/notes-float.ts` | the agent notes float and the two `PreviewNotes` | 391–505 |
| 3 | `window/tree.ts` | the sidebar tree: render, collapsed and expanded folders, rows, `reveal`, `markActive` | 931–1700 |
| 3 | `window/trash.ts` | trash nodes, the read-only view, restore, delete, empty | 1182–1450, 3113–3150 |
| 3 | `window/files.ts` | standalone files, drag and drop, attachments | 2816–3046 |
| 3 | `window/marks.ts` | bookmarks, tasks, headings, recent notes | 1727–1880 |
| 4 | `window/edits.ts` | create, rename, move, duplicate and delete notes and folders; inline rename | 2374–2630, 3047–3112 |
| 4 | `window/vaults.ts` | the vault menu, rename, colour, forget, switch, adopt, ⌘1…⌘9 | 1922–1956, 2279–2812, 4022–4031 |
| 4 | `window/commands.ts` | command menus, runs, run output, note commands | 1957–2278, 3490–3620 |
| 4 | `window/sync.ts` | the sync panel, the chip, statuses, conflicts | 3151–3394 |
| 4 | `window/appearance.ts` | app settings, themes, language, line numbers | 3395–3462, 3810–3930 |
| 5 | `window/toolbar.ts` | the note menu, the bookmarks, run and backlinks menus, the new note form | 3463–3750 |
| 5 | `window/palette.ts` | palette items | 3753–3809 |
| 5 | `window/keys.ts` | window actions, the keydown handler, the keymap | 3931–4031 |
| 6 | `main.ts` | `boot` and the Tauri event subscriptions | 4032–4160 |

The settings window gets the same treatment in `ui/src/settings/`:

| Module | What it owns | Lines now (approx.) |
|---|---|---|
| `settings/controls.ts` | `row`, `select`, `toggle`, `field`, `chips`, `scaleControl`, `heading` | 122–325 |
| `settings/general.ts` | the General section, updates, the `.md` association | 352–510 |
| `settings/appearance.ts` | the Appearance section, themes | 511–630 |
| `settings/keys.ts` | the Keys section, key recording | 631–936 |
| `settings/editor.ts` | the Editor section | 937–987 |
| `settings/commands.ts` | the command list and the command form | 988–1596 |
| `settings/vaults.ts` | vaults, vault details and stats | 1597–1941 |
| `settings/sync-form.ts` | the sync form: provider, S3, folder | 1942–2079 |
| `settings/main.ts` | sections, `render`, `boot` | the rest |

## The store

```ts
// window/state.ts: the sketch, not the final API
type Events = {
  note: NoteView | null;      // the open note changed
  vault: VaultInfo | null;    // the active vault changed
  vaults: VaultView[];
  files: FileEntry[];
  mode: ViewMode;
  settings: AppSettings;
  sync: Map<string, SyncStatus>;
};

export const state = {
  get note(): NoteView | null,
  get vault(): VaultInfo | null,
  // …
  setNote(note: NoteView | null): void,   // emits "note"
  on<K extends keyof Events>(event: K, handler: (value: Events[K]) => void): () => void,
};
```

- The store has no logic of its own. It keeps values and sends events.
- Each value has one owner module. Only the owner calls its setter: `note.ts`
  calls `setNote`, and `vaults.ts` calls `setVaults`. A test or a lint rule
  can check this later. For now, a code review checks it.
- State that only one zone uses stays private in that zone. Examples:
  `trashViewing` in `trash.ts`, `noteCommands` in `commands.ts`, `themes` in
  `appearance.ts`.

## Phases

Each step is one commit. After each step, all of these must pass:
`pnpm ui:test`, `npx tsc --noEmit`, `pnpm ui:build`, the smoke tests, and
`cargo test --workspace` (for `bindings.ts`).

### Phase 0. Safety net

1. Add Playwright as a dev dependency of `ui`. It starts the Vite mock server
   and runs Chromium. WebKit is optional: the memory notes say that Chrome
   does not catch WebKit bugs. A WebKit run is useful for the window, but it
   is slower.
2. Write smoke tests for the main paths:
   - open a note; type; the status shows "saved"; the text stays after a
     reload of the note;
   - back and forward;
   - create, rename and delete a note; restore it from the trash;
   - switch the vault from the menu and with ⌘2;
   - search in the palette;
   - sync from the status bar (the mock), and the conflict chip;
   - open a standalone file;
   - change the mode and the font size;
   - in the settings window: each section opens; a theme change reaches the
     main window.
3. Add `pnpm ui:e2e` and a CI step.

Phase 0 is the most important one. Without it, a refactor of 4000 lines of
DOM code depends on manual checks.

### Phase 1. Moves with no risk

1. `window/dom.ts`: the markup and `el`.
2. `window/status.ts`.
3. Pure modules with unit tests: `tree-model.ts`, `sync-text.ts`.

### Phase 2. The store

1. Add `window/state.ts`.
2. Move the six shared values into it: `current`, `vault`, `vaults`,
   `files`, `mode`, `appSettings`. Replace each write with the setter of its
   owner. The readers change from `current` to `state.note`.
3. Replace the direct calls "after the note changed" with subscriptions where
   the call goes up a layer. Examples: `applyNote` calls `renderTree`,
   `markActive` and `refreshNoteCommands`; after this step, `tree.ts` and
   `commands.ts` subscribe to `note`.

### Phase 3. Zones, one commit each

The order goes from few links to many links:

1. `notes-float.ts`
2. `appearance.ts`
3. `sync.ts`
4. `commands.ts`
5. `files.ts`
6. `marks.ts`
7. `trash.ts`
8. `tree.ts`
9. `edits.ts`
10. `vaults.ts`
11. `note.ts` (it uses `session.ts` and `navigation.ts`)
12. `toolbar.ts`, `palette.ts`, `keys.ts`
13. `main.ts` keeps only `boot`.

### Phase 4. The settings window

The same steps for `settings/main.ts`: first `controls.ts`, then one section
at a time. This window has only three shared values (`settings`, `vaults`,
`statuses`), so it needs no store. A small object in `settings/main.ts` is
enough.

Phase 4 does not touch the files of phases 1–3. It can run in parallel, in
its own branch, after phase 0.

### Phase 5. Guards and documents

1. A test that finds import cycles in `ui/src` and fails on them.
2. A test that fails on a `ui/src` file with more than 800 lines. Generated
   `bindings.ts` and existing large widgets (`table-widget.ts`) are on an
   allow list.
3. A test that `main.ts` has no module-level `let`.
4. ARCHITECTURE.md: a module map of the window, the store rules and the
   layer rule. Remove the note "state leaves `main.ts` one zone at a time".

## Done when

- `ui/src/main.ts` has less than 200 lines and no module-level `let`.
- `ui/src/settings/main.ts` has less than 300 lines.
- Each new module has less than 800 lines and one clear owner of its state.
- No import cycles.
- The smoke tests, the unit tests, `tsc`, the build and the Rust tests pass.
- A manual check in the real window (`pnpm dev`), with the user present:
  WebKit can differ from Chromium.

## Risks

- **Order of top-level code.** Now `main.ts` runs its top-level code in file
  order: it builds the markup, creates `Editor` and `StatusBar`, and assigns
  `onclick`. Modules run on import, in import order. Rule: a module does not
  touch the DOM on import. It exports `init()`, and `boot` calls the `init`
  functions in a fixed order.
- **Circular calls.** A lower layer that must call a higher one (for example,
  the tree opens a note) gets the call as a parameter of `init`, or it goes
  through the store. It does not import the higher module.
- **Async races.** Code such as `saveNow` checks `current?.id !== note.id`
  after an `await`. With the store, this becomes `state.note?.id`. The meaning
  must not change: the value is read after the `await`, not before it.
- **Size of the work.** About 6000 lines move. Small commits and the smoke
  tests keep each step reviewable.

## Estimate

| Phase | Effort |
|---|---|
| 0. Smoke tests | 1 session |
| 1. Moves with no risk | 0.5 session |
| 2. Store | 1 session |
| 3. Zones | 2–3 sessions |
| 4. Settings window | 1 session, in parallel with 1–3 |
| 5. Guards and documents | 0.5 session |
