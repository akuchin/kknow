//! S3 as note storage, through opendal.
//!
//! opendal is async, but kknow is fully sync: the vault reads files, the CLI
//! waits for the result. So the provider uses the blocking opendal facade on
//! its own tokio runtime. The cost is two threads per process. The gain is
//! that sync does not force `async` on the rest of the code.
//!
//! Any S3-compatible storage works: MinIO uses the same code as AWS, only
//! the address is different.

use std::sync::LazyLock;

use opendal::services::S3;
use opendal::{ErrorKind, Operator, blocking, options::ListOptions};

use crate::provider::{Error, Operation, Result, SyncProvider};

/// One runtime per process, not per provider.
///
/// The opendal HTTP client outlives the operator and keeps a connection pool
/// on the runtime that opened the connections. If the runtime were dropped
/// with the provider, the shared pool would keep dead connections, and the
/// next provider would get "runtime dropped the dispatch task".
static RUNTIME: LazyLock<std::io::Result<tokio::runtime::Runtime>> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("kknow-sync")
        .build()
});

fn runtime() -> Result<&'static tokio::runtime::Runtime> {
    RUNTIME
        .as_ref()
        .map_err(|source| Error::new(Operation::Get, "tokio", source.to_string()))
}

/// Where to connect.
///
/// Access keys are empty by default: opendal reads `AWS_*` from the
/// environment and `~/.aws/credentials`. The explicit fields are for MinIO
/// and tests.
#[derive(Debug, Clone, Default)]
pub struct S3Config {
    pub bucket: String,
    /// Storage address. AWS does not need it.
    pub endpoint: Option<String>,
    pub region: Option<String>,
    /// Prefix inside the bucket, so one bucket can hold several vaults.
    pub root: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl S3Config {
    pub fn new(bucket: impl Into<String>) -> Self {
        S3Config {
            bucket: bucket.into(),
            ..S3Config::default()
        }
    }

    /// The address to show to a person, without keys.
    ///
    /// `config::s3_location` builds it. The settings show the same string, and
    /// the two must match: the keychain stores the secret under this string.
    pub fn location(&self) -> String {
        crate::config::s3_location(self.endpoint.as_deref(), &self.bucket, self.root.as_deref())
    }
}

pub struct S3Provider {
    operator: blocking::Operator,
}

impl S3Provider {
    pub fn connect(config: &S3Config) -> Result<Self> {
        let mut builder = S3::default().bucket(&config.bucket);
        if let Some(endpoint) = &config.endpoint {
            builder = builder.endpoint(endpoint);
        }
        // MinIO does not need a region, but AWS signing fails without one.
        builder = builder.region(config.region.as_deref().unwrap_or("us-east-1"));
        if let Some(root) = &config.root {
            builder = builder.root(root);
        }
        if let Some(key) = &config.access_key_id {
            builder = builder.access_key_id(key);
        }
        if let Some(secret) = &config.secret_access_key {
            builder = builder.secret_access_key(secret);
        }

        let operator = Operator::new(builder)
            .map_err(|source| Error::new(Operation::Get, &config.bucket, source))?
            .finish();

        // blocking::Operator keeps the runtime it was created in. Use our own
        // runtime, not the current one: otherwise a call from other async code
        // would block that code's thread while it waits for its own reply.
        let operator = {
            let _guard = runtime()?.enter();
            blocking::Operator::new(operator)
                .map_err(|source| Error::new(Operation::Get, &config.bucket, source))?
        };

        Ok(S3Provider { operator })
    }
}

impl SyncProvider for S3Provider {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.operator.read(key) {
            Ok(buffer) => Ok(Some(buffer.to_vec())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::new(Operation::Get, key, source)),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.operator
            .write(key, bytes.to_vec())
            .map(|_| ())
            .map_err(|source| Error::new(Operation::Put, key, source))
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.operator
            .delete(key)
            .map_err(|source| Error::new(Operation::Delete, key, source))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let options = ListOptions {
            recursive: true,
            ..ListOptions::default()
        };

        match self.operator.list_options(prefix, options) {
            Ok(entries) => Ok(entries
                .into_iter()
                .map(|entry| entry.path().to_string())
                // S3 directories are not real, but list returns them too.
                .filter(|path| !path.ends_with('/'))
                .collect()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(source) => Err(Error::new(Operation::List, prefix, source)),
        }
    }
}

impl std::fmt::Debug for S3Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Provider").finish_non_exhaustive()
    }
}
