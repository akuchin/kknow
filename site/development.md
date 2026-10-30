# Development

kknow is open source under the MIT License. This page tells how to build it and
run the tests.

## Requirements

- [Rust](https://rustup.rs) (stable);
- [Node.js](https://nodejs.org) 24 and [pnpm](https://pnpm.io);
- the [Tauri 2 system dependencies](https://tauri.app/start/prerequisites/)
  for your platform;
- optional: Docker, for the sync tests against MinIO.

## Get the code

```bash
git clone https://github.com/akuchin/kknow.git
cd kknow
pnpm install
```

`ui` and `site` are pnpm workspace packages, so one install covers all of
them. Run all commands from the repository root.

## Run

```bash
pnpm dev          # the app with hot reload
pnpm ui:mock      # only the interface, in a browser, with stubs
cargo run -p kknow-cli -- --vault testdata/vault stats
```

`pnpm ui:mock` needs no Rust build. Stubs replace the core commands. The
settings window is at `/settings.html`.

## Build

```bash
pnpm build:app      # kknow.app in target/release/bundle/macos/
pnpm install:app    # build and install to /Applications
```

`open -a kknow` and **Open With** work only with the installed app. macOS
needs the bundle to know that kknow opens Markdown.

## Test

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm ui:test        # UI unit tests
pnpm ui:e2e         # smoke tests in Chromium on the stubs
```

The sync tests against a real S3 have the `#[ignore]` mark. They need MinIO:

```bash
docker compose up -d
cargo test -p kknow-sync --test minio -- --ignored
```

## This site

The documentation is a [VitePress](https://vitepress.dev) site in `site/`.

```bash
pnpm docs:dev       # http://localhost:5173/
pnpm docs:build
```

A push to `main` that changes `site/` publishes the site to GitHub Pages.

## Make a release

The git tag is the version:

```bash
git tag -m "kknow v0.6.0" v0.6.0
git push origin v0.6.0
```

GitHub Actions builds the app and the CLI for all platforms, signs the update
archives and publishes the release. Then it updates the Homebrew cask and the
Scoop manifests. The README in the repository lists the secrets that the
workflow needs.

## Read next

- [ARCHITECTURE.md](https://github.com/akuchin/kknow/blob/main/ARCHITECTURE.md)
  — the design and the reasons for each decision.
- [TODO.md](https://github.com/akuchin/kknow/blob/main/TODO.md) — the open
  work.
