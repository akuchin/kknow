# Quick start

This page installs kknow and opens your first vault. It takes about two
minutes.

## 1. Install the app

::: code-group

```bash [macOS (Homebrew)]
brew install --cask akuchin/kknow/kknow
```

```powershell [Windows (Scoop)]
scoop bucket add kknow https://github.com/akuchin/scoop-kknow
scoop install kknow/kknow kknow/kknow-cli
```

```bash [Linux]
# Download the .deb, .rpm or .AppImage file from the Releases page:
# https://github.com/akuchin/kknow/releases/latest
sudo apt install ./kknow_*_amd64.deb
```

:::

The builds:

| Platform | App | CLI |
|---|---|---|
| macOS | Apple Silicon only (`aarch64`) | `kknow-cli-…-aarch64-apple-darwin.tar.gz` |
| Windows | MSI installer, or Scoop | `kknow-cli-…-x86_64-pc-windows-msvc.zip`, or Scoop |
| Linux | `.deb`, `.rpm`, `.AppImage` | `kknow-cli-…-x86_64-unknown-linux-gnu.tar.gz` |

All files are on the [Releases](https://github.com/akuchin/kknow/releases) page.
There is no build for Intel Macs.

::: warning The app has no Apple signature
macOS puts a downloaded unsigned app in quarantine. Then Gatekeeper tells you
that the app is "damaged". The Homebrew cask removes the quarantine flag for
you. If you installed the app from the Releases page, move it to
`/Applications` and run this command:

```bash
xattr -dr com.apple.quarantine /Applications/kknow.app
```
:::

## 2. Open a vault

A vault is any folder with `.md` files. You can use a folder that you already
have, for example an Obsidian vault. kknow does not change the files when it
opens them.

Do one of these:

- Drag a folder from Finder or Explorer into the window.
- Click the folder button in the sidebar header and select a folder.
- Run `open -a kknow ~/notes` in the terminal (macOS).

The folder becomes a vault and appears in the sidebar. To add more vaults,
repeat the step. You can switch between them with <kbd>⌘1</kbd>…<kbd>⌘9</kbd>.

## 3. Write a note

1. Press <kbd>⌘N</kbd> to make a new note.
2. Type a heading: `# My first note`.
3. Link to another note: `[[Ideas]]`. Click the link to make that note.
4. Add a tag: `#inbox`.

kknow saves the note 1 second after the last edit. You do not need to press
<kbd>⌘S</kbd>.

Press <kbd>⌘E</kbd> to go through the three note modes: **Edit**, **Side by
side** and **Read**.

## 4. Find it again

Press <kbd>⌘K</kbd> and type a word from the note. The palette searches all
vaults at the same time. Try `tag:inbox` to find all notes with the tag.

## 5. Install the CLI (optional)

The `kknow` program works with the same vaults in the terminal. On Windows,
Scoop installs it (`kknow/kknow-cli`). On macOS and Linux, download the archive
from the Releases page and put the binary on your `PATH`:

```bash
tar -xzf kknow-cli-*-aarch64-apple-darwin.tar.gz
mv kknow ~/.local/bin/
```

Then try it:

```bash
kknow vaults                       # the same list as in the window
kknow --vault Personal stats       # notes, folders and size
kknow --vault Personal search "tag:inbox"
```

See the [CLI reference](/reference/cli) for all commands.

## Next steps

- Connect an AI agent to your notes: [Agents (MCP)](/features/agents).
- Sync between machines: [Sync](/features/sync).
- Learn the keys: [Keyboard shortcuts](/reference/shortcuts).
