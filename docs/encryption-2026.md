# Plan: E2E encryption for sync

**Status: in progress.** Steps 1–3 are in `main`.

This plan covers the first item of "Larger tasks" in [TODO.md](../TODO.md)
and Phase 2 of [launch-checklist.md](launch-checklist.md). Do the steps in
order. Each step is one commit. After each step, run `cargo test --workspace`
and `cargo clippy --workspace --all-targets -- -D warnings`. After a UI step,
also run `pnpm ui:test` and `pnpm ui:e2e`.

## Decisions

- **Names and contents are encrypted** (the `rclone crypt` model). The
  storage shows only the number of objects and their approximate sizes.
- **A plaintext note in the storage is imported and encrypted.** A person or
  an agent can still drop a `.md` file into the bucket. The next sync reads
  it, encrypts it and deletes the plaintext object.
- **The key is in an envelope.** A random data key encrypts the vault. A
  passphrase and a recovery code each wrap the data key. A change of the
  passphrase does not encrypt the notes again.
- **Only `SyncProvider` storages:** S3 and folder (thus also iCloud and
  Dropbox). Git is not encrypted. The settings refuse git with encryption.
- **Not in this plan:** device keys and revocation (the `age` model), turning
  encryption off, protection against rollback (see "Threat model").

## Threat model

Who has the storage (the S3 provider, the owner of the iCloud account, a
person who stole the access key) **cannot**:

- read the contents of notes and shared files;
- read the names of notes and folders;
- change an object or swap two objects without detection.

They **can**:

- see the number of objects, their sizes (rounded up) and the times of
  writes;
- delete objects;
- give back an older version of an object or of the manifest (rollback).

The site page about encryption must say this in the same words.

## How it works

### The wrapper

`Encrypted<P: SyncProvider>` is a `SyncProvider` too. The engine and the plan
do not change: they see the logical keys (`notes/a.md`,
`.kknow/manifest.json`) and plaintext bytes. Under them, the wrapper keeps
encrypted names and encrypted bytes.

```
engine ── logical key, plaintext ──▶ Encrypted<P> ── stored key, ciphertext ──▶ S3 / folder
```

### Keys

- `.kknow/key.json` is the only object with a plaintext name and plaintext
  JSON. It holds the format version, the Argon2id parameters, and the data key
  wrapped two times: by the passphrase and by the recovery code.
- Argon2id: 128 MiB, 4 passes, 1 lane (about 0.2 s). The parameters are in
  the file, so a later build can make them stronger.
- The recovery code is 128 random bits in base32, in groups:
  `K7QF-2M9X-…`. No word list, so no new dependency.
- Subkeys come from the data key through `blake3::derive_key` (`blake3` is
  already a dependency): one for contents, one for names.
- The window keeps the data key in the OS keychain, next to the S3 secret.
  The terminal takes the passphrase from `KKNOW_SYNC_PASSPHRASE` on each run.

### Objects

```
"KKE1" | nonce (24 bytes) | XChaCha20-Poly1305( u32 length | data | padding )
```

- The AAD (associated data) is the logical key. An object moved to another
  name does not open.
- Padding rounds the size up to the next multiple of 256 bytes, so small
  edits do not show in the size.
- The manifest is an object as any other. Its revision for the `commit`
  check is the hash of the ciphertext. This still works: each write has a new
  nonce.

### Names

- Stored key = `e/` + base32 (lowercase, no padding) of
  AES-SIV(name subkey, logical key). AES-SIV is deterministic: the same
  logical key always gives the same stored key, so `get` and `put` need no
  table.
- `list` decrypts each name. A name that does not decrypt and is not a known
  plaintext key is skipped, as foreign files are now.
- Base32, not base64: `FolderProvider` can be on a file system that does not
  see the case of letters (macOS, Windows).
- The whole path is encrypted, not each folder, so the tree is hidden. A file
  name has a limit of 255 bytes, so the base32 text is cut into segments of
  200 characters: `e/<200>/<200>/<rest>`. S3 has a limit of 1024 bytes for a
  key: a logical key of about 600 bytes. `put` refuses a longer key with a
  clear error.

### Import of plaintext objects

The engine gets one new step before `reconcile`:
`SyncProvider::adopt() -> Result<Vec<String>>`. The default implementation
does nothing and gives an empty list. `Encrypted::adopt`:

1. finds plaintext note keys (`*.md`) and plaintext shared keys in the list;
2. for each one: reads it, writes it encrypted, then deletes the plaintext
   object. The order is important: if the sync stops between the two writes,
   the next sync does the same work again;
3. gives the logical keys that it changed.

`reconcile` then calculates the hash again for each changed key. A plaintext
object for a note that the manifest already lists is an edit in place by
another program. With the new hash, the plan sees a remote change or a
conflict, as for any other device. (Now such an edit is not seen at all.)

### Protection against mixed storage

The worst failure: a device without the key writes plaintext notes into an
encrypted storage. Three checks prevent it.

- **A decoy manifest.** An encrypted storage also has a plaintext
  `.kknow/manifest.json` with `{"format": 2, "encrypted": true, "notes": {}}`.
  An older build reads it and stops with "manifest of version 2" (the check
  exists already, see `Manifest::from_json`). The encrypted manifest has a
  different stored key, so the two do not collide.
- **A current build without the key.** `connect` sees `.kknow/key.json` but
  the settings have no encryption. It refuses with "this storage is
  encrypted: enter the passphrase".
- **An encrypted build that sees a plaintext manifest with notes.** A device
  with an older build wrote into the storage in spite of the decoy (for
  example, a user deleted the decoy). The sync stops with a clear error. It
  does not import such a manifest.

## Steps

### Step 1. Core: crypto

- A new module `kknow_sync::crypto`: `DataKey`, `KeyFile` (`key.json`),
  `seal`/`open` for objects, `encrypt_name`/`decrypt_name`, the recovery code.
- New dependencies: `chacha20poly1305`, `aes-siv`, `argon2`, `getrandom`,
  `zeroize`. All are pure Rust.
- Tests: round trip; a changed byte, a changed key and a moved object do not
  open; the same name gives the same stored key; a long key is refused; the
  passphrase and the recovery code open the same data key; a wrong passphrase
  gives a clear error, not a panic.

### Step 2. Core: the wrapper and the engine

- `Encrypted<P>`, `SyncProvider::adopt`, the call in `stage` before
  `reconcile`.
- `RemoteConfig.encrypted: bool` (`serde(default)`, so old files still read).
  `connect` takes the data key as one more argument and wraps the provider.
  `connect` refuses git with encryption.
- Operations on a storage: `encrypt::init` (a new key, the decoy, then
  `adopt` encrypts a storage that has plaintext notes), `encrypt::unlock`
  (passphrase or code → data key), `encrypt::change_passphrase`.
- Tests on `MemoryProvider`: two devices sync through an encrypted storage;
  no stored key and no object contains the text or the name of a note; a
  plaintext note dropped into the storage arrives on both devices and leaves
  the storage as plaintext; a plaintext edit of a known note gives a
  conflict; an older-format reader stops at the decoy; a device without the
  key is refused; `init` on a full plaintext storage leaves no plaintext.

### Step 3. App and CLI

- `kknow_sync::storage` gives the storage under the encryption; the
  encryption operations work on it. `kknow-app` needs nothing new: the key
  goes through `merge::sync_engine` as one more argument.
- `src-tauri`: the data key in the keychain, next to the S3 secret
  (`<address> encryption key`). Commands `sync_encrypt` (gives the recovery
  code one time), `sync_unlock`, `sync_change_passphrase`. `SyncSettings`
  has `hasKey`. A new storage address resets the `encrypted` flag.
- CLI: `kknow sync encrypt` and `kknow sync passphrase`. There is no
  `unlock`: the terminal has no keychain, so each `kknow sync` opens the key
  with `KKNOW_SYNC_PASSPHRASE` or a prompt without echo. It also finds an
  encrypted storage that the settings do not mark (for example, a storage
  from `--folder`).

### Step 4. UI

- The sync settings get an "Encryption" section: on, the passphrase two times,
  then a screen with the recovery code and the check "I saved the code".
- Connecting to an encrypted storage asks for the passphrase or the code.
- The status shows a lock for an encrypted vault. The errors above have their
  own texts.
- All strings go to `locales/ru.json` and `locales/en.json`. Stubs in
  `ui/src/mock`, an e2e test for the "turn on" path.

### Step 5. Documentation

- `site/features/encryption.md` (a new page in the sidebar and in
  `site/features/index.md`): what is encrypted, the threat model, where the
  key is, what happens if you lose the passphrase and the code.
- `site/features/sync.md`, `site/reference/` (CLI, files: `key.json`, the
  variable), `site/architecture.md`, `ARCHITECTURE.md` (the Sync section),
  `README.md`, `TODO.md`, `launch-checklist.md`.

## Open questions

- A cache of decrypted names: `list` on each sync decrypts every name. This
  is AES-SIV on short strings, so it is fast for 10 000 notes. Measure it in
  step 2 before adding a cache.
- Rollback: a counter in the encrypted manifest and the last seen value in
  `.kknow/sync.json`. A lower counter gives a warning. This is a separate
  task after the launch.
