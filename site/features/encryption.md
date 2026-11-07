# Encryption

kknow can encrypt a sync storage. Then your notes are on your own S3 or in
your own folder, and only you can read them. kknow encrypts each note on the
device, before the note leaves it, and decrypts it after it arrives.

Encryption works with S3-compatible storage and with a folder (iCloud Drive,
Dropbox). A git repository cannot be encrypted.

## What is encrypted

| | Encrypted | Visible to the owner of the storage |
|---|---|---|
| The text of notes | yes | — |
| The names of notes and folders | yes | — |
| Bookmarks, threads, proposals | yes | — |
| The manifest of the sync | yes | — |
| The number of objects | — | yes |
| The size of each object | — | rounded up to 256 bytes |
| The time of each write | — | yes |

The storage then has a folder `e/` with objects that have random names, and
two plain files in `.kknow/`: `key.json` (the wrapped key, see below) and
`manifest.json` (a stub that stops older versions of kknow, see
[Older versions](#older-versions)).

## Turn on encryption

1. Configure sync for the vault, see [Sync](/features/sync).
2. In the settings (<kbd>⌘,</kbd>), go to **Vaults**, select the vault and
   click **turn on…** in the **Encryption** row.
3. Type a passphrase two times. It must have at least 8 characters.
4. Click **encrypt**. kknow encrypts all notes that are already in the
   storage and removes the plain copies.
5. kknow shows the **recovery code**, for example
   `K7QF-2MXA-PL4D-WN6E-ZR3T-YB5H-JC`. Write it down and keep it apart from
   the passphrase. kknow shows it only this one time.

In the terminal:

```bash
kknow --vault ~/notes sync encrypt
```

The command asks for the passphrase two times and prints the recovery code.

## Connect another device

Configure sync with the same storage on the other device. The first sync
stops with the message "the storage is encrypted". The settings page then
asks for the passphrase or the recovery code. After that, the device keeps
the key in the system keychain and does not ask again.

The terminal has no keychain, so `kknow sync` asks for the passphrase each
time, or reads it from `KKNOW_SYNC_PASSPHRASE`:

```bash
export KKNOW_SYNC_PASSPHRASE='…'
kknow --vault ~/notes sync
```

`kknow sync` finds an encrypted storage itself, also a storage that you give
with `--folder` or `--bucket`.

## Change the passphrase

Click **change passphrase…** in the **Encryption** row, or run
`kknow sync passphrase`. Type the current passphrase (or the recovery code)
and the new one. The notes stay as they are, the recovery code stays valid,
and the other devices keep working: they have the key already.

## If you lose the passphrase

Use the recovery code in place of the passphrase, then set a new passphrase.

::: danger Without the passphrase and the code, the notes are lost
kknow has no server and no copy of your key. If you lose both the passphrase
and the recovery code, nobody can read the notes in the storage, and neither
can kknow. The notes on your devices stay readable: they are plain files.
:::

## Files from other programs

A person or an agent can still put a plain `.md` file into the storage. The
next sync encrypts it, removes the plain file and brings the note to the
vault. If the file has the path of a note that already exists, kknow treats
it as a remote edit, or as a conflict if you changed the note too.

## What encryption does not protect

The owner of the storage, or a person with its access keys, cannot read or
change your notes without detection. But they can:

- delete objects;
- give back an older version of a note or of the whole storage;
- see how many notes there are, their sizes and when you write.

kknow does not protect the notes on your devices: use disk encryption for
that (FileVault, BitLocker).

## Older versions

An older version of kknow cannot read an encrypted storage. It stops with a
"manifest of version 2" error and does not write to the storage. Update kknow
on all devices before you turn on encryption.

## How it works

- A random 256-bit **data key** encrypts the storage. `.kknow/key.json` keeps
  it wrapped two times: by a key from the passphrase (Argon2id, 128 MiB) and
  by the recovery code. A new passphrase changes only this file.
- Each object is XChaCha20-Poly1305 with a random nonce. Its name is part of
  the authenticated data, so an object moved to another name does not open.
- Names are encrypted with AES-SIV, which is deterministic: the same note
  always has the same object name, so kknow needs no table of names.

The details are in [Architecture](/architecture#encryption).
