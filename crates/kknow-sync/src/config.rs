use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::ConflictPolicy;
use crate::{Error as SyncError, Result};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sync settings: no bucket is set")]
    NoBucket,

    #[error("sync settings: no folder is set")]
    NoFolder,

    #[error("this build has no S3 support")]
    NoS3,

    #[error("sync settings: no repository address is set")]
    NoGitUrl,

    #[error("git syncs the vault with its own command, not through the manifest")]
    GitIsNotAProvider,

    #[error("git storage cannot be encrypted")]
    GitEncrypted,
}

pub const CONFIG_PATH: &str = ".kknow/remote.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct S3Settings {
    pub bucket: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub access_key_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum GitAuth {
    #[default]
    System,
    Token,
    Env,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitSettings {
    pub url: String,
    #[serde(default)]
    pub auth: GitAuth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Provider {
    S3(S3Settings),
    Folder { path: PathBuf },
    Git(GitSettings),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RemoteConfig {
    pub provider: Provider,
    pub policy: ConflictPolicy,
    #[serde(default)]
    pub auto_sync_minutes: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub encrypted: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    Current {
        provider: Provider,
        #[serde(default)]
        policy: ConflictPolicy,
        #[serde(default)]
        auto_sync_minutes: u32,
        #[serde(default)]
        encrypted: bool,
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
                encrypted,
            } => RemoteConfig {
                provider,
                policy,
                auto_sync_minutes,
                encrypted,
            },
            Stored::Legacy { s3, policy } => RemoteConfig {
                provider: Provider::S3(s3),
                policy,
                auto_sync_minutes: 0,
                encrypted: false,
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
            encrypted: false,
        }
    }

    pub fn folder(path: impl Into<PathBuf>) -> Self {
        RemoteConfig {
            provider: Provider::Folder { path: path.into() },
            policy: ConflictPolicy::default(),
            auto_sync_minutes: 0,
            encrypted: false,
        }
    }

    pub fn git(settings: GitSettings) -> Self {
        RemoteConfig {
            provider: Provider::Git(settings),
            policy: ConflictPolicy::default(),
            auto_sync_minutes: 0,
            encrypted: false,
        }
    }

    pub fn is_git(&self) -> bool {
        matches!(self.provider, Provider::Git(_))
    }

    pub fn auto_sync(&self) -> u32 {
        if self.is_git() {
            0
        } else {
            self.auto_sync_minutes
        }
    }

    pub fn path(vault_root: impl AsRef<Path>) -> PathBuf {
        vault_root.as_ref().join(CONFIG_PATH)
    }

    pub fn load(path: &Path) -> Result<Option<Self>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SyncError::io(path, source)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.check()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SyncError::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("the config serializes");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| SyncError::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| SyncError::io(path, source))
    }

    fn check(&self) -> Result<()> {
        match &self.provider {
            Provider::S3(s3) if s3.bucket.trim().is_empty() => Err(Error::NoBucket.into()),
            Provider::Folder { path } if path.as_os_str().is_empty() => Err(Error::NoFolder.into()),
            Provider::Git(git) if git.url.trim().is_empty() => Err(Error::NoGitUrl.into()),
            Provider::Git(_) if self.encrypted => Err(Error::GitEncrypted.into()),
            _ => Ok(()),
        }
    }

    pub fn forget(path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SyncError::io(path, source)),
        }
    }

    pub fn location(&self) -> String {
        match &self.provider {
            Provider::S3(s3) => {
                s3_location(s3.endpoint.as_deref(), &s3.bucket, s3.prefix.as_deref())
            }
            Provider::Folder { path } => path.to_string_lossy().into_owned(),
            Provider::Git(git) => git.url.clone(),
        }
    }

    pub fn needs_secret(&self) -> bool {
        match &self.provider {
            Provider::S3(_) => true,
            Provider::Folder { .. } => false,
            Provider::Git(git) => git.auth == GitAuth::Token,
        }
    }
}

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
