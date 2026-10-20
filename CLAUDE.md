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
- Do not start the app window if the user does not ask for it. The window opens
  on the desktop of the user.
