use std::path::PathBuf;

use crate::note::NoteId;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("корень хранилища — не каталог: {0}")]
    NotADirectory(PathBuf),

    #[error("заметки нет в хранилище: {0}")]
    NoteNotFound(NoteId),

    #[error("путь выходит за корень хранилища: {0}")]
    OutsideVault(PathBuf),

    #[error("некорректный путь папки: {0}")]
    InvalidFolder(String),

    #[error("уже существует: {0}")]
    AlreadyExists(PathBuf),

    #[error("запись корзины не найдена: {0}")]
    TrashEntryNotFound(String),

    #[error("файл слишком большой: {0}")]
    TooLarge(PathBuf),

    #[error("некорректное имя картинки: {0}")]
    InvalidImageName(String),

    #[error("ошибка ввода-вывода в {path}: {source}")]
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
