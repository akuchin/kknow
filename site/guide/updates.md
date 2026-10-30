# Updates

The app updates itself. You do not need to download new versions by hand.

## How it works

1. The app checks for a new version 15 seconds after the start, and then each
   6 hours.
2. If a new version exists, the app downloads it in the background.
3. The status bar shows **Restart to update**. Click it. kknow saves the note
   and starts the new version.

If you quit without the button, the new version installs on quit. On Windows,
the update installs only from the button.

The app menu also has **Check for Updates…**.

## Privacy and safety

The check is one GET request to the `latest.json` file of the latest GitHub
release. The app sends no data about you or your notes.

Each update archive has a signature. The app does not install an archive with
a bad signature.

## Turn off automatic updates

Turn off **Settings → General → Update automatically**. The **Check now**
button still works.

## Package managers

**Homebrew.** The cask has `auto_updates`, so `brew upgrade` skips kknow. To
update through Homebrew, run:

```bash
brew upgrade --cask --greedy kknow
```

**Scoop.** The MSI updater installs a second copy into `Program Files`. Thus,
with Scoop, turn off **Update automatically**, and update only through Scoop:

```powershell
scoop update kknow kknow-cli
```

## Developer builds

A local build never checks for updates. `pnpm dev` and `pnpm install:app` give
version 0.0.0.
