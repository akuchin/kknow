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

use crate::provider::{Cause, Error, Operation, Result, SyncProvider};

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
    bucket: String,
    /// The address for "does not answer": the endpoint, or AWS.
    address: String,
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

        Ok(S3Provider {
            operator,
            bucket: config.bucket.clone(),
            address: config
                .endpoint
                .clone()
                .unwrap_or_else(|| "AWS S3".to_string()),
        })
    }

    /// An opendal error as a storage error, with the cause when it is a
    /// common one.
    ///
    /// opendal gives a kind and a long text with the HTTP headers. The S3
    /// code is in the text (`code: "NoSuchBucket"`), and a failed connection
    /// is a temporary error from the HTTP client.
    fn failure(&self, operation: Operation, key: &str, error: opendal::Error) -> Error {
        let text = error.to_string();
        let code = s3_code(&text);
        let cause = match (error.kind(), code.as_deref()) {
            (_, Some("NoSuchBucket")) => Cause::NoBucket(self.bucket.clone()),
            (ErrorKind::PermissionDenied, Some(code)) => Cause::KeysRefused(code.to_string()),
            (ErrorKind::PermissionDenied, None) => Cause::KeysRefused("403".to_string()),
            (ErrorKind::Unexpected, None)
                if error.is_temporary() && text.contains("send http request") =>
            {
                Cause::Unreachable(self.address.clone())
            }
            _ => Cause::Other,
        };
        Error::new(operation, key, short(&text)).because(cause)
    }
}

/// The S3 error code in the text of an opendal error.
fn s3_code(text: &str) -> Option<String> {
    let start = text.find("code: \"")? + "code: \"".len();
    let end = text[start..].find('"')?;
    Some(text[start..start + end].to_string())
}

/// The text of an opendal error without its context: the context has the
/// HTTP headers, and the person needs the kind and the message.
fn short(text: &str) -> String {
    let Some(start) = text.find(", context: {") else {
        return text.to_string();
    };
    match text[start..].find("} =>") {
        Some(end) => format!("{}{}", &text[..start], &text[start + end + 1..]),
        None => text[..start].to_string(),
    }
}

/// Runs a blocking opendal call where `block_on` is allowed.
///
/// opendal's `block_on` panics on a thread that drives async tasks. The window
/// is such a caller: Tauri runs `#[tauri::command(async)]` inside a tokio
/// task. Then the call goes to a separate thread with no runtime, and the
/// caller waits for it. The terminal and the scheduler threads call directly.
fn blocking<T: Send>(call: impl FnOnce() -> T + Send) -> T {
    if tokio::runtime::Handle::try_current().is_err() {
        return call();
    }
    std::thread::scope(|scope| {
        scope
            .spawn(call)
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

impl SyncProvider for S3Provider {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match blocking(|| self.operator.read(key)) {
            Ok(buffer) => Ok(Some(buffer.to_vec())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(source) => Err(self.failure(Operation::Get, key, source)),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        blocking(|| self.operator.write(key, bytes.to_vec()))
            .map(|_| ())
            .map_err(|source| self.failure(Operation::Put, key, source))
    }

    fn delete(&self, key: &str) -> Result<()> {
        blocking(|| self.operator.delete(key))
            .map_err(|source| self.failure(Operation::Delete, key, source))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let options = ListOptions {
            recursive: true,
            ..ListOptions::default()
        };

        match blocking(|| self.operator.list_options(prefix, options)) {
            Ok(entries) => Ok(entries
                .into_iter()
                .map(|entry| entry.path().to_string())
                // S3 directories are not real, but list returns them too.
                .filter(|path| !path.ends_with('/'))
                .collect()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(source) => Err(self.failure(Operation::List, prefix, source)),
        }
    }
}

impl std::fmt::Debug for S3Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Provider").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed port: each request fails at once, no S3 is needed.
    fn unreachable() -> S3Provider {
        S3Provider::connect(&S3Config {
            endpoint: Some("http://127.0.0.1:9".into()),
            access_key_id: Some("ключ".into()),
            secret_access_key: Some("секрет".into()),
            ..S3Config::new("заметки")
        })
        .unwrap()
    }

    /// The window runs `#[tauri::command(async)]` commands inside a tokio
    /// task. There opendal's `block_on` panicked, the task died, and the UI
    /// waited for the answer forever.
    #[test]
    fn a_call_from_async_code_returns_an_error_not_a_panic() {
        let caller = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();

        let outcome = caller.block_on(async {
            tokio::spawn(async {
                let provider = unreachable();
                (
                    provider.get("manifest.json").is_err(),
                    provider.put("notes/a.md", b"x").is_err(),
                    provider.delete("notes/a.md").is_err(),
                    provider.list("notes/").is_err(),
                )
            })
            .await
        });

        assert_eq!(
            outcome.expect("провайдер упал в async-коде"),
            (true, true, true, true)
        );
    }
}
