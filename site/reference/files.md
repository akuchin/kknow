# Files and folders

kknow keeps data in two places: in each vault and in the app config directory.

## In each vault

```text
<vault>/
  *.md                  your notes
  attachments/          pasted and dropped images
  .kknow/
    sync.json           the snapshot of the last sync
    remote.json         the storage address, the conflict policy, the interval;
                        for git: the address and the sign-in
    bookmarks.json      bookmarks
    themes/*.json       themes of this vault
    trash/<entry>/…     deleted notes and folders
```

kknow skips all hidden folders, for example `.git` and `.obsidian`. S3 and
folder sync do not copy `.kknow/`.

With git sync, kknow adds `.kknow/remote.json`, `.kknow/sync.json` and the
trash to `.git/info/exclude`. The rest of `.kknow/` goes into the repository.

`remote.json` has no secret key or token. The window keeps them in the system
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
| `settings.json` | language, theme, editor settings, the path to git |
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
