use aes_siv::siv::Aes256Siv;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub mod locked;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("encryption: {0}")]
    Cipher(String),

    #[error("wrong passphrase or recovery code")]
    WrongPassphrase,

    #[error("the passphrase must have at least {0} characters")]
    PassphraseTooShort(usize),

    #[error(
        "the object {0} does not open: it is damaged, or it was encrypted with a different key"
    )]
    Damaged(String),

    #[error("the note was locked with another password: this vault has a different lock key")]
    OtherNotesKey,

    #[error("the path {0} is too long for an encrypted storage")]
    KeyTooLong(String),
}

pub const KEY_FILE: &str = ".kknow/key.json";

pub const MAGIC: &[u8; 4] = b"KKE1";

pub const NAME_PREFIX: &str = "e/";

pub const MIN_PASSPHRASE: usize = 8;

const MAX_STORED_KEY: usize = 1024;

const NAME_SEGMENT: usize = 200;

const PAD: usize = 256;

const NONCE: usize = 24;
const SALT: usize = 16;
const RECOVERY_BYTES: usize = 16;
const KEY_FILE_FORMAT: u32 = 1;

const CONTEXT_CONTENT: &str = "kknow 2026-11-07 sync object contents";
const CONTEXT_NAMES: &str = "kknow 2026-11-07 sync object names";
const CONTEXT_RECOVERY: &str = "kknow 2026-11-07 sync recovery code";
const CONTEXT_CHECK: &str = "kknow 2026-11-07 sync key check";
const CONTEXT_LOCKED: &str = "kknow 2026-11-11 locked note contents";
const NAME_HEADER: &[u8] = b"kknow name v1";
const AAD_PASSPHRASE: &[u8] = b"kknow key slot passphrase";
const AAD_RECOVERY: &[u8] = b"kknow key slot recovery";

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct DataKey([u8; 32]);

impl DataKey {
    pub fn generate() -> Result<Self> {
        let mut key = DataKey([0; 32]);
        random(&mut key.0)?;
        Ok(key)
    }

    pub fn export(&self) -> Zeroizing<String> {
        Zeroizing::new(base32::encode(&self.0))
    }

    pub fn import(text: &str) -> Result<Self> {
        let bytes = Zeroizing::new(
            base32::decode(text.trim())
                .ok_or_else(|| Error::Cipher("the key is damaged".into()))?,
        );
        let array: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::Cipher("the key has a wrong length".into()))?;
        Ok(DataKey(array))
    }
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DataKey(…)")
    }
}

#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct RecoveryCode([u8; RECOVERY_BYTES]);

impl RecoveryCode {
    fn generate() -> Result<Self> {
        let mut code = RecoveryCode([0; RECOVERY_BYTES]);
        random(&mut code.0)?;
        Ok(code)
    }

    pub fn display(&self) -> Zeroizing<String> {
        let plain = Zeroizing::new(base32::encode(&self.0).to_uppercase());
        let groups: Vec<&str> = plain
            .as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect();
        Zeroizing::new(groups.join("-"))
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Default for KdfParams {
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
            .map_err(|why| Error::Cipher(format!("Argon2 parameters: {why}")))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut out = Zeroizing::new([0; 32]);
        argon
            .hash_password_into(passphrase.as_bytes(), salt, &mut out[..])
            .map_err(|why| Error::Cipher(format!("Argon2: {why}")))?;
        Ok(out)
    }
}

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
        let mut nonce = [0; NONCE];
        random(&mut nonce)?;
        let wrapped = XChaCha20Poly1305::new(wrapping.into())
            .encrypt(&XNonce::from(nonce), Payload { msg: &key.0, aad })
            .map_err(|_| Error::Cipher("cannot wrap the key".into()))?;
        Ok(Slot {
            salt,
            nonce: nonce.to_vec(),
            wrapped,
        })
    }

    fn unwrap(&self, wrapping: &[u8; 32], aad: &[u8]) -> Option<DataKey> {
        let nonce = XNonce::try_from(self.nonce.as_slice()).ok()?;
        let bytes = Zeroizing::new(
            XChaCha20Poly1305::new(wrapping.into())
                .decrypt(
                    &nonce,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFile {
    format: u32,
    #[serde(with = "base32::serde")]
    check: Vec<u8>,
    kdf: KdfParams,
    passphrase: Slot,
    recovery: Slot,
}

impl KeyFile {
    pub fn create(passphrase: &str) -> Result<(KeyFile, DataKey, RecoveryCode)> {
        Self::create_with(passphrase, KdfParams::default())
    }

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
            check: key_check(&key),
            kdf,
            passphrase,
            recovery,
        };
        Ok((file, key, code))
    }

    pub fn unlock(&self, secret: &str) -> Result<DataKey> {
        self.open(secret).map(|(key, _)| key)
    }

    pub fn open(&self, secret: &str) -> Result<(DataKey, Secret)> {
        if let Some(code) = RecoveryCode::parse(secret) {
            let wrapping = code.wrapping_key(&self.recovery.salt);
            if let Some(key) = self.recovery.unwrap(&wrapping, AAD_RECOVERY) {
                return Ok((key, Secret::RecoveryCode));
            }
        }
        let wrapping = self.kdf.derive(secret, &self.passphrase.salt)?;
        self.passphrase
            .unwrap(&wrapping, AAD_PASSPHRASE)
            .map(|key| (key, Secret::Passphrase))
            .ok_or(Error::WrongPassphrase)
    }

    pub fn change_passphrase(&mut self, key: &DataKey, passphrase: &str) -> Result<()> {
        check_passphrase(passphrase)?;
        self.passphrase = passphrase_slot(key, passphrase, self.kdf)?;
        Ok(())
    }

    pub fn matches(&self, check: &[u8]) -> bool {
        self.check == check
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let file: KeyFile = serde_json::from_slice(bytes)
            .map_err(|why| Error::Cipher(format!("the key file is damaged: {why}")))?;
        if file.format > KEY_FILE_FORMAT {
            return Err(Error::Cipher(format!(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    Passphrase,
    RecoveryCode,
}

pub fn key_check(key: &DataKey) -> Vec<u8> {
    blake3::derive_key(CONTEXT_CHECK, &key.0)[..16].to_vec()
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

pub struct Cipher {
    content: XChaCha20Poly1305,
    names: Zeroizing<[u8; 64]>,
}

impl Cipher {
    pub fn new(key: &DataKey) -> Self {
        Self::with_context(key, CONTEXT_CONTENT)
    }

    pub fn for_locked_notes(key: &DataKey) -> Self {
        Self::with_context(key, CONTEXT_LOCKED)
    }

    fn with_context(key: &DataKey, context: &str) -> Self {
        let content = Zeroizing::new(blake3::derive_key(context, &key.0));
        let mut names = Zeroizing::new([0; 64]);
        let mut hasher = blake3::Hasher::new_derive_key(CONTEXT_NAMES);
        hasher.update(&key.0);
        hasher.finalize_xof().fill(&mut names[..]);
        Cipher {
            content: XChaCha20Poly1305::new((&*content).into()),
            names,
        }
    }

    pub fn seal(&self, key: &str, data: &[u8]) -> Result<Vec<u8>> {
        let length = u32::try_from(data.len())
            .map_err(|_| Error::Cipher(format!("{key}: the object is larger than 4 GiB")))?;
        let mut plain = Zeroizing::new(Vec::with_capacity(padded(data.len())));
        plain.extend_from_slice(&length.to_le_bytes());
        plain.extend_from_slice(data);
        plain.resize(padded(data.len()), 0);

        let mut nonce = [0; NONCE];
        random(&mut nonce)?;
        let sealed = self
            .content
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: key.as_bytes(),
                },
            )
            .map_err(|_| Error::Cipher(format!("{key}: cannot encrypt")))?;

        let mut out = Vec::with_capacity(MAGIC.len() + NONCE + sealed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    pub fn open(&self, key: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let damaged = || Error::Damaged(key.to_string());
        let body = bytes.strip_prefix(MAGIC.as_slice()).ok_or_else(damaged)?;
        if body.len() < NONCE {
            return Err(damaged());
        }
        let (nonce, sealed) = body.split_at(NONCE);
        let nonce = XNonce::try_from(nonce).map_err(|_| damaged())?;
        let plain = Zeroizing::new(
            self.content
                .decrypt(
                    &nonce,
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

    pub fn encrypt_name(&self, key: &str) -> Result<String> {
        let sealed = self
            .siv()
            .encrypt([NAME_HEADER], key.as_bytes())
            .map_err(|_| Error::Cipher(format!("{key}: cannot encrypt the name")))?;
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

pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn padded(len: usize) -> usize {
    (4 + len).div_ceil(PAD) * PAD
}

fn salt() -> Result<Vec<u8>> {
    let mut salt = vec![0; SALT];
    random(&mut salt)?;
    Ok(salt)
}

fn random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|why| Error::Cipher(format!("no random numbers: {why}")))
}

pub(crate) mod base32 {
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
        assert!(file.matches(&key_check(&key)));
        assert!(!file.matches(&key_check(&DataKey::generate().unwrap())));
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
