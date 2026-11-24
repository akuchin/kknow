use crate::{
    self as crypto, Cipher, DataKey, Error, KdfParams, KeyFile, RecoveryCode, Result, base32,
};

pub const LOCK_FILE: &str = ".kknow/lock.json";

pub const BEGIN: &str = "-----BEGIN KKNOW LOCKED NOTE-----";

pub const END: &str = "-----END KKNOW LOCKED NOTE-----";

const NOTICE: &str = "This note is locked. Open it in kknow with the vault password.";

const KEY_PREFIX: &str = "Key: ";

const AAD: &str = "kknow locked note v1";

pub fn is_locked(bytes: &[u8]) -> bool {
    let first = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    let first = first.strip_suffix(b"\r").unwrap_or(first);
    first == BEGIN.as_bytes()
}

pub fn create(password: &str) -> Result<(KeyFile, NotesKey, RecoveryCode)> {
    create_with(password, KdfParams::default())
}

pub fn create_with(password: &str, kdf: KdfParams) -> Result<(KeyFile, NotesKey, RecoveryCode)> {
    let (file, key, code) = KeyFile::create_with(password, kdf)?;
    Ok((file, NotesKey::new(&key), code))
}

pub struct NotesKey {
    key: DataKey,
    cipher: Cipher,
    check: String,
}

impl NotesKey {
    pub fn new(key: &DataKey) -> Self {
        NotesKey {
            key: key.clone(),
            cipher: Cipher::for_locked_notes(key),
            check: base32::encode(&crypto::key_check(key)),
        }
    }

    pub fn data_key(&self) -> &DataKey {
        &self.key
    }

    pub fn seal(&self, text: &str) -> Result<String> {
        let sealed = self.cipher.seal(AAD, text.as_bytes())?;
        Ok(format!(
            "{BEGIN}\n{NOTICE}\n{KEY_PREFIX}{}\n{}\n{END}\n",
            self.check,
            base32::encode(&sealed)
        ))
    }

    pub fn open(&self, locked: &str) -> Result<String> {
        let armor = Armor::parse(locked).ok_or_else(damaged)?;
        if armor.check != self.check {
            return Err(Error::OtherNotesKey);
        }
        let sealed = base32::decode(armor.body).ok_or_else(damaged)?;
        let plain = self.cipher.open(AAD, &sealed).map_err(|_| damaged())?;
        String::from_utf8(plain).map_err(|_| damaged())
    }
}

impl std::fmt::Debug for NotesKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotesKey(…)")
    }
}

fn damaged() -> Error {
    Error::Damaged("locked note".into())
}

struct Armor<'a> {
    check: &'a str,
    body: &'a str,
}

impl<'a> Armor<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        if lines.next()? != BEGIN {
            return None;
        }
        let mut check = None;
        let mut body = None;
        for line in lines.by_ref() {
            if line == END {
                return Some(Armor {
                    check: check?,
                    body: body?,
                });
            }
            if let Some(value) = line.strip_prefix(KEY_PREFIX) {
                check = Some(value.trim());
            } else if line != NOTICE {
                if body.is_some() {
                    return None;
                }
                body = Some(line);
            }
        }
        None
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

    fn key() -> NotesKey {
        NotesKey::new(&DataKey::generate().unwrap())
    }

    #[test]
    fn a_note_round_trips() {
        let key = key();
        for text in ["", "# Diary\n\nA private line.\n", "кириллица ✓"] {
            let locked = key.seal(text).unwrap();
            assert!(is_locked(locked.as_bytes()));
            assert!(!locked.contains("private"));
            assert_eq!(key.open(&locked).unwrap(), text);
        }
    }

    #[test]
    fn the_sealed_text_is_one_line() {
        let locked = key().seal(&"word ".repeat(2000)).unwrap();
        assert_eq!(locked.lines().count(), 5);
        assert_eq!(locked.lines().next(), Some(BEGIN));
        assert_eq!(locked.lines().last(), Some(END));
    }

    #[test]
    fn other_line_ends_still_open() {
        let key = key();
        let locked = key.seal("text").unwrap().replace('\n', "\r\n");
        assert!(is_locked(locked.as_bytes()));
        assert_eq!(key.open(&locked).unwrap(), "text");
    }

    #[test]
    fn a_plain_note_is_not_locked() {
        assert!(!is_locked(b"# Title\n"));
        assert!(!is_locked(b""));
        assert!(!is_locked(format!("text\n{BEGIN}\n").as_bytes()));
    }

    #[test]
    fn another_key_is_told_apart_from_damage() {
        let locked = key().seal("secret").unwrap();
        assert!(matches!(key().open(&locked), Err(Error::OtherNotesKey)));

        let key = key();
        let locked = key.seal("secret").unwrap();
        let body = locked.lines().nth(3).unwrap();
        let mut broken = body.to_string();
        let last = broken.pop().unwrap();
        broken.push(if last == 'a' { 'b' } else { 'a' });
        let broken = locked.replace(body, &broken);
        assert!(matches!(key.open(&broken), Err(Error::Damaged(_))));
        assert!(matches!(key.open("# plain"), Err(Error::Damaged(_))));
    }

    #[test]
    fn the_password_and_the_recovery_code_open_the_same_key() {
        let (file, key, code) = create_with("correct horse", WEAK).unwrap();
        let locked = key.seal("secret").unwrap();

        let (by_password, secret) = file.open("correct horse").unwrap();
        assert_eq!(secret, crypto::Secret::Passphrase);
        assert_eq!(NotesKey::new(&by_password).open(&locked).unwrap(), "secret");

        let (by_code, secret) = file.open(&code.display()).unwrap();
        assert_eq!(secret, crypto::Secret::RecoveryCode);
        assert_eq!(NotesKey::new(&by_code).open(&locked).unwrap(), "secret");

        assert!(matches!(
            file.open("wrong horse"),
            Err(Error::WrongPassphrase)
        ));
    }

    #[test]
    fn a_short_password_is_refused() {
        assert!(matches!(
            create_with("short", WEAK),
            Err(Error::PassphraseTooShort(8))
        ));
    }
}
