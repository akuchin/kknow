import type {
  AppSettings,
  KeymapView,
  McpStatus,
  ThemeSet,
  UpdateStatus,
} from "../api";
import { keepSettings, session } from "./store";
import { emit, listen } from "./event";
import type { Handlers } from "./handlers";

export const mcp = { enabled: true, port: 7377, token: "0123456789abcdef0123456789abcdef" };

export function mcpView(): McpStatus {
  const busy = new URLSearchParams(location.search).get("mcp") === "busy";
  const running = mcp.enabled && !busy;
  return {
    ...mcp,
    running,
    error: mcp.enabled && busy ? "Address already in use (os error 48)" : null,
  };
}


export const sharedThemes = new Set<string>();

export function keymapView(): KeymapView {
  let keys: Record<string, string[]> = {};
  try {
    keys = JSON.parse(localStorage.getItem("kknow:mock:keymap") ?? "{}");
  } catch {
  }
  return { keys, file: "~/Library/Application Support/dev.kknow.desktop/keybindings.json", problems: [] };
}

void listen<AppSettings>("settings-changed", ({ payload }) => keepSettings(payload));

export const update: UpdateStatus = sessionStorage.getItem("kknow:mock:updated")
  ? { state: "idle" }
  : { state: "ready", version: "0.2.0" };


export const settingsHandlers = {
  get_settings: () => {
    return session.appSettings;
  },

  set_settings: (args) => {
    keepSettings(args.settings as AppSettings);
    emit("settings-changed", session.appSettings);
    return session.appSettings;
  },

  get_keymap: () => {
    return keymapView();
  },

  set_keymap: (args) => {
    localStorage.setItem("kknow:mock:keymap", JSON.stringify(args.keys ?? {}));
    return keymapView();
  },

  reveal_keymap: () => {
    console.info("reveal_keymap: the browser has no file manager");
    return;
  },

  open_settings: (args) => {
    const section = (args.section) ?? "general";
    window.open(`/settings.html#${section}`, "kknow-settings");
    return;
  },

  list_themes: (): ThemeSet => {
    return {
      themes: [
        {
          id: "sepia",
          name: "Sepia",
          author: "kknow",
          base: "light",
          source: "vault",
          shadowsGlobal: sharedThemes.has("sepia"),
          variables: {
            bg: "#f6efe0",
            "bg-soft": "#efe6d3",
            "bg-raised": "#fbf6ea",
            border: "#ddd0b4",
            text: "#3f3527",
            "text-muted": "#8a7c62",
            accent: "#9c5b28",
            "accent-soft": "#f0e0c9",
          },
          warnings: [],
        },
        {
          id: "gruvbox-light",
          name: "Gruvbox Light",
          author: "kknow",
          base: "light",
          source: "builtIn",
          shadowsGlobal: false,
          variables: {
            bg: "#fbf1c7",
            "bg-soft": "#f2e5bc",
            "bg-raised": "#f9f5d7",
            border: "#d5c4a1",
            text: "#3c3836",
            "text-muted": "#7c6f64",
            heading: "#b57614",
            accent: "#af3a03",
            "accent-soft": "#f2ddb0",
            danger: "#9d0006",
            selection: "#d5c4a1",
            "code-keyword": "#9d0006",
            "code-string": "#79740e",
            "code-comment": "#928374",
            "code-number": "#8f3f71",
            "code-type": "#b57614",
            "code-function": "#427b58",
          },
          warnings: [],
        },
      ],
      problems: [],
    };
  },

  share_theme: (args) => {
    sharedThemes.add(args.id);
    return;
  },

  mcp_status: () => {
    return mcpView();
  },

  set_mcp: (args) => {
    mcp.enabled = args.enabled;
    mcp.port = args.port;
    return mcpView();
  },

  reveal_themes: () => {
    console.info("reveal_themes: the browser has no file manager");
    return;
  },

  update_status: () => {
    return update;
  },

  check_update: async () => {
    await new Promise((resolve) => setTimeout(resolve, 600));
    return update;
  },

  restart_to_update: () => {
    sessionStorage.setItem("kknow:mock:updated", "1");
    setTimeout(() => location.reload(), 300);
    return;
  },
} satisfies Handlers;
