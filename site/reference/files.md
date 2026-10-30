# Files and folders

kknow keeps data in two places: in each vault and in the app config directory.

## In each vault

```text
<vault>/
  *.md                  your notes
  attachments/          pasted and dropped images
  .kknow/
    sync.json           the snapshot of the last sync
    remote.json         the storage address, the conflict policy, the interval
    bookmarks.json      bookmarks
    themes/*.json       themes of this vault
    trash/<entry>/…     deleted notes and folders
```

kknow skips all hidden folders, for example `.git` and `.obsidian`. Sync does
not copy `.kknow/`. Add `.kknow/` to `.gitignore` if you keep the vault in git.

`remote.json` has no secret key. The window keeps the key in the system
keychain.

## App config directory

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/dev.kknow.desktop/` |
| Windows | `%APPDATA%\kknow\` |
| Linux | `$XDG_CONFIG_HOME/kknow/`, or `~/.config/kknow/` |

The window and the CLI share this directory.

| File | Content |
|---|---|
| `vaults.json` | the vault list: names, paths, colors, labels |
| `settings.json` | language, theme, editor settings |
| `keybindings.json` | your changed shortcuts |
| `commands.json` | custom commands |
| `files.json` | standalone files |
| `mcp.json` | MCP server: on or off, the port, the token |
| `themes/*.json` | shared themes |

## Remove kknow completely

1. Remove the app.
2. Remove the app config directory.
3. Optional: remove the `.kknow/` folder from each vault.

Your notes stay as they are.

With Homebrew, `brew uninstall --zap --cask kknow` also removes the config
directory and the caches.
