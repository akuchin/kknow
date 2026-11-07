//! Encryption of the storage: keys, objects and object names.
//!
//! The plan is in `docs/encryption-2026.md`. In short:
//!
//! - A random [`DataKey`] encrypts the vault. The [`KeyFile`]
//!   (`.kknow/key.json` in the storage) holds it wrapped two times: by the
//!   passphrase (Argon2id) and by the recovery code. A new passphrase changes
//!   only this file, not the notes.
//! - [`Cipher::seal`] gives `"KKE1" | nonce | XChaCha20-Poly1305(...)`. The
//!   logical key is the associated data, so an object moved to another name
//!   does not open. The plaintext is padded to a multiple of 256 bytes, so a
//!   small edit does not show in the size.
//! - [`Cipher::encrypt_name`] is deterministic (AES-SIV): the same logical
//!   key always gives the same stored key, so `get` and `put` need no table.
//!   The whole path is encrypted, not each folder, so the tree is hidden.
//!
//! This module only transforms bytes. It does no input or output.

use aes_siv::siv::Aes256Siv;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{Error, Result};

/// The key of the key file in the storage. Its name and contents are
/// plaintext: a device must read it before it has the key.
pub const KEY_FILE: &str = ".kknow/key.json";

/// The first bytes of each encrypted object.
pub const MAGIC: &[u8; 4] = b"KKE1";

/// The prefix of each encrypted object name.
pub const NAME_PREFIX: &str = "e/";

/// The shortest passphrase that the core accepts.
pub const MIN_PASSPHRASE: usize = 8;

/// S3 refuses a longer key.
const MAX_STORED_KEY: usize = 1024;

/// A file name has a limit of 255 bytes, so a long encrypted name becomes
/// several folders.
const NAME_SEGMENT: usize = 200;

/// The plaintext of an object is padded to a multiple of this size.
const PAD: usize = 256;

const NONCE: usize = 24;
const SALT: usize = 16;
const RECOVERY_BYTES: usize = 16;
const KEY_FILE_FORMAT: u32 = 1;

// Contexts for `blake3::derive_key`. They must never change: a new context
// gives other keys, and old storages do not open.
const CONTEXT_CONTENT: &str = "kknow 2026-11-07 sync object contents";
const CONTEXT_NAMES: &str = "kknow 2026-11-07 sync object names";
const CONTEXT_RECOVERY: &str = "kknow 2026-11-07 sync recovery code";
const NAME_HEADER: &[u8] = b"kknow name v1";
const AAD_PASSPHRASE: &[u8] = b"kknow key slot passphrase";
const AAD_RECOVERY: &[u8] = b"kknow key slot recovery";

/// The key that encrypts a vault. It never goes to the storage unwrapped.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct DataKey([u8; 32]);

impl DataKey {
    /// A new random key.
    pub fn generate() -> Result<Self> {
        let mut key = DataKey([0; 32]);
        random(&mut key.0)?;
        Ok(key)
    }

    /// The key as text, for the keychain of the OS.
    pub fn export(&self) -> Zeroizing<String> {
        Zeroizing::new(base32::encode(&self.0))
    }

    /// The key from [`DataKey::export`].
    pub fn import(text: &str) -> Result<Self> {
        let bytes = Zeroizing::new(
            base32::decode(text.trim())
                .ok_or_else(|| Error::Crypto("the key is damaged".into()))?,
        );
        let array: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::Crypto("the key has a wrong length".into()))?;
        Ok(DataKey(array))
    }
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DataKey(…)")
    }
}

/// The code that opens the vault when the passphrase is lost.
///
/// 128 random bits in base32, in groups of four: `K7QF-2M9X-…`. A person
/// writes it on paper, so the text has no letters that look like digits:
/// base32 has no `0`, `1`, `8` or `9`.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct RecoveryCode([u8; RECOVERY_BYTES]);

impl RecoveryCode {
    fn generate() -> Result<Self> {
        let mut code = RecoveryCode([0; RECOVERY_BYTES]);
        random(&mut code.0)?;
        Ok(code)
    }

    /// The code as a person sees it.
    pub fn display(&self) -> Zeroizing<String> {
        let plain = Zeroizing::new(base32::encode(&self.0).to_uppercase());
        let groups: Vec<&str> = plain
            .as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect();
        Zeroizing::new(groups.join("-"))
    }

    /// Reads a code as a person types it: any case, with or without dashes
    /// and spaces. `None` if the text is not a code.
    pub fn parse(text: &str) -> Option<Self> {
        let plain: Zeroizing<String> = Zeroizing::new(
            text.chars()
                .filter(|c| !c.is_whitespace() && *c != '-')
                .collect(),
        );
        let bytes = Zeroizing::new(base32::decode(&plain)?);
        let array: [u8; RECOVERY_BYTES] = bytes.as_slice().try_into().ok()?;
        Some(RecoveryCode(array))
    }

    /// The recovery code has 128 random bits, so a slow key derivation adds
    /// no protection. A fast hash with a salt is enough.
    fn wrapping_key(&self, salt: &[u8]) -> Zeroizing<[u8; 32]> {
        let mut hasher = blake3::Hasher::new_derive_key(CONTEXT_RECOVERY);
        hasher.update(salt);
        hasher.update(&self.0);
        Zeroizing::new(*hasher.finalize().as_bytes())
    }
}

impl std::fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryCode(…)")
    }
}

/// Argon2id parameters. They are in the key file, so a later build can make
/// them stronger and still open old files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Default for KdfParams {
    /// 128 MiB and 4 passes: about 0.2 s on an Apple M1 (measured on
    /// 2026-11-07; 64 MiB and 3 passes took 65 ms). A device does this only
    /// when it connects, then it keeps the data key in the keychain.
    fn default() -> Self {
        KdfParams {
            memory_kib: 128 * 1024,
            passes: 4,
            lanes: 1,
        }
    }
}

impl KdfParams {
    fn derive(&self, passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
        let params = Params::new(self.memory_kib, self.passes, self.lanes, Some(32))
            .map_err(|why| Error::Crypto(format!("Argon2 parameters: {why}")))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut out = Zeroizing::new([0; 32]);
        argon
            .hash_password_into(passphrase.as_bytes(), salt, &mut out[..])
            .map_err(|why| Error::Crypto(format!("Argon2: {why}")))?;
        Ok(out)
    }
}

/// The data key wrapped by one secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    #[serde(with = "base32::serde")]
    salt: Vec<u8>,
    #[serde(with = "base32::serde")]
    nonce: Vec<u8>,
    #[serde(with = "base32::serde")]
    wrapped: Vec<u8>,
}

impl Slot {
    fn wrap(key: &DataKey, wrapping: &[u8; 32], salt: Vec<u8>, aad: &[u8]) -> Result<Self> {
        let mut nonce = vec![0; NONCE];
        random(&mut nonce)?;
        let wrapped = XChaCha20Poly1305::new(wrapping.into())
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: &key.0, aad })
            .map_err(|_| Error::Crypto("cannot wrap the key".into()))?;
        Ok(Slot {
            salt,
            nonce,
            wrapped,
        })
    }

    /// `None` if the secret is wrong.
    fn unwrap(&self, wrapping: &[u8; 32], aad: &[u8]) -> Option<DataKey> {
        if self.nonce.len() != NONCE {
            return None;
        }
        let bytes = Zeroizing::new(
            XChaCha20Poly1305::new(wrapping.into())
                .decrypt(
                    XNonce::from_slice(&self.nonce),
                    Payload {
                        msg: &self.wrapped,
                        aad,
                    },
                )
                .ok()?,
        );
        let array: [u8; 32] = bytes.as_slice().try_into().ok()?;
        Some(DataKey(array))
    }
}

/// `.kknow/key.json`: the data key, wrapped by the passphrase and by the
/// recovery code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFile {
    format: u32,
    kdf: KdfParams,
    passphrase: Slot,
    recovery: Slot,
}

impl KeyFile {
    /// A new vault key with the default Argon2 parameters.
    pub fn create(passphrase: &str) -> Result<(KeyFile, DataKey, RecoveryCode)> {
        Self::create_with(passphrase, KdfParams::default())
    }

    /// A new vault key. Tests give weak parameters to stay fast.
    pub fn create_with(
        passphrase: &str,
        kdf: KdfParams,
    ) -> Result<(KeyFile, DataKey, RecoveryCode)> {
        check_passphrase(passphrase)?;
        let key = DataKey::generate()?;
        let code = RecoveryCode::generate()?;

        let passphrase = passphrase_slot(&key, passphrase, kdf)?;
        let salt = salt()?;
        let recovery = Slot::wrap(&key, &code.wrapping_key(&salt), salt, AAD_RECOVERY)?;

        let file = KeyFile {
            format: KEY_FILE_FORMAT,
            kdf,
            passphrase,
            recovery,
        };
        Ok((file, key, code))
    }

    /// The data key from the passphrase or from the recovery code.
    ///
    /// A person types one of the two in the same field. A text that reads as
    /// a code is tried as a code first: that check is fast.
    pub fn unlock(&self, secret: &str) -> Result<DataKey> {
        if let Some(code) = RecoveryCode::parse(secret) {
            let wrapping = code.wrapping_key(&self.recovery.salt);
            if let Some(key) = self.recovery.unwrap(&wrapping, AAD_RECOVERY) {
                return Ok(key);
            }
        }
        let wrapping = self.kdf.derive(secret, &self.passphrase.salt)?;
        self.passphrase
            .unwrap(&wrapping, AAD_PASSPHRASE)
            .ok_or(Error::WrongPassphrase)
    }

    /// A new passphrase for the same data key. The notes stay as they are,
    /// and the recovery code still works.
    pub fn change_passphrase(&mut self, key: &DataKey, passphrase: &str) -> Result<()> {
        check_passphrase(passphrase)?;
        self.passphrase = passphrase_slot(key, passphrase, self.kdf)?;
        Ok(())
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let file: KeyFile = serde_json::from_slice(bytes)
            .map_err(|why| Error::Crypto(format!("the key file is damaged: {why}")))?;
        if file.format > KEY_FILE_FORMAT {
            return Err(Error::Crypto(format!(
                "the key file has format {}, and this build knows only {KEY_FILE_FORMAT}",
                file.format
            )));
        }
        Ok(file)
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).expect("the key file is plain data")
    }
}

fn passphrase_slot(key: &DataKey, passphrase: &str, kdf: KdfParams) -> Result<Slot> {
    let salt = salt()?;
    let wrapping = kdf.derive(passphrase, &salt)?;
    Slot::wrap(key, &wrapping, salt, AAD_PASSPHRASE)
}

fn check_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(Error::PassphraseTooShort(MIN_PASSPHRASE));
    }
    Ok(())
}

/// Encrypts objects and their names with one data key.
pub struct Cipher {
    content: XChaCha20Poly1305,
    names: Zeroizing<[u8; 64]>,
}

impl Cipher {
    pub fn new(key: &DataKey) -> Self {
        let content = Zeroizing::new(blake3::derive_key(CONTEXT_CONTENT, &key.0));
        let mut names = Zeroizing::new([0; 64]);
        let mut hasher = blake3::Hasher::new_derive_key(CONTEXT_NAMES);
        hasher.update(&key.0);
        hasher.finalize_xof().fill(&mut names[..]);
        Cipher {
            content: XChaCha20Poly1305::new((&*content).into()),
            names,
        }
    }

    /// Encrypts the bytes of the object with the given logical key.
    pub fn seal(&self, key: &str, data: &[u8]) -> Result<Vec<u8>> {
        let length = u32::try_from(data.len())
            .map_err(|_| Error::Crypto(format!("{key}: the object is larger than 4 GiB")))?;
        let mut plain = Zeroizing::new(Vec::with_capacity(padded(data.len())));
        plain.extend_from_slice(&length.to_le_bytes());
        plain.extend_from_slice(data);
        plain.resize(padded(data.len()), 0);

        let mut nonce = [0; NONCE];
        random(&mut nonce)?;
        let sealed = self
            .content
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: key.as_bytes(),
                },
            )
            .map_err(|_| Error::Crypto(format!("{key}: cannot encrypt")))?;

        let mut out = Vec::with_capacity(MAGIC.len() + NONCE + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// Decrypts an object from [`Cipher::seal`]. The logical key must be the
    /// same as at the write.
    pub fn open(&self, key: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let damaged = || Error::Damaged(key.to_string());
        let body = bytes.strip_prefix(MAGIC.as_slice()).ok_or_else(damaged)?;
        if body.len() < NONCE {
            return Err(damaged());
        }
        let (nonce, sealed) = body.split_at(NONCE);
        let plain = Zeroizing::new(
            self.content
                .decrypt(
                    XNonce::from_slice(nonce),
                    Payload {
                        msg: sealed,
                        aad: key.as_bytes(),
                    },
                )
                .map_err(|_| damaged())?,
        );
        let (length, rest) = plain.split_first_chunk::<4>().ok_or_else(damaged)?;
        let length = u32::from_le_bytes(*length) as usize;
        rest.get(..length).map(<[u8]>::to_vec).ok_or_else(damaged)
    }

    /// The stored key for a logical key: `e/` and the base32 text of
    /// AES-SIV, cut into segments of 200 characters.
    pub fn encrypt_name(&self, key: &str) -> Result<String> {
        let sealed = self
            .siv()
            .encrypt([NAME_HEADER], key.as_bytes())
            .map_err(|_| Error::Crypto(format!("{key}: cannot encrypt the name")))?;
        let text = base32::encode(&sealed);
        let segments: Vec<&str> = text
            .as_bytes()
            .chunks(NAME_SEGMENT)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect();
        let stored = format!("{NAME_PREFIX}{}", segments.join("/"));
        if stored.len() > MAX_STORED_KEY {
            return Err(Error::KeyTooLong(key.to_string()));
        }
        Ok(stored)
    }

    /// The logical key for a stored key. `None` for a key that this cipher
    /// did not make: a foreign file, a plaintext note, a different key.
    pub fn decrypt_name(&self, stored: &str) -> Option<String> {
        let text: String = stored
            .strip_prefix(NAME_PREFIX)?
            .chars()
            .filter(|c| *c != '/')
            .collect();
        let sealed = base32::decode(&text)?;
        let plain = self.siv().decrypt([NAME_HEADER], &sealed).ok()?;
        String::from_utf8(plain).ok()
    }

    fn siv(&self) -> Aes256Siv {
        Aes256Siv::new((&*self.names).into())
    }
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cipher(…)")
    }
}

/// True for bytes from [`Cipher::seal`]. A plaintext note can never start so:
/// the check is for import, not for security.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The size of the padded plaintext: the length field and the data, rounded
/// up to a multiple of [`PAD`].
fn padded(len: usize) -> usize {
    (4 + len).div_ceil(PAD) * PAD
}

fn salt() -> Result<Vec<u8>> {
    let mut salt = vec![0; SALT];
    random(&mut salt)?;
    Ok(salt)
}

fn random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|why| Error::Crypto(format!("no random numbers: {why}")))
}

/// RFC 4648 base32 in lowercase, without padding.
///
/// Not base64: `FolderProvider` can be on a file system that does not see
/// the case of letters, and there two base64 names would be one file.
mod base32 {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
        let mut buffer: u16 = 0;
        let mut bits = 0;
        for &byte in bytes {
            buffer = (buffer << 8) | u16::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(char::from(ALPHABET[usize::from((buffer >> bits) & 31)]));
            }
        }
        if bits > 0 {
            out.push(char::from(
                ALPHABET[usize::from((buffer << (5 - bits)) & 31)],
            ));
        }
        out
    }

    /// Any case. `None` for a character outside the alphabet or for bits
    /// left over at the end that are not zero.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() * 5 / 8);
        let mut buffer: u16 = 0;
        let mut bits = 0;
        for c in text.bytes() {
            let value = ALPHABET.iter().position(|&a| a == c.to_ascii_lowercase())?;
            buffer = (buffer << 5) | value as u16;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
            }
        }
        // A canonical text has fewer than five spare bits, and they are zero.
        if bits >= 5 || buffer & ((1 << bits) - 1) != 0 {
            return None;
        }
        Some(out)
    }

    pub mod serde {
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&super::encode(bytes))
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec<u8>, D::Error> {
            let text = String::deserialize(deserializer)?;
            super::decode(&text).ok_or_else(|| serde::de::Error::custom("not base32"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default Argon2 parameters take 128 MiB for each call. The format is
    /// the same.
    const WEAK: KdfParams = KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };

    fn cipher() -> Cipher {
        Cipher::new(&DataKey::generate().unwrap())
    }

    #[test]
    fn base32_round_trips_every_length() {
        for len in 0..40 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let text = base32::encode(&bytes);
            assert_eq!(base32::decode(&text).unwrap(), bytes, "length {len}");
            assert_eq!(base32::decode(&text.to_uppercase()).unwrap(), bytes);
        }
        assert_eq!(base32::encode(b"foobar"), "mzxw6ytboi");
        assert!(base32::decode("0").is_none());
        // The same text as "foobar", but the spare bits at the end are not zero.
        assert!(base32::decode("mzxw6ytbop").is_none());
    }

    #[test]
    fn an_object_round_trips() {
        let cipher = cipher();
        for data in [&b""[..], b"# Title\n\nText", &[7u8; 1000]] {
            let sealed = cipher.seal("notes/a.md", data).unwrap();
            assert!(is_sealed(&sealed));
            assert_eq!(cipher.open("notes/a.md", &sealed).unwrap(), data);
        }
    }

    #[test]
    fn the_size_shows_only_the_padded_length() {
        let cipher = cipher();
        let short = cipher.seal("a.md", b"a").unwrap();
        let longer = cipher.seal("a.md", &[b'a'; 200]).unwrap();
        assert_eq!(short.len(), longer.len());
        let overhead = MAGIC.len() + NONCE + 16;
        assert_eq!(short.len(), PAD + overhead);
        assert_eq!(
            cipher.seal("a.md", &[0; 300]).unwrap().len(),
            2 * PAD + overhead
        );
    }

    #[test]
    fn the_same_text_gives_different_bytes() {
        let cipher = cipher();
        let first = cipher.seal("a.md", b"same").unwrap();
        let second = cipher.seal("a.md", b"same").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn a_changed_byte_does_not_open() {
        let cipher = cipher();
        let sealed = cipher.seal("a.md", b"secret").unwrap();
        for index in [0, 5, sealed.len() / 2, sealed.len() - 1] {
            let mut broken = sealed.clone();
            broken[index] ^= 1;
            assert!(matches!(
                cipher.open("a.md", &broken),
                Err(Error::Damaged(key)) if key == "a.md"
            ));
        }
        assert!(cipher.open("a.md", &sealed[..10]).is_err());
        assert!(cipher.open("a.md", b"# plain text").is_err());
    }

    #[test]
    fn a_moved_object_does_not_open() {
        let cipher = cipher();
        let sealed = cipher.seal("a.md", b"secret").unwrap();
        assert!(cipher.open("b.md", &sealed).is_err());
    }

    #[test]
    fn another_key_does_not_open() {
        let sealed = cipher().seal("a.md", b"secret").unwrap();
        assert!(cipher().open("a.md", &sealed).is_err());
    }

    #[test]
    fn a_name_is_deterministic_and_hides_the_path() {
        let cipher = cipher();
        let key = "Health/Diagnosis.md";
        let stored = cipher.encrypt_name(key).unwrap();
        assert_eq!(cipher.encrypt_name(key).unwrap(), stored);
        assert!(stored.starts_with(NAME_PREFIX));
        assert!(!stored.to_lowercase().contains("health"));
        // One path segment: the folders of the vault are hidden.
        assert_eq!(stored.matches('/').count(), 1);
        assert_eq!(cipher.decrypt_name(&stored).unwrap(), key);
        assert_ne!(cipher.encrypt_name("Health/Other.md").unwrap(), stored);
    }

    #[test]
    fn a_name_has_only_safe_characters() {
        let cipher = cipher();
        let stored = cipher.encrypt_name("Заметки/Ёлка и пробелы.md").unwrap();
        assert!(
            stored
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '/'),
            "{stored}"
        );
    }

    #[test]
    fn a_long_name_becomes_segments_that_fit_a_file_system() {
        let cipher = cipher();
        let key = format!("{}.md", "a/".repeat(200));
        let stored = cipher.encrypt_name(&key).unwrap();
        assert!(
            stored
                .split('/')
                .all(|segment| segment.len() <= NAME_SEGMENT)
        );
        assert!(stored.matches('/').count() > 1);
        assert_eq!(cipher.decrypt_name(&stored).unwrap(), key);
    }

    #[test]
    fn a_name_too_long_for_s3_is_refused() {
        let key = format!("{}.md", "x".repeat(700));
        assert!(matches!(
            cipher().encrypt_name(&key),
            Err(Error::KeyTooLong(k)) if k == key
        ));
    }

    #[test]
    fn a_foreign_name_does_not_decrypt() {
        let cipher = cipher();
        assert!(cipher.decrypt_name("notes/a.md").is_none());
        assert!(cipher.decrypt_name(".kknow/key.json").is_none());
        assert!(cipher.decrypt_name("e/abc").is_none());
        let other = super::Cipher::new(&DataKey::generate().unwrap());
        let stored = other.encrypt_name("a.md").unwrap();
        assert!(cipher.decrypt_name(&stored).is_none());
    }

    #[test]
    fn the_passphrase_and_the_code_open_the_same_key() {
        let (file, key, code) = KeyFile::create_with("correct horse", WEAK).unwrap();
        let file = KeyFile::from_json(&file.to_json()).unwrap();
        assert_eq!(file.unlock("correct horse").unwrap(), key);
        assert_eq!(file.unlock(&code.display()).unwrap(), key);
        // A person types the code in another case and without dashes.
        let typed = code.display().replace('-', " ").to_lowercase();
        assert_eq!(file.unlock(&typed).unwrap(), key);
    }

    #[test]
    fn a_wrong_passphrase_is_a_clear_error() {
        let (file, _, _) = KeyFile::create_with("correct horse", WEAK).unwrap();
        assert!(matches!(
            file.unlock("wrong horse"),
            Err(Error::WrongPassphrase)
        ));
        let (_, _, other) = KeyFile::create_with("correct horse", WEAK).unwrap();
        assert!(matches!(
            file.unlock(&other.display()),
            Err(Error::WrongPassphrase)
        ));
    }

    #[test]
    fn a_short_passphrase_is_refused() {
        assert!(matches!(
            KeyFile::create_with("short", WEAK),
            Err(Error::PassphraseTooShort(MIN_PASSPHRASE))
        ));
    }

    #[test]
    fn a_new_passphrase_keeps_the_key_and_the_code() {
        let (mut file, key, code) = KeyFile::create_with("first secret", WEAK).unwrap();
        file.change_passphrase(&key, "second secret").unwrap();
        assert!(file.unlock("first secret").is_err());
        assert_eq!(file.unlock("second secret").unwrap(), key);
        assert_eq!(file.unlock(&code.display()).unwrap(), key);
    }

    #[test]
    fn the_key_file_has_no_secret_in_it() {
        let (file, key, code) = KeyFile::create_with("correct horse", WEAK).unwrap();
        let json = String::from_utf8(file.to_json()).unwrap();
        assert!(!json.contains("correct horse"));
        assert!(!json.contains(key.export().as_str()));
        assert!(
            !json
                .to_uppercase()
                .contains(&code.display().replace('-', ""))
        );
    }

    #[test]
    fn a_key_file_from_the_future_is_refused() {
        let (file, _, _) = KeyFile::create_with("correct horse", WEAK).unwrap();
        let mut json: serde_json::Value = serde_json::from_slice(&file.to_json()).unwrap();
        json["format"] = 2.into();
        let error = KeyFile::from_json(json.to_string().as_bytes()).unwrap_err();
        assert!(error.to_string().contains("format 2"), "{error}");
        assert!(KeyFile::from_json(b"{}").is_err());
    }

    #[test]
    fn the_data_key_round_trips_through_the_keychain_text() {
        let key = DataKey::generate().unwrap();
        assert_eq!(DataKey::import(&key.export()).unwrap(), key);
        assert!(DataKey::import("abc").is_err());
        assert_eq!(format!("{key:?}"), "DataKey(…)");
    }

    #[test]
    fn the_recovery_code_reads_as_people_write_it() {
        let code = RecoveryCode::generate().unwrap();
        let shown = code.display();
        assert_eq!(shown.len(), 26 + 6);
        assert!(
            shown
                .chars()
                .all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        assert_eq!(RecoveryCode::parse(&shown).unwrap(), code);
        assert!(RecoveryCode::parse("correct horse").is_none());
    }

    #[test]
    fn the_default_parameters_are_valid() {
        let params = KdfParams::default();
        assert!(Params::new(params.memory_kib, params.passes, params.lanes, Some(32)).is_ok());
    }
}
