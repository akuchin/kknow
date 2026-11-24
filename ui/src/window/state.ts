import type { AppSettings, FileEntry, NoteView, VaultInfo, VaultView, ViewMode } from "../api";

export type StateValues = {
  note: NoteView | null;
  vault: VaultInfo | null;
  vaults: VaultView[];
  files: FileEntry[];
  mode: ViewMode;
  settings: AppSettings | null;
};

export type StateEvent = keyof StateValues;

export type Handler<K extends StateEvent> = (value: StateValues[K]) => void;

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

export const state = createState();
