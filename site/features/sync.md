# Sync

kknow syncs a vault without a kknow server. You select the storage:

- **S3-compatible storage**: Amazon S3, MinIO, Cloudflare R2 and others;
- **a plain folder**: iCloud Drive, Dropbox, a network disk or a USB stick.

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

Sync copies only notes. The `.kknow/` folder stays on each machine:
bookmarks, the trash and the sync state.

## iCloud placeholders

iCloud can replace a file that it did not download with a `.name.icloud`
placeholder. kknow shows such a file with its real name, but it does not read
it. Otherwise sync would think that the note was deleted.

## Git

Git is not a sync provider. Its model of commits, branches and merges does not
fit four simple operations on files. You can still keep a vault in a git
repository: kknow shows the changes from `git pull` as any outside edit.

To learn how the sync algorithm works, read
[Architecture → Sync](/architecture#sync).
