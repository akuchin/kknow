use std::sync::LazyLock;

use opendal::services::S3;
use opendal::{ErrorKind, Operator, blocking, options::ListOptions};

use crate::provider::{Cause, Error, Operation, Result, SyncProvider};

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

#[derive(Debug, Clone, Default)]
pub struct S3Config {
    pub bucket: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
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

    pub fn location(&self) -> String {
        crate::config::s3_location(self.endpoint.as_deref(), &self.bucket, self.root.as_deref())
    }
}

pub struct S3Provider {
    operator: blocking::Operator,
    bucket: String,
    address: String,
}

impl S3Provider {
    pub fn connect(config: &S3Config) -> Result<Self> {
        let mut builder = S3::default().bucket(&config.bucket);
        if let Some(endpoint) = &config.endpoint {
            builder = builder.endpoint(endpoint);
        }
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

fn s3_code(text: &str) -> Option<String> {
    let start = text.find("code: \"")? + "code: \"".len();
    let end = text[start..].find('"')?;
    Some(text[start..start + end].to_string())
}

fn short(text: &str) -> String {
    let Some(start) = text.find(", context: {") else {
        return text.to_string();
    };
    match text[start..].find("} =>") {
        Some(end) => format!("{}{}", &text[..start], &text[start + end + 1..]),
        None => text[..start].to_string(),
    }
}

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

    fn unreachable() -> S3Provider {
        S3Provider::connect(&S3Config {
            endpoint: Some("http://127.0.0.1:9".into()),
            access_key_id: Some("ключ".into()),
            secret_access_key: Some("секрет".into()),
            ..S3Config::new("заметки")
        })
        .unwrap()
    }

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
