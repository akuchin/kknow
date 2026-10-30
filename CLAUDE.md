# Notes for agents

Before you change code, read [ARCHITECTURE.md](ARCHITECTURE.md). It tells how
the project works and why the team made each decision. These reasons explain
much of the code that looks strange. [README.md](README.md) gives the same
information in short form for users. [TODO.md](TODO.md) lists the work that is
still open.

Rules for work in this repository:

- Run all commands from the root: `pnpm dev`, `pnpm build:app`,
  `cargo test --workspace`.
- The core (`crates/*`) must not depend on Tauri. The project has two
  frontends: the window and the CLI (command-line interface).
- Docs, code comments and identifiers are in English. The default UI (user
  interface) language is Russian. The UI also has an English locale in
  `ui/src/i18n.ts`. UI strings, CLI output and test data stay in Russian.
- After each change, run `cargo test --workspace` and
  `cargo clippy --workspace --all-targets -- -D warnings`. Both must show no
  errors and no warnings.
- To check UI behavior, use `pnpm ui:mock`. It runs in a browser and uses stubs
  instead of commands. You do not need a built app.
- After a UI change, also run `pnpm ui:test` and `pnpm ui:e2e`. The second one
  runs the smoke tests in Chromium on the same stubs.
- Do not start the app window if the user does not ask for it. The window opens
  on the desktop of the user.

## Documentation

The documentation site is in `site/` and is live at <https://kknow.kuca.dev/>.
`pnpm docs:dev` starts it locally, and `pnpm docs:build` checks the build.

You must update the site in the same change as an important change in the app:

- a new feature or a new setting;
- a change in a shortcut, a CLI command or a file format;
- a removed feature or a changed default behavior;
- a change in the crates or in the data flow (update `site/architecture.md`).

A small fix, a style change or a refactor does not need a site update. If you
are not sure, update the site.

Where to write:

- a feature: `site/features/<feature>.md`. Add a new page to the sidebar in
  `site/.vitepress/config.ts` and to the list in `site/features/index.md`;
- shortcuts, CLI and files: the pages in `site/reference/`;
- the install steps: `site/guide/quick-start.md`.

Also update `README.md` if the change is in its short feature list. The site is
in English and uses the same style as the other docs.
