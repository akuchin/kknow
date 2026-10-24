/**
 * The store of the main window: the values that many zones read.
 *
 * The store has no logic of its own. It keeps the values and tells the
 * subscribers about each change: a setter writes the value and then calls the
 * handlers of its event, in the order they subscribed. A setter emits even if
 * the value is the same.
 *
 * Rules (see `docs/refactor-ui-2026.md`, "The store"):
 * - Each value has one owner. Only the owner calls its setter.
 * - A lower layer learns about a change through an event, not through a call
 *   to a higher module.
 * - Read `state.x` when you need it. After an `await`, read it again: another
 *   note or vault can open during the call.
 *
 * There is no reactivity library on purpose: ARCHITECTURE.md keeps the
 * frontend plain. The module has only type imports, so `node --test` loads it.
 */

import type { AppSettings, FileEntry, NoteView, VaultInfo, VaultView, ViewMode } from "../api";

/** The values of the store. Each key is also the name of its event. */
export type StateValues = {
  /** The open note or separate file; `null` when nothing is open. */
  note: NoteView | null;
  /** The active vault: the tree, the editor and the status bar work with it. */
  vault: VaultInfo | null;
  /** All vaults. This list is also the switcher menu. */
  vaults: VaultView[];
  /** Separate files in open order. The core keeps this list. */
  files: FileEntry[];
  /**
   * Mode of the open note: "Live preview" (markup in the text, no preview),
   * "Side by side" (markdown and preview) or "Read" (full-width preview only).
   */
  mode: ViewMode;
  /** App settings: shared with the settings window and stored in the core. */
  settings: AppSettings | null;
};

export type StateEvent = keyof StateValues;

export type Handler<K extends StateEvent> = (value: StateValues[K]) => void;

/** Makes a store. The window uses the one `state`; tests make their own. */
export function createState() {
  const values: StateValues = {
    note: null,
    vault: null,
    vaults: [],
    files: [],
    mode: "split",
    settings: null,
  };
  const handlers: { [K in StateEvent]: Set<Handler<K>> } = {
    note: new Set(),
    vault: new Set(),
    vaults: new Set(),
    files: new Set(),
    mode: new Set(),
    settings: new Set(),
  };

  function set<K extends StateEvent>(key: K, value: StateValues[K]): void {
    values[key] = value;
    // A copy: a handler can unsubscribe while the event goes on.
    const listeners: Set<Handler<K>> = handlers[key];
    for (const handler of [...listeners]) handler(value);
  }

  return {
    get note(): NoteView | null {
      return values.note;
    },
    get vault(): VaultInfo | null {
      return values.vault;
    },
    get vaults(): VaultView[] {
      return values.vaults;
    },
    get files(): FileEntry[] {
      return values.files;
    },
    get mode(): ViewMode {
      return values.mode;
    },
    get settings(): AppSettings | null {
      return values.settings;
    },

    setNote(note: NoteView | null): void {
      set("note", note);
    },
    setVault(vault: VaultInfo | null): void {
      set("vault", vault);
    },
    setVaults(vaults: VaultView[]): void {
      set("vaults", vaults);
    },
    setFiles(files: FileEntry[]): void {
      set("files", files);
    },
    setMode(mode: ViewMode): void {
      set("mode", mode);
    },
    setSettings(settings: AppSettings | null): void {
      set("settings", settings);
    },

    /**
     * Calls `handler` after each change of the value, with the new value.
     * Returns the function that unsubscribes. The same handler twice is one
     * subscription.
     */
    on<K extends StateEvent>(event: K, handler: Handler<K>): () => void {
      const listeners: Set<Handler<K>> = handlers[event];
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },
  };
}

export type State = ReturnType<typeof createState>;

/** The store of the main window. */
export const state = createState();
