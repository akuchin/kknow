# Standalone files

You can open any `.md` file without a vault. For example, open a `README.md`
from a repository or a draft on the desktop.

## Open a file

All of these give the same result:

- press <kbd>⌘O</kbd> and select the file;
- drag a `.md` file into the window (a folder still becomes a vault);
- double-click the file in Finder, use **Open With**, or run
  `open -a kknow README.md`;
- give the file as an argument: `kknow README.md`.

kknow adds the file to the **Files** section at the bottom of the sidebar. The
list stays after a restart. kknow removes a file from the list if the file no
longer exists.

## Limits

If the file is inside a vault from the list, kknow opens it as a note of that
vault, with links and search.

A file outside any vault has no search, backlinks or bookmarks. `[[links]]`
in the preview do not work. kknow saves the file with the same care as a note:
if another program changed it, kknow writes your text next to it as a copy.

## Make kknow the default app for .md

Click **Make kknow the default** in **Settings → General**. The setting also
shows which app opens `.md` files now. In Finder, you can also use
**Get Info → Open with → Change All**.
