import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const resolvePath = (relative: string) =>
  fileURLToPath(new URL(relative, import.meta.url));

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
    rollupOptions: {
      input: {
        main: resolvePath("./index.html"),
        settings: resolvePath("./settings.html"),
      },
    },
  },
}));
