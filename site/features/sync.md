# Sync

kknow syncs a vault without a kknow server. You select the storage:

- **S3-compatible storage**: Amazon S3, MinIO, Cloudflare R2 and others;
- **a plain folder**: iCloud Drive, Dropbox, a network disk or a USB stick;
- **a git repository**: GitHub, GitLab or any git server. You stage and commit
  the changes by hand. See [Git](#git).

Each vault has its own sync configuration. A vault without sync works as usual.

## Configure sync in the window

1. Press <kbd>⌘,</kbd> to open the settings.
2. Go to **Vaults** and select the vault.
3. Select the provider: S3-compatible storage or a folder.
4. Fill in the fields. For S3, the window keeps the secret key in the system
   keychain (Keychain, Credential Manager or Secret Service).
5. Optional: turn on automatic sync and set the interval in minutes.
6. Select the conflict policy.

The status bar shows the sync state: a dot, a short storage address and the
time of the last sync. Click it or press <kbd>⌘⇧S</kbd> to sync now.

## Sync in the terminal

```bash
# A folder, for example iCloud Drive
kknow --vault ~/notes sync --folder ~/Library/Mobile\ Documents/iCloud~kknow

# S3 or MinIO
export KKNOW_S3_BUCKET=kknow KKNOW_S3_ENDPOINT=http://localhost:9000
export KKNOW_S3_ACCESS_KEY=... KKNOW_S3_SECRET_KEY=...
kknow --vault ~/notes sync --dry-run
```

- `--dry-run` prints the plan and changes nothing.
- `--prefix` keeps several vaults in one bucket.
- Without `--bucket` and `--folder`, `kknow sync` uses the vault configuration
  from the window.

::: tip Keys only from the environment
kknow reads access keys only from environment variables. Other users can see
the command line in `ps`, and the shell keeps it in the history. If you do not
set `KKNOW_S3_*`, kknow uses the usual `AWS_*` variables.
:::

## Conflicts

If both devices changed the same note, kknow keeps both versions by default:

- your version stays in place;
- the other version goes next to it as `Note (конфликт 88b6a2d6).md`.

The copy is an ordinary note. It syncs to other devices too, and you can merge
it anywhere. A repeated sync does not make new copies.

To select a side without a copy, set the policy to "local" or "remote", or use
`--on-conflict local|remote`.

An inactive vault also shows its conflicts. A chip
"Archive 2024 · conflict: 2 files" appears in the status bar. Its **Resolve**
item shows the copies.

## Deletes

A note that another device deleted does not disappear here for good. Sync
moves it to the [trash](/features/organize#trash). The delete on the other
device can be a mistake.

## Automatic sync

- Each vault has its own interval.
- After a failure, the pause doubles, to a maximum of eight times the interval.
- A successful sync resets the pause.
- Before the window closes, kknow does a last sync. It waits a maximum of 5
  seconds.

You can save notes during a sync. The network part runs without the vault
lock.

## What sync does not copy

S3 and folder sync copy only notes. The `.kknow/` folder stays on each
machine: bookmarks, the trash and the sync state. Git is different: see
[What goes into the repository](#what-goes-into-the-repository).

## iCloud placeholders

iCloud can replace a file that it did not download with a `.name.icloud`
placeholder. kknow shows such a file with its real name, but it does not read
it. Otherwise sync would think that the note was deleted.

## Git

A git vault works differently from S3 and a folder. Git has commits, branches
and its own merge, so kknow does not copy files. It runs the `git` program of
your computer. Your SSH keys, credential helpers and `~/.gitconfig` work as
they work in the terminal.

### Before you start

kknow needs git on your computer. Press <kbd>⌘,</kbd>, go to **General** and
set **Git**. Click **Find** to fill in the path that kknow finds. Then the
text under the field shows the `git version` of that path.

kknow does not guess the path. An app that starts from the Dock does not get
the `PATH` of your terminal. Also, `/usr/bin/git` on macOS can be a stub that
asks you to install the Xcode tools.

### Connect a vault

1. Go to **Vaults**, select the vault and set the provider to
   **Git repository**.
2. Enter the address, for example `https://github.com/me/notes.git` or
   `git@github.com:me/notes.git`.
3. Select the sign-in:
   - **as git is set up**: kknow gives no token. Git uses an SSH key or its
     credential helper (for example, after `gh auth setup-git`);
   - **token**: the window keeps the token of this repository in the system
     keychain;
   - **token from an environment variable**: enter the variable name, for
     example `GITHUB_TOKEN`. kknow reads the variable from its own process
     first, then from your login shell (`$SHELL -l`). Thus a variable from
     `config.fish` or `.zprofile` works also in an app from the Dock.
4. Click **Connect**.

kknow accepts two kinds of vault folders:

- **an empty folder**: kknow makes a repository and checks out the default
  branch of the address. An empty remote repository is also correct;
- **a folder that is already a repository**: kknow takes the address of
  `origin`. The vault must be the root of the repository: one vault is one
  repository.

kknow refuses a folder with notes and without `.git`. Two histories of the
same notes cannot merge without questions. Also, kknow refuses an address with
a login or a token in it, because git writes the address to `.git/config` as
plain text.

### Commit the changes

Click the branch icon on the vault row in the sidebar, or the sync segment in
the status bar. The icon shows when you move the pointer over the row, next to
**⋯** and **+**. Both open the **Changes** view. It covers the editor and
the preview and has two parts.

The narrow column on the left has:

- the branch and the commits to push or to pull;
- **Staged**: what the next commit takes. **−** removes a file from it;
- **Changed**: the files that are not staged. **+** stages a file;
- the commit message and **Commit**. <kbd>⌘↵</kbd> in the message also
  commits.

The right part shows the diff of the selected file, as `git diff` gives it:
for a staged file, the diff of the index; for a changed file, the diff of
the work tree. The first file is selected when the view opens. Click another
file to see its diff. When you stage or unstage a file, the diff follows it
to the other list.

The ↺ button on a file throws away its changes. It shows on the selected
file and on the file under the pointer, and asks first:

- in **Changed**, the file goes back to its staged version, or to the last
  commit if nothing is staged. A new file that git does not track is removed;
- in **Staged**, the file goes back to the last commit, both in the index and
  on the disk. A new file leaves the index. A renamed file gets its old name
  back.

The current version of the file goes to the [trash](/features/organize#trash) first,
so you can restore it. A file with a merge conflict has no ↺ button.

Double-click a note to open it. The view closes, as it does when you open a
note in the sidebar. <kbd>Esc</kbd> or **×** also closes the view. The view
follows your saves.

### Sync

The refresh button in the view, the palette action and <kbd>⌘⇧S</kbd> sync
the vault:

1. `git fetch`;
2. a merge of the remote branch;
3. `git push`.

The sync does not commit. Only you decide what goes into a commit. A git
vault has no automatic sync and no conflict policy.

Git merges edits in different parts of one note by itself. If both sides
changed the same lines, your version stays in place and the remote version
goes next to it as a conflict copy, as with the other providers. The note gets
no `<<<<<<<` marks.

If you have uncommitted edits in a file that the merge must change, the sync
stops and names the files. Commit them, then sync again.

### What goes into the repository

kknow adds these lines to `.git/info/exclude`. It does not change your
`.gitignore`:

```text
/.kknow/remote.json
/.kknow/sync.json
/.kknow/*.tmp
/.kknow/trash/
/.trash/
```

These are the settings of this computer and the trash. The rest of `.kknow/`
goes into the repository: bookmarks, themes and commands. kknow checks the
lines each time it reads the changes, so a vault that you connected with an
older version gets the new lines too.

### Git in the terminal

```bash
kknow --vault ~/notes sync --dry-run   # the branch, unpushed commits, changes
kknow --vault ~/notes sync             # fetch, merge, push
```

The terminal uses `KKNOW_GIT` or `git` from `PATH`. For the "token" sign-in it
reads `KKNOW_GIT_TOKEN`, because the terminal cannot read the keychain of the
window.

To learn how the sync algorithm works, read
[Architecture → Sync](/architecture#sync).
