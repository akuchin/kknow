use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_via(path, &suffixed(path, ".tmp"), bytes)
}

pub fn write_atomic_via(path: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
    let guard = TempGuard(temp);
    write_temp(path, temp, bytes)?;
    fs::rename(temp, path)?;
    guard.keep();
    sync_folder(path);
    Ok(())
}

pub fn set_aside(path: &Path) -> io::Result<PathBuf> {
    let broken = suffixed(path, ".broken");
    fs::rename(path, &broken)?;
    Ok(broken)
}

#[derive(Debug)]
pub enum Loaded<T, E> {
    Missing,
    Parsed(T),
    SetAside { broken: PathBuf, error: E },
}

pub fn load_or_set_aside<T, E>(
    path: &Path,
    parse: impl FnOnce(&[u8]) -> Result<T, E>,
) -> io::Result<Loaded<T, E>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(err) => return Err(err),
    };
    match parse(&bytes) {
        Ok(value) => Ok(Loaded::Parsed(value)),
        Err(error) => {
            let broken = set_aside(path).unwrap_or_else(|_| suffixed(path, ".broken"));
            Ok(Loaded::SetAside { broken, error })
        }
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn write_temp(path: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(temp)?;
    file.write_all(bytes)?;
    if let Ok(existing) = fs::metadata(path) {
        file.set_permissions(existing.permissions())?;
    }
    file.sync_all()
}

#[cfg(unix)]
fn sync_folder(path: &Path) {
    if let Some(folder) = path.parent() {
        let folder = if folder.as_os_str().is_empty() {
            Path::new(".")
        } else {
            folder
        };
        if let Ok(handle) = File::open(folder) {
            let _ = handle.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_folder(_path: &Path) {}

struct TempGuard<'a>(&'a Path);

impl TempGuard<'_> {
    fn keep(self) {
        std::mem::forget(self);
    }
}

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        assert!(!dir.path().join("a.json.tmp").exists());
    }

    #[test]
    fn an_existing_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("A.md");
        fs::write(&path, "old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!dir.path().join("A.md.tmp").exists());
    }

    #[test]
    fn no_temp_file_stays_after_a_failed_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target");
        fs::create_dir(&path).unwrap();
        assert!(write_atomic(&path, b"x").is_err());
        assert!(!dir.path().join("target.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_permissions_of_an_existing_file_are_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        write_atomic(&path, b"new").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn the_given_temp_path_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        let temp = dir.path().join(".a.txt.kknow-tmp");
        write_atomic_via(&path, &temp, b"data").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"data");
        assert!(!temp.exists());
        assert!(!dir.path().join("a.txt.tmp").exists());
    }

    #[test]
    fn a_missing_file_loads_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_or_set_aside(&dir.path().join("a.json"), |_| Ok::<u8, ()>(1)).unwrap();
        assert!(matches!(loaded, Loaded::Missing));
    }

    #[test]
    fn a_good_file_loads_as_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        fs::write(&path, "7").unwrap();
        let loaded = load_or_set_aside(&path, |bytes| Ok::<usize, ()>(bytes.len())).unwrap();
        assert!(matches!(loaded, Loaded::Parsed(1)));
        assert!(path.exists());
    }

    #[test]
    fn a_bad_file_is_set_aside_with_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        fs::write(&path, "oops").unwrap();
        let loaded = load_or_set_aside(&path, |_| Err::<(), _>("bad")).unwrap();
        let Loaded::SetAside { broken, error } = loaded else {
            panic!("expected SetAside");
        };
        assert_eq!(error, "bad");
        assert_eq!(broken, dir.path().join("a.json.broken"));
        assert_eq!(fs::read(&broken).unwrap(), b"oops");
        assert!(!path.exists());
    }
}
