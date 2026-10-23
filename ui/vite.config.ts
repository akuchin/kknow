import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// KKNOW_MOCK=1 replaces the Tauri bridges with mocks, so the UI opens in a
// plain browser. This is faster than starting the whole app.
const useMocks = process.env.KKNOW_MOCK === "1";

const resolvePath = (relative: string) =>
  fileURLToPath(new URL(relative, import.meta.url));

// The port is fixed: Tauri expects the same one in devUrl.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  resolve: {
    alias: useMocks
      ? {
          "@tauri-apps/api/app": resolvePath("./src/mock/app.ts"),
          "@tauri-apps/api/core": resolvePath("./src/mock/core.ts"),
          "@tauri-apps/api/event": resolvePath("./src/mock/event.ts"),
          "@tauri-apps/api/webview": resolvePath("./src/mock/webview.ts"),
          "@tauri-apps/plugin-dialog": resolvePath("./src/mock/dialog.ts"),
        }
      : {},
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
});
