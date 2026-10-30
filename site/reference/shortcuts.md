# Keyboard shortcuts

`⌘` is Cmd on macOS. On Windows and Linux, use Ctrl instead.

kknow reads the keys by their position on the keyboard. The shortcuts work the
same in any keyboard layout, for example in Russian.

## Notes

| Keys | Action |
|---|---|
| <kbd>⌘N</kbd> | new note |
| <kbd>⌘O</kbd> | open a Markdown file without a vault |
| <kbd>⌘S</kbd> | save now |
| <kbd>⌘E</kbd> | next note mode: Edit, Side by side, Read |
| <kbd>⌘=</kbd>, <kbd>⌘−</kbd>, <kbd>⌘0</kbd> | font: larger, smaller, default |
| <kbd>⌘⇧M</kbd> | add a comment for the agent |
| <kbd>⌘⇧B</kbd> | set or remove a bookmark on the cursor line |

## Search and navigation

| Keys | Action |
|---|---|
| <kbd>⌘K</kbd>, <kbd>⌘⇧P</kbd> | search in all vaults |
| <kbd>↑</kbd> <kbd>↓</kbd>, <kbd>Enter</kbd> | select and open a result in the palette |
| <kbd>⌘F</kbd> | search in the open note |
| <kbd>⌘G</kbd>, <kbd>⌘⇧G</kbd> | next and previous match |
| <kbd>⌘⇧O</kbd> | go to a heading of the open note |
| <kbd>⌘⌥B</kbd> | bookmarks of the vault |
| <kbd>⌘⌥T</kbd> | open tasks of the vault |
| <kbd>⌘⌥←</kbd>, <kbd>⌘⌥→</kbd> | back and forward through opened notes |
| <kbd>Esc</kbd> | close the palette, the search panel or the changes view |

## Window and vaults

| Keys | Action |
|---|---|
| <kbd>⌘\\</kbd> | hide or show the sidebar |
| <kbd>⌘1</kbd>…<kbd>⌘9</kbd> | switch to a vault by its position in the list |
| <kbd>⌘⇧S</kbd> | sync the active vault |
| <kbd>⌘,</kbd> | settings |

For the keys inside a table, see [Editor → Tables](/features/editor#tables).

## Change a shortcut

1. Open **Settings → Keys**.
2. Click **Change** next to an action.
3. Press the new combination. <kbd>Esc</kbd> cancels.

kknow checks the combination before it saves it:

- A combination needs ⌘, Ctrl or ⌥. F1…F24 need nothing.
- System and text keys (⌘C, ⌘V, ⌘Z, ⌘Q, Enter, Esc and others) and ⌘1…⌘9 do
  not change.
- A combination of an editor command, for example ⌘D or ⌘/, does not fit.
- If another action has the combination, kknow asks you. **Reassign** moves it
  to the new action.

**Reset** gives an action its default keys back. **Reset all** does it for all
actions.

## The file

The changed keys are in `keybindings.json` in the
[app config directory](/reference/files). The file holds only your changes:

```json
{
  "note.new": ["Mod-Shift-n"],
  "format.strike": []
}
```

The key is the action ID. The value is the full list of its keys. `Mod` is ⌘ on
macOS and Ctrl on Windows and Linux. An empty list removes all keys of the
action. kknow reads the file again when the window gets focus.
