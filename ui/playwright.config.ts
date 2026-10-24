import { defineConfig, devices } from "@playwright/test";

/**
 * Smoke tests of the window in a browser, on the mock commands
 * (`KKNOW_MOCK=1`). They check user paths through the DOM, so they must keep
 * passing while the window code moves between modules.
 *
 * The server uses its own port, not 1420: `pnpm ui:mock` or `pnpm dev` can run
 * at the same time, and a test against the real app would change real notes.
 *
 * Chromium is the default. WebKit is the engine of the macOS window, and it is
 * slower: `pnpm ui:e2e:webkit`.
 */
const PORT = 1430;

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "retain-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1400, height: 900 } } },
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1400, height: 900 } } },
  ],
  webServer: {
    command: `vite --port ${PORT} --strictPort`,
    env: { KKNOW_MOCK: "1" },
    url: `http://localhost:${PORT}`,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
