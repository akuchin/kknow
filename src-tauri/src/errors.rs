//! Errors as the window sees them: a code and its values, not a ready text.
//!
//! The core writes its errors in Russian, because the CLI prints them as they
//! are. The window has more than one language, so it gets `error.<code>` and
//! translates it in `ui/src/i18n.ts`. The Russian text comes too: the window
//! shows it for a code that the locale file does not have yet.
//!
//! The map lives here, not in the crates: the codes are a contract with the
//! window, and the core knows nothing about the UI.

use std::collections::BTreeMap;

use kknow_sync::provider::Operation;
use serde::Serialize;

use crate::state::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ErrorView {
    pub code: &'static str,
    pub params: BTreeMap<&'static str, String>,
    pub message: String,
}

impl From<&Error> for ErrorView {
    fn from(error: &Error) -> Self {
        let (code, params) = window(error);
        ErrorView {
            code,
            params: params.into_iter().collect(),
            message: error.to_string(),
        }
    }
}

type Coded = (&'static str, Vec<(&'static str, String)>);

fn value(code: &'static str, value: impl ToString) -> Coded {
    (code, vec![("value", value.to_string())])
}

fn detail(code: &'static str, detail: impl ToString) -> Coded {
    (code, vec![("detail", detail.to_string())])
}

/// An input or output error. The common kinds have their own codes, as in
/// `kknow_core::io_text`: "os error 13" says nothing to a person.
fn io(path: &std::path::Path, source: &std::io::Error) -> Coded {
    use std::io::ErrorKind;
    let code = match source.kind() {
        ErrorKind::PermissionDenied => "noAccess",
        ErrorKind::NotFound => "pathMissing",
        ErrorKind::ReadOnlyFilesystem => "readOnly",
        ErrorKind::StorageFull => "diskFull",
        _ => "io",
    };
    if code != "io" {
        return (code, vec![("path", path.display().to_string())]);
    }
    (
        "io",
        vec![
            ("path", path.display().to_string()),
            ("detail", source.to_string()),
        ],
    )
}

fn window(error: &Error) -> Coded {
    match error {
        Error::NoVault => ("noVault", vec![]),
        Error::NotFound(id) => value("notFound", id),
        Error::NotConfigured => ("syncNotConfigured", vec![]),
        Error::Reveal(why) => detail("reveal", why),
        Error::Open(why) => detail("open", why),
        Error::NoConfig => ("noConfig", vec![]),
        Error::ShareTheme(why) => detail("shareTheme", why),
        Error::UnknownFile(path) => value("unknownFile", path),
        Error::FileGone(path) => value("fileGone", path),
        Error::ImageNotFound(target) => value("imageNotFound", target),
        Error::NoImageBytes => ("noImageBytes", vec![]),
        Error::Window(why) => detail("window", why),
        Error::UpdateNotReady => ("updateNotReady", vec![]),
        Error::Update(why) => detail("update", why),
        Error::Keychain(why) => detail("keychain", why),
        Error::GitPathNotSet => ("gitPathNotSet", vec![]),
        Error::NoGitToken => ("noGitToken", vec![]),
        Error::App(error) => app(error),
        Error::Sync(error) => sync(error),
        Error::Run(error) => run(error),
    }
}

fn app(error: &kknow_app::Error) -> Coded {
    use kknow_app::Error as E;
    match error {
        E::Vault(error) => core(error),
        E::Search(error) => search(error),
        E::Sync(error) => sync(error),
        E::Run(error) => run(error),
        E::Watch(why) => detail("watch", why),
        E::InvalidName(name) => value("invalidName", name),
        E::AlreadyExists(id) => value("noteExists", id),
        E::NotATask(id, line) => (
            "notATask",
            vec![("value", id.to_string()), ("line", line.to_string())],
        ),
        E::NotAMention(id, line) => (
            "notAMention",
            vec![("value", id.to_string()), ("line", line.to_string())],
        ),
        E::NotAnAgentComment(id, line, text) => (
            "notAnAgentComment",
            vec![
                ("value", id.to_string()),
                ("line", line.to_string()),
                ("other", text.clone()),
            ],
        ),
        E::UnknownProposal(id) => value("unknownProposal", id),
        E::BadProposal(why) => detail("badProposal", why),
        E::StaleEdit(id, index) => (
            "staleEdit",
            vec![("value", id.clone()), ("line", index.to_string())],
        ),
        E::UnknownComment(id) => value("unknownComment", id),
        E::CommentBusy(id, state) => (
            "commentBusy",
            vec![("value", id.clone()), ("other", state.to_string())],
        ),
        E::EmptyMessage => ("emptyMessage", vec![]),
        E::UnknownVault(id) => value("unknownVault", id),
        E::UnknownTarget(id) => value("unknownTarget", id),
        E::NotADirectory(path) => value("notADirectory", path.display()),
        E::NotAMarkdownFile(path) => value("notMarkdown", path.display()),
        E::DuplicateVault(path) => value("duplicateVault", path.display()),
        E::NestedVault(inner, outer) => (
            "nestedVault",
            vec![
                ("value", inner.display().to_string()),
                ("other", outer.display().to_string()),
            ],
        ),
        E::NoteLocked(id) => value("noteLocked", id),
        E::NotesClosed => ("notesClosed", vec![]),
        E::NoLockPassword => ("noLockPassword", vec![]),
        E::LockPasswordExists => ("lockPasswordExists", vec![]),
        E::NotLocked(id) => value("notLocked", id),
        E::AlreadyLocked(id) => value("alreadyLocked", id),
        E::Io { path, source } => io(path, source),
    }
}

fn core(error: &kknow_core::Error) -> Coded {
    use kknow_core::Error as E;
    match error {
        E::NotADirectory(path) => value("notADirectory", path.display()),
        E::NoteNotFound(id) => value("notFound", id),
        E::OutsideVault(path) => value("outsideVault", path.display()),
        E::InvalidFolder(path) => value("invalidFolder", path),
        E::AlreadyExists(path) => value("exists", path.display()),
        E::TrashEntryNotFound(entry) => value("trashEntryNotFound", entry),
        E::TooLarge(path) => value("tooLarge", path.display()),
        E::InvalidImageName(name) => value("invalidImageName", name),
        E::Io { path, source } => io(path, source),
    }
}

fn search(error: &kknow_index::search::Error) -> Coded {
    use kknow_index::search::Error as E;
    match error {
        E::Tantivy(why) => detail("searchIndex", why),
        E::Query(why) => detail("searchQuery", why),
    }
}

fn sync(error: &kknow_sync::Error) -> Coded {
    use kknow_sync::Error as E;
    match error {
        E::Vault(error) => core(error),
        E::Provider(error) => {
            use kknow_sync::provider::Cause;
            match &error.cause {
                Cause::KeysRefused(code) => return value("storageKeys", code),
                Cause::NoBucket(bucket) => return value("noSuchBucket", bucket),
                Cause::Unreachable(address) => return value("storageUnreachable", address),
                Cause::FolderDenied(path) => {
                    return ("folderDenied", vec![("path", path.display().to_string())]);
                }
                Cause::FolderMissing(path) => {
                    return ("folderMissing", vec![("path", path.display().to_string())]);
                }
                Cause::Other => {}
            }
            let code = match error.operation {
                Operation::Get => "storageGet",
                Operation::Put => "storagePut",
                Operation::Delete => "storageDelete",
                Operation::List => "storageList",
            };
            (
                code,
                vec![
                    ("value", error.key.clone()),
                    ("detail", error.source.to_string()),
                ],
            )
        }
        E::Manifest(why) => detail("manifest", why),
        E::NoBucket => ("noBucket", vec![]),
        E::NoFolder => ("noFolder", vec![]),
        E::NoS3 => ("noS3", vec![]),
        E::MissingRemote(id) => value("missingRemote", id),
        E::NotUtf8(id) => value("notUtf8", id),
        E::RemoteChanged => ("remoteChanged", vec![]),
        E::Crypto(why) => detail("crypto", why),
        E::WrongPassphrase => ("wrongPassphrase", vec![]),
        E::PassphraseTooShort(min) => value("passphraseTooShort", min),
        E::Damaged(key) => value("damagedObject", key),
        E::KeyTooLong(key) => value("keyTooLong", key),
        E::OtherNotesKey => ("otherNotesKey", vec![]),
        E::Locked => ("storageLocked", vec![]),
        E::NotEncrypted => ("notEncrypted", vec![]),
        E::AlreadyEncrypted => ("alreadyEncrypted", vec![]),
        E::WrongKey => ("wrongStorageKey", vec![]),
        E::MixedStorage => ("mixedStorage", vec![]),
        E::GitEncrypted => ("gitEncrypted", vec![]),
        E::NoGitUrl => ("noGitUrl", vec![]),
        E::GitIsNotAProvider => ("gitNotProvider", vec![]),
        E::GitMissing(path) => value("gitMissing", path.display()),
        E::Git(why) => detail("git", why),
        E::GitNotEmpty => ("gitNotEmpty", vec![]),
        E::GitNotRoot(top) => value("gitNotRoot", top.display()),
        E::GitNoOrigin => ("gitNoOrigin", vec![]),
        E::GitUnreachable(why) => detail("gitUnreachable", why),
        E::GitDenied(why) => detail("gitDenied", why),
        E::GitRepoMissing(why) => detail("gitRepoMissing", why),
        E::GitUrlCredentials => ("gitUrlCredentials", vec![]),
        E::GitDetached => ("gitDetached", vec![]),
        E::GitEmptyMessage => ("gitEmptyMessage", vec![]),
        E::GitLocalChanges(files) => value("gitLocalChanges", files.join(", ")),
        E::Io { path, source } => io(path, source),
    }
}

fn run(error: &kknow_run::Error) -> Coded {
    use kknow_run::Error as E;
    match error {
        E::Unknown(name) => value("unknownCommand", name),
        E::UnknownKind(kind) => value("unknownKind", kind),
        E::NotApplicable { command } => value("notApplicable", command),
        E::NoProgram { program, places } => (
            "noProgram",
            vec![("value", program.clone()), ("other", places.clone())],
        ),
        E::Spawn { program, source } => (
            "spawn",
            vec![("value", program.clone()), ("detail", source.to_string())],
        ),
        E::Manifest { path, reason } => (
            "commandsFile",
            vec![
                ("path", path.display().to_string()),
                ("detail", reason.clone()),
            ],
        ),
        E::Io { path, source } => io(path, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names of the `{{…}}` values in the texts.
    const PLACEHOLDERS: [&str; 5] = ["value", "detail", "path", "line", "other"];

    /// Every code that the window can get has a text in each locale. A code
    /// without a text would fall back to Russian in the English window.
    #[test]
    fn every_code_has_a_text_in_each_locale() {
        // The code part of this file has only two kinds of string literals:
        // codes and placeholder names. Thus a new code cannot miss the check.
        let source = include_str!("errors.rs");
        let code_part = source.split("#[cfg(test)]").next().unwrap();
        let codes: Vec<&str> = code_part
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|word| word.chars().all(|c| c.is_ascii_alphanumeric()))
            .filter(|word| !PLACEHOLDERS.contains(word))
            .collect();
        assert!(codes.len() > 40, "коды не нашлись: {codes:?}");

        for (name, text) in [
            ("ru", include_str!("../../locales/ru.json")),
            ("en", include_str!("../../locales/en.json")),
        ] {
            let dict: BTreeMap<String, String> = serde_json::from_str(text).unwrap();
            let missing: Vec<&&str> = codes
                .iter()
                .filter(|code| !dict.contains_key(&format!("error.{code}")))
                .collect();
            assert!(missing.is_empty(), "в {name}.json нет текстов: {missing:?}");
        }
    }

    #[test]
    fn a_nested_error_keeps_its_code_and_values() {
        let error = Error::App(kknow_app::Error::NotATask(
            kknow_core::NoteId::parse("план").unwrap(),
            3,
        ));

        let view = ErrorView::from(&error);

        assert_eq!(view.code, "notATask");
        assert_eq!(view.params["value"], "план");
        assert_eq!(view.params["line"], "3");
        assert_eq!(view.message, error.to_string());
    }

    /// The problems of a first start get their own codes, not "io" or
    /// "storageList" with the text of a library.
    #[test]
    fn common_problems_have_their_own_codes() {
        use kknow_sync::provider::{Cause, Error as StorageError, Operation};
        use std::io::{Error as IoError, ErrorKind};

        let io = |kind| {
            Error::Sync(kknow_sync::Error::Vault(kknow_core::Error::Io {
                path: "/notes".into(),
                source: IoError::from(kind),
            }))
        };
        let view = ErrorView::from(&io(ErrorKind::PermissionDenied));
        assert_eq!(view.code, "noAccess");
        assert_eq!(view.params["path"], "/notes");
        assert_eq!(ErrorView::from(&io(ErrorKind::NotFound)).code, "pathMissing");
        assert_eq!(ErrorView::from(&io(ErrorKind::Interrupted)).code, "io");

        let storage = |cause| {
            Error::Sync(kknow_sync::Error::Provider(
                StorageError::new(Operation::List, "", "library text").because(cause),
            ))
        };
        let cases = [
            (Cause::KeysRefused("InvalidAccessKeyId".into()), "storageKeys"),
            (Cause::NoBucket("notes".into()), "noSuchBucket"),
            (Cause::Unreachable("http://localhost:9000".into()), "storageUnreachable"),
            (Cause::FolderDenied("/x".into()), "folderDenied"),
            (Cause::FolderMissing("/x".into()), "folderMissing"),
            (Cause::Other, "storageList"),
        ];
        for (cause, code) in cases {
            assert_eq!(ErrorView::from(&storage(cause)).code, code);
        }

        let git = Error::Sync(kknow_sync::Error::GitDenied("Authentication failed".into()));
        let view = ErrorView::from(&git);
        assert_eq!(view.code, "gitDenied");
        assert_eq!(view.params["detail"], "Authentication failed");
    }
}
