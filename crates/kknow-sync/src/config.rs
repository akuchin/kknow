//! Where this vault syncs to.
//!
//! The settings are next to the sync state, in `.kknow/remote.json`. Thus they
//! belong to the notes directory, not to the app. A second vault cannot go to
//! the wrong bucket by accident: each directory has its own address.
//!
//! This struct has **no field** for the secret key, so it cannot be written
//! even by mistake. The code that stores secrets (the OS keychain,
//! environment variables) gives the key at connect time.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::ConflictPolicy;
use crate::{Error, Result};

/// Where the vault keeps its sync settings.
///
/// The directory is hidden, so the vault does not see it as a note.
pub const CONFIG_PATH: &str = ".kknow/remote.json";

/// Settings of S3-compatible storage: AWS, MinIO, R2.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct S3Settings {
    pub bucket: String,
    /// Storage address. AWS does not need it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub region: Option<String>,
    /// Prefix inside the bucket, so one bucket can hold several vaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub prefix: Option<String>,
    /// Access key ID. It is not a secret: it is the login for the secret key,
    /// and AWS keeps it in plain text in `~/.aws/credentials`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub access_key_id: Option<String>,
}

/// Where the token for a git repository comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum GitAuth {
    /// kknow gives no token: git signs in as it is set up on this machine
    /// (an SSH key, its credential helper).
    #[default]
    System,
    /// A token of this repository in the OS keychain.
    Token,
    /// A token in an environment variable, also from the login shell.
    Env,
}

/// Settings of a git repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitSettings {
    /// The address of the repository. For a vault that was already a
    /// repository, it is the URL of `origin`.
    pub url: String,
    #[serde(default)]
    pub auth: GitAuth,
    /// The variable name for [`GitAuth::Env`], for example `GITHUB_TOKEN`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub env: Option<String>,
}

/// The kind of storage on the other end.
///
/// Cloud drive services do not get their own variants: they sync a plain
/// folder, so for kknow they are [`Provider::Folder`] with a path. Git is a
/// variant here only as a setting: it has commits and its own conflict model,
/// that is, a different algorithm (`crate::git`), not a trait implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Provider {
    S3(S3Settings),
    Folder { path: PathBuf },
    Git(GitSettings),
}

/// Storage address and conflict policy.
///
/// Unlike the manifest, this file has no format version: only kknow on the
/// same machine writes and reads it. The old format (S3 fields at the root)
/// is detected by the shape of the file, see [`Stored`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemoteConfig {
    pub provider: Provider,
    pub policy: ConflictPolicy,
    /// Auto sync interval in minutes. `0` turns auto sync off.
    ///
    /// This is a directory setting, not an app setting: a work vault can sync
    /// every five minutes, and an archive on an external disk never.
    #[serde(default)]
    pub auto_sync_minutes: u32,
}

/// The settings as stored in the file: the current format or the format from
/// before providers.
///
/// Users already have such files. Asking them to set up sync again because
/// of a format change is a bad trade.
#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    Current {
        provider: Provider,
        #[serde(default)]
        policy: ConflictPolicy,
        #[serde(default)]
        auto_sync_minutes: u32,
    },
    Legacy {
        #[serde(flatten)]
        s3: S3Settings,
        #[serde(default)]
        policy: ConflictPolicy,
    },
}

impl<'de> Deserialize<'de> for RemoteConfig {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(match Stored::deserialize(deserializer)? {
            Stored::Current {
                provider,
                policy,
                auto_sync_minutes,
            } => RemoteConfig {
                provider,
                policy,
                auto_sync_minutes,
            },
            // The old format had no auto sync, so it is off.
            Stored::Legacy { s3, policy } => RemoteConfig {
                provider: Provider::S3(s3),
                policy,
                auto_sync_minutes: 0,
            },
        })
    }
}

impl RemoteConfig {
    pub fn s3(settings: S3Settings) -> Self {
        RemoteConfig {
            provider: Provider::S3(settings),
            policy: ConflictPolicy::default(),
            auto_sync_minutes: 0,
        }
    }

    pub fn folder(path: impl Into<PathBuf>) -> Self {
        RemoteConfig {
            provider: Provider::Folder { path: path.into() },
            policy: ConflictPolicy::default(),
            auto_sync_minutes: 0,
        }
    }

    pub fn git(settings: GitSettings) -> Self {
        RemoteConfig {
            provider: Provider::Git(settings),
            policy: ConflictPolicy::default(),
            auto_sync_minutes: 0,
        }
    }

    /// Git syncs by hand: commits are a decision of the person, and a sync
    /// without new commits only gets the remote ones. So it has no auto sync
    /// and no conflict policy: a conflict always keeps both versions.
    pub fn is_git(&self) -> bool {
        matches!(self.provider, Provider::Git(_))
    }

    /// The auto sync interval that applies: always off for git.
    pub fn auto_sync(&self) -> u32 {
        if self.is_git() {
            0
        } else {
            self.auto_sync_minutes
        }
    }

    /// Path to the settings inside the vault.
    ///
    /// Takes the root, not an open [`Vault`](kknow_core::Vault): some callers
    /// have only the directory path.
    pub fn path(vault_root: impl AsRef<Path>) -> PathBuf {
        vault_root.as_ref().join(CONFIG_PATH)
    }

    /// Reads the settings. No file means sync is not set up yet.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::io(path, source)),
        }
    }

    /// Writes the settings through a temporary file, like the manifest.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.check()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("настройка сериализуема");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    /// Empty settings are not saved: without a bucket or path there is no target.
    fn check(&self) -> Result<()> {
        match &self.provider {
            Provider::S3(s3) if s3.bucket.trim().is_empty() => Err(Error::NoBucket),
            Provider::Folder { path } if path.as_os_str().is_empty() => Err(Error::NoFolder),
            Provider::Git(git) if git.url.trim().is_empty() => Err(Error::NoGitUrl),
            _ => Ok(()),
        }
    }

    /// Removes the settings, which turns sync off.
    ///
    /// The snapshot of the last sync stays. If the storage is connected again,
    /// the snapshot prevents treating all notes as conflicts.
    pub fn forget(path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::io(path, source)),
        }
    }

    /// The address to show to a person, without keys.
    ///
    /// The keychain stores the secret under this string, so it must be
    /// different for different storages.
    pub fn location(&self) -> String {
        match &self.provider {
            Provider::S3(s3) => {
                s3_location(s3.endpoint.as_deref(), &s3.bucket, s3.prefix.as_deref())
            }
            Provider::Folder { path } => path.to_string_lossy().into_owned(),
            Provider::Git(git) => git.url.clone(),
        }
    }

    /// Whether this storage needs a secret key from the keychain. A folder has
    /// no secrets; git has one only when the token is stored in kknow.
    pub fn needs_secret(&self) -> bool {
        match &self.provider {
            Provider::S3(_) => true,
            Provider::Folder { .. } => false,
            Provider::Git(git) => git.auth == GitAuth::Token,
        }
    }
}

/// The S3 address as a string. It is a separate function because builds
/// without S3 support also show it.
pub fn s3_location(endpoint: Option<&str>, bucket: &str, prefix: Option<&str>) -> String {
    let root = prefix.unwrap_or_default().trim_start_matches('/');
    match endpoint {
        Some(endpoint) => format!("{}/{bucket}/{root}", endpoint.trim_end_matches('/')),
        None => format!("s3://{bucket}/{root}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3() -> RemoteConfig {
        RemoteConfig {
            policy: ConflictPolicy::PreferLocal,
            ..RemoteConfig::s3(S3Settings {
                bucket: "notes".into(),
                endpoint: Some("http://localhost:9000".into()),
                access_key_id: Some("minioadmin".into()),
                ..S3Settings::default()
            })
        }
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);

        s3().save(&path).unwrap();

        assert_eq!(RemoteConfig::load(&path).unwrap(), Some(s3()));
        assert!(!dir.path().join(".kknow/remote.json.tmp").exists());
    }

    #[test]
    fn a_folder_needs_no_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);
        let config = RemoteConfig::folder("/Users/кто-то/iCloud/kknow");

        config.save(&path).unwrap();

        let loaded = RemoteConfig::load(&path).unwrap().unwrap();
        assert_eq!(loaded, config);
        assert!(!loaded.needs_secret());
        assert_eq!(loaded.location(), "/Users/кто-то/iCloud/kknow");
    }

    /// Users have settings from before providers on disk. Read them, do not
    /// ask to set up sync again.
    #[test]
    fn the_old_flat_format_is_still_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remote.json");
        fs::write(
            &path,
            r#"{"bucket":"notes","endpoint":"http://localhost:9000",
                "accessKeyId":"minioadmin","policy":"preferLocal"}"#,
        )
        .unwrap();

        let loaded = RemoteConfig::load(&path).unwrap().unwrap();

        assert_eq!(loaded, s3());
        assert_eq!(loaded.policy, ConflictPolicy::PreferLocal);

        // It is written back in the current format.
        loaded.save(&path).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""kind": "s3""#), "{written}");
    }

    #[test]
    fn a_git_repository_round_trips_without_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);
        let config = RemoteConfig::git(GitSettings {
            url: "https://github.com/me/notes.git".into(),
            auth: GitAuth::Env,
            env: Some("GITHUB_TOKEN".into()),
        });

        config.save(&path).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""kind": "git""#), "{written}");
        assert_eq!(RemoteConfig::load(&path).unwrap(), Some(config.clone()));
        assert_eq!(config.location(), "https://github.com/me/notes.git");
        assert!(!config.needs_secret(), "токен из переменной, не из связки");
        assert!(
            RemoteConfig::git(GitSettings::default())
                .save(&path)
                .is_err()
        );
    }

    #[test]
    fn git_has_no_auto_sync() {
        let config = RemoteConfig {
            auto_sync_minutes: 5,
            ..RemoteConfig::git(GitSettings {
                url: "git@github.com:me/notes.git".into(),
                auth: GitAuth::Token,
                env: None,
            })
        };

        assert_eq!(config.auto_sync(), 0);
        assert!(config.needs_secret());
        assert_eq!(
            RemoteConfig {
                auto_sync_minutes: 5,
                ..s3()
            }
            .auto_sync(),
            5
        );
    }

    #[test]
    fn missing_file_means_not_configured() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            RemoteConfig::load(&dir.path().join(CONFIG_PATH)).unwrap(),
            None
        );
    }

    #[test]
    fn the_secret_never_reaches_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);

        s3().save(&path).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("minioadmin"), "логин остаётся: {written}");
        assert!(!written.to_lowercase().contains("secret"), "{written}");
    }

    #[test]
    fn an_empty_destination_is_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);

        assert!(RemoteConfig::s3(S3Settings::default()).save(&path).is_err());
        assert!(RemoteConfig::folder("").save(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn forget_removes_the_file_and_forgives_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_PATH);
        s3().save(&path).unwrap();

        RemoteConfig::forget(&path).unwrap();
        RemoteConfig::forget(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn locations_differ_between_storages() {
        // The keychain finds the secret by this string, so they must not match.
        assert_eq!(s3().location(), "http://localhost:9000/notes/");
        assert_eq!(
            RemoteConfig::s3(S3Settings {
                bucket: "notes".into(),
                prefix: Some("/личное".into()),
                ..S3Settings::default()
            })
            .location(),
            "s3://notes/личное"
        );
        assert_ne!(
            s3().location(),
            RemoteConfig::folder("/tmp/notes").location()
        );
    }
}
