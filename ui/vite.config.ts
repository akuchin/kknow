import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const resolvePath = (relative: string) =>
  fileURLToPath(new URL(relative, import.meta.url));

// `--mode mock` replaces the Tauri bridges with mocks, so the UI opens in a
// plain browser. This is faster than starting the whole app. KKNOW_MOCK=1 does
// the same for a caller that cannot pass the flag, as `playwright.config.ts`.
// The mode is the way `pnpm ui:mock` asks for it: a variable in front of a
// command is POSIX syntax, and the script must also run on Windows.
function mockAliases(mode: string): Record<string, string> {
  if (mode !== "mock" && process.env.KKNOW_MOCK !== "1") return {};
  return {
    "@tauri-apps/api/app": resolvePath("./src/mock/app.ts"),
    "@tauri-apps/api/core": resolvePath("./src/mock/core.ts"),
    "@tauri-apps/api/event": resolvePath("./src/mock/event.ts"),
    "@tauri-apps/api/webview": resolvePath("./src/mock/webview.ts"),
    "@tauri-apps/api/window": resolvePath("./src/mock/window.ts"),
    "@tauri-apps/plugin-dialog": resolvePath("./src/mock/dialog.ts"),
  };
}

// The port is fixed: Tauri expects the same one in devUrl.
export default defineConfig(({ mode }) => ({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  resolve: {
    alias: mockAliases(mode),
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    // There are two windows, each with its own page: main and settings.
    // Settings is a separate window, not a panel in the main one, so you can
    // see it next to what you change.
    rollupOptions: {
      input: {
        main: resolvePath("./index.html"),
        settings: resolvePath("./settings.html"),
      },
    },
  },
}));
