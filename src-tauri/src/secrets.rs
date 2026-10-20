//! The secret access key for remote storage.
//!
//! All other sync settings are in `.kknow/remote.json` as plain text: endpoint,
//! bucket, key ID. The secret key must not go there. It goes to the OS keyring
//! (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux).
//! There the system protects it, not file permissions.
//!
//! The key belongs to the remote storage, not to the notes folder: two vaults
//! in one bucket use the same access keys.

use crate::state::Result;

pub fn load(location: &str) -> Result<Option<String>> {
    imp::load(location)
}

pub fn store(location: &str, secret: &str) -> Result<()> {
    imp::store(location, secret)
}

/// Forgets the secret. A missing entry is not an error.
pub fn forget(location: &str) -> Result<()> {
    imp::forget(location)
}

#[cfg(not(test))]
mod imp {
    use crate::state::{Error, Result};

    /// The name under which kknow appears in the keyring.
    const SERVICE: &str = "dev.kknow.desktop";

    fn entry(location: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, location).map_err(failure)
    }

    fn failure(error: keyring::Error) -> Error {
        Error::Keychain(error.to_string())
    }

    pub fn load(location: &str) -> Result<Option<String>> {
        match entry(location)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(failure(error)),
        }
    }

    pub fn store(location: &str, secret: &str) -> Result<()> {
        entry(location)?.set_password(secret).map_err(failure)
    }

    pub fn forget(location: &str) -> Result<()> {
        match entry(location)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(failure(error)),
        }
    }
}

/// In tests, secrets live in memory.
///
/// The real keyring asks the person who runs `cargo test` for permission and
/// leaves entries on their machine. A settings serialization test is not
/// worth that.
#[cfg(test)]
mod imp {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use super::Result;

    fn store_map() -> &'static Mutex<HashMap<String, String>> {
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(Mutex::default)
    }

    pub fn load(location: &str) -> Result<Option<String>> {
        Ok(store_map().lock().unwrap().get(location).cloned())
    }

    pub fn store(location: &str, secret: &str) -> Result<()> {
        store_map()
            .lock()
            .unwrap()
            .insert(location.to_string(), secret.to_string());
        Ok(())
    }

    pub fn forget(location: &str) -> Result<()> {
        store_map().lock().unwrap().remove(location);
        Ok(())
    }
}
