use std::path::{Path, PathBuf};

#[cfg(unix)]
const WELL_KNOWN_BINS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/opt/local/bin",
    "/usr/bin",
    "/bin",
];

pub fn locate(program: &str) -> Option<PathBuf> {
    first_real(candidates(program).into_iter())
}

pub(crate) fn locate_any(names: &[&str]) -> Option<PathBuf> {
    first_real(names.iter().flat_map(|name| candidates(name)))
}

fn candidates(program: &str) -> Vec<PathBuf> {
    let extensions = executable_extensions();
    if let Some(rest) = program.strip_prefix("~/") {
        let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
        else {
            return Vec::new();
        };
        return with_extensions(&PathBuf::from(home).join(rest), &extensions)
            .filter(|candidate| runnable(candidate))
            .collect();
    }
    if program.contains('/') || program.contains(std::path::MAIN_SEPARATOR) {
        return with_extensions(Path::new(program), &extensions)
            .filter(|candidate| runnable(candidate))
            .collect();
    }

    let from_path = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    from_path
        .into_iter()
        .chain(well_known_bins())
        .flat_map(|dir| with_extensions(&dir.join(program), &extensions).collect::<Vec<_>>())
        .filter(|candidate| runnable(candidate) && !is_wsl_launcher(candidate))
        .collect()
}

fn first_real(candidates: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    let mut stub = None;
    for candidate in candidates {
        if std::fs::metadata(&candidate).is_ok_and(|meta| meta.len() > 0) {
            return Some(candidate);
        }
        stub.get_or_insert(candidate);
    }
    stub
}

fn with_extensions<'a>(
    path: &'a Path,
    extensions: &'a [String],
) -> impl Iterator<Item = PathBuf> + 'a {
    std::iter::once(path.to_path_buf()).chain(extensions.iter().map(move |extension| {
        let mut name = path.as_os_str().to_os_string();
        name.push(extension);
        PathBuf::from(name)
    }))
}

#[cfg(unix)]
fn executable_extensions() -> Vec<String> {
    Vec::new()
}

#[cfg(not(unix))]
fn executable_extensions() -> Vec<String> {
    parse_pathext(std::env::var("PATHEXT").ok().as_deref())
}

#[cfg(any(not(unix), test))]
fn parse_pathext(value: Option<&str>) -> Vec<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD")
        .split(';')
        .map(str::trim)
        .filter(|extension| extension.starts_with('.') && extension.len() > 1)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(unix)]
pub(crate) fn well_known_bins() -> Vec<PathBuf> {
    WELL_KNOWN_BINS.iter().map(PathBuf::from).collect()
}

#[cfg(not(unix))]
pub(crate) fn well_known_bins() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    for name in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(folder) = var(name) {
            roots.push(folder.join("Git"));
        }
    }
    if let Some(local) = var("LOCALAPPDATA") {
        roots.push(local.join("Programs").join("Git"));
    }
    if let Some(home) = var("USERPROFILE") {
        roots.push(home.join("scoop").join("apps").join("git").join("current"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("git.exe").is_file()
                && let Some(root) = git_root_of(&dir)
            {
                roots.push(root);
            }
        }
    }
    git_for_windows_bins(roots)
}

#[cfg(any(not(unix), test))]
fn git_root_of(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_string_lossy().to_lowercase();
    let parent = dir.parent()?;
    match name.as_str() {
        "cmd" => Some(parent.to_path_buf()),
        "bin"
            if parent
                .file_name()?
                .to_string_lossy()
                .eq_ignore_ascii_case("mingw64") =>
        {
            parent.parent().map(Path::to_path_buf)
        }
        "bin" => Some(parent.to_path_buf()),
        _ => None,
    }
}

#[cfg(any(not(unix), test))]
fn git_for_windows_bins(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut bins: Vec<PathBuf> = Vec::new();
    for root in roots {
        for dir in [
            root.join("bin"),
            root.join("usr").join("bin"),
            root.join("cmd"),
        ] {
            if !bins.contains(&dir) {
                bins.push(dir);
            }
        }
    }
    bins
}

fn is_wsl_launcher(path: &Path) -> bool {
    let Some(stem) = path.file_stem() else {
        return false;
    };
    let folder = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_lowercase());
    stem.eq_ignore_ascii_case("bash")
        && matches!(folder.as_deref(), Some("system32" | "windowsapps"))
}

#[cfg(unix)]
fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::run;
    use crate::test_support::{note, spec};

    #[test]
    fn a_missing_interpreter_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut broken = spec("echo");
        broken.program = Some("такой-программы-нет".to_string());

        let refusal = run(&broken, &note(dir.path())).unwrap_err().to_string();

        assert!(refusal.contains("такой-программы-нет"), "{refusal}");
        assert!(refusal.contains("PATH"), "{refusal}");
    }

    #[test]
    fn an_interpreter_is_found_outside_a_short_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        assert!(locate("sh").is_some(), "системная оболочка есть всегда");
        assert!(locate("такой-программы-нет").is_none());
        std::fs::create_dir_all(bin.join("ловушка")).unwrap();
        assert!(locate(&bin.join("ловушка").to_string_lossy()).is_none());
    }

    #[test]
    fn a_windows_program_is_found_with_its_extension() {
        let extensions = parse_pathext(Some(".COM;.EXE; ;.BAT;CMD"));
        assert_eq!(extensions, [".com", ".exe", ".bat"]);
        assert_eq!(parse_pathext(None), [".com", ".exe", ".bat", ".cmd"]);

        let tried: Vec<PathBuf> = with_extensions(Path::new("bin/sh"), &extensions).collect();
        assert_eq!(
            tried,
            ["bin/sh", "bin/sh.com", "bin/sh.exe", "bin/sh.bat"].map(PathBuf::from)
        );
    }

    #[test]
    fn git_bash_is_found_next_to_git() {
        let root = PathBuf::from("Programs").join("Git");
        assert_eq!(git_root_of(&root.join("cmd")), Some(root.clone()));
        assert_eq!(git_root_of(&root.join("bin")), Some(root.clone()));
        assert_eq!(
            git_root_of(&root.join("mingw64").join("bin")),
            Some(root.clone())
        );
        assert_eq!(git_root_of(Path::new("Windows")), None);

        let bins = git_for_windows_bins(vec![root.clone(), root.clone()]);
        assert_eq!(
            bins,
            [
                root.join("bin"),
                root.join("usr").join("bin"),
                root.join("cmd")
            ]
        );
    }

    #[test]
    fn the_wsl_launcher_is_not_bash() {
        let system = PathBuf::from("Windows").join("System32");
        assert!(is_wsl_launcher(&system.join("bash.exe")));
        assert!(is_wsl_launcher(
            &PathBuf::from("WindowsApps").join("bash.exe")
        ));
        assert!(!is_wsl_launcher(
            &PathBuf::from("Git").join("bin").join("bash.exe")
        ));
        assert!(!is_wsl_launcher(&system.join("cmd.exe")));
    }

    #[test]
    fn a_home_relative_program_is_expanded() {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("своя-программа");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let was = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.path()) };
        let found = locate("~/bin/своя-программа");
        match was {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }

        assert_eq!(found, Some(tool));
    }
}
