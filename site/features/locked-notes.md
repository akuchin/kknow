# Locked notes

You can lock a single note with a password. A locked note is encrypted on
your disk, in the trash, in the sync storage and in git. Nobody can read it
there, also a person who uses your unlocked computer while kknow is closed.

The behavior is the same as in Apple Notes: each locked note has a lock icon,
and one password opens all locked notes of the vault until you close kknow.

## Lock a note

1. Right-click the note in the sidebar, or click the note name in the header.
2. Select **Lock**.
3. The first time, kknow asks for a password two times. It must have at least
   8 characters. One password is for all locked notes of this vault. It is
   not the passphrase of [sync encryption](/features/encryption).
4. kknow shows the **recovery code** one time. Write it down and keep it in a
   safe place.

The note stays open. The tree and the header show an open lock next to it.
On disk, the file keeps its name and its `.md` extension, but its text is
ciphertext:

```text
-----BEGIN KKNOW LOCKED NOTE-----
This note is locked. Open it in kknow with the vault password.
Key: …
…
-----END KKNOW LOCKED NOTE-----
```

If the vault has a password but the locked notes are closed now, **Lock**
asks for the password one time.

In the terminal:

```bash
kknow lock Diary
```

## Open locked notes

A closed note shows a closed lock and a password field in place of its text.
Type the password and click **Unlock**. All locked notes of the vault open
until you close kknow, and all their icons change to the open lock. A wrong
password shows an error, and you can try again.

The recovery code works in the same field. After the code, kknow asks for a
new password. The notes stay as they are, and the recovery code stays valid.

## Lock all

Click an open lock in the tree or in the header, or select **Lock all notes**
in the palette (<kbd>⌘K</kbd>). kknow saves the open note and drops the key
from memory. All locked notes of the vault close.

kknow keeps the key only in memory, never on disk and never in the system
keychain. A restart of kknow closes all locked notes.

## Edit and remove the lock

You edit an open locked note as any other note. Each save writes ciphertext:
the plain text never goes to disk.

**Remove lock** in the note menu writes the note back as plain text. It needs
the password if the locked notes are closed.

In the terminal:

```bash
kknow unlock Diary
```

`kknow lock` and `kknow unlock` ask for the password, or read it from
`KKNOW_LOCK_PASSPHRASE`. The terminal keeps no session: each command asks
again.

## What a locked note does not have

kknow treats the text of a locked note as secret, also while it is open:

- **Search** finds the note by its name only. The index never has its text.
- **Backlinks, hover previews and embeds** do not show its text. Its links
  and tags are not in the graph. Links to it from other notes work.
- **Agents** get "note is locked" from each MCP tool, also while the note is
  open in the window. Comments for the agent in a locked note do not reach
  agents, and threads and proposals are not available on it.
- **Bookmarks** are not available: a bookmark keeps the text of its line in
  `.kknow/bookmarks.json`.

## Sync and devices

Sync sends the ciphertext as it is. With [sync encryption](/features/encryption)
on, the note is encrypted two times.

`.kknow/lock.json` keeps the key of locked notes, wrapped by the password and
by the recovery code. It syncs with the vault, so a second device opens the
notes with the same password. A new password changes only this file.

A conflict on a locked note always gives a [conflict copy](/features/sync),
whatever the conflict policy: kknow cannot read the two versions to choose one.
The copy is locked too. With git, the encrypted text is one line, so git never
merges two versions of a locked note: a conflict gives a copy.

Set the password on one device and sync before you lock notes on a second
device. Two devices that set a password each before a sync have two keys, and
a note of one does not open on the other.

## Limits

- The file name, and the fact that a note is locked, stay visible.
- The protection is only as strong as the password.
- Attachments and images of a locked note are not encrypted.
- A folder cannot be locked.
- Threads and proposals that a note had before you locked it stay in
  `.kknow/threads` and `.kknow/proposals` as they were. Close them first.
- A rename does not update the links inside a locked note: kknow does not
  read them.
- There is no auto-lock by time or on sleep, and no Touch ID.

::: danger Without the password and the code, the notes are lost
kknow has no server and no copy of your key. If you lose both the password
and the recovery code, nobody can read the locked notes, and neither can
kknow.
:::

## How it works

- A random 256-bit **notes key** encrypts the locked notes. `.kknow/lock.json`
  keeps it wrapped two times: by a key from the password (Argon2id, 128 MiB)
  and by the recovery code. This is the same envelope as `key.json` of sync
  encryption.
- Each locked note is XChaCha20-Poly1305 with a random nonce. The text is
  padded to a multiple of 256 bytes, so a small edit does not show in the size.
- The `Key:` line is a hash of the notes key. It tells "locked with another
  password" from a damaged file.

The details are in [Architecture](/architecture#locked-notes).
