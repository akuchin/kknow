# Vaults and the sidebar

A vault is a folder of notes. You can have many vaults, for example "Work",
"Study" and "Personal". The window and the terminal use one list of vaults.

## The workspace sidebar

The sidebar is a workspace, as in VS Code. The tree roots are vaults. Each
vault expands into its own tree of folders and notes, and you see all of them
at the same time.

kknow reads a vault only when you expand it, search in it or sync it. Ten
vaults in the list cost nothing.

- Folders come before notes. Names sort by the alphabet of the window language.
- A long name stays on one line. The tooltip shows the full name and path.
- kknow remembers the collapsed folders, the recent notes and the last note of
  each vault. <kbd>⌘2</kbd> returns you to the exact place in that vault.

## Add a vault

- Drag one or more folders into the window.
- Click the folder button in the sidebar header.
- Run `open -a kknow ~/notes`, or use **Open With** in Finder.
- Run `kknow vaults add ~/notes --name Personal`.

If the folder is already in the list, kknow only switches to it.

## Switch between vaults

- Click a note of that vault in the sidebar.
- Press <kbd>⌘1</kbd>…<kbd>⌘9</kbd>. The number is the position in the list.
- Click the active vault in the status bar.

The **active** vault owns the theme, the sync status and the status bar.

## Vault actions

The **⋯** button on a tree root (or a right-click) has these actions:

- make the vault active;
- rename it (the files do not change);
- reveal it in Finder;
- change the label color;
- remove it from the list. The files stay on disk.

## Labels

A vault can have text labels, for example `work` or `study`. Set them in
**Settings → Vaults → Labels**, or in the terminal:

```bash
kknow vaults label Personal home ai
```

Labels do two things:

- **Filter.** The row of labels above the tree is a filter. Click `#work`, and
  the tree and ⌘K show only the vaults with this label. The active vault always
  stays.
- **Group for agents.** `kknow mcp --label work` serves all vaults with the
  label. See [Agents](/features/agents).

`#Work` and `work` are the same label.

## Unavailable vaults

If you move a vault folder or unmount a disk, the vault stays in the list with
a mark. It does not open until the folder comes back.

## Where the settings of a vault are

Each vault keeps its sync configuration, bookmarks, themes and trash in the
hidden `.kknow/` folder inside the vault. The vault list itself is in the app
config directory. See [Files and folders](/reference/files).
