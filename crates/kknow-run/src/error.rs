use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("command is not found: {0}")]
    Unknown(String),

    #[error("unknown command kind: {0}")]
    UnknownKind(String),

    #[error("command \"{command}\" does not apply to this target")]
    NotApplicable { command: String },

    #[error(
        "cannot find program {program}: it is not in PATH or in the usual places \
         ({places}). A window started from Finder sees a short system PATH: \
         give the full path in the program field or set PATH in env. \
         On Windows, sh and bash come with Git for Windows"
    )]
    NoProgram { program: String, places: String },

    #[error("cannot start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{}: {reason}", path.display())]
    Manifest { path: PathBuf, reason: String },

    #[error("I/O error in {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
