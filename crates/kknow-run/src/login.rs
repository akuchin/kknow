//! One environment variable as the login shell of the user sees it.
//!
//! An app started from Finder or the Dock does not get the variables from
//! `config.fish` or `.zprofile`: launchd starts it, not a shell. Commands
//! solve this by running inside the shell (see [`crate::runner`]). A value
//! that kknow needs itself, for example a git token in `GITHUB_TOKEN`, comes
//! the same way: kknow asks the shell to print it.
//!
//! kknow reads one variable on each request and keeps nothing: a changed
//! profile works without a restart, and no secret stays in memory longer than
//! one operation needs it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A profile that waits for input or a slow network mount must not block a
/// sync for long. Five seconds is much more than a normal profile needs.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The value goes between two marks. A profile can print its own text (a
/// greeting, a warning about a missing file), and the marks separate the value
/// from that text.
const MARK: &str = "@@kknow-env@@";

/// The value of a variable: from the process environment, else from the login
/// shell.
///
/// The process environment comes first. In the terminal and in `pnpm dev` it
/// already has the user variables, and a shell start costs time. An empty value
/// means "not set": an empty token cannot sign in anywhere.
pub fn variable(name: &str) -> Option<String> {
    if !valid_name(name) {
        return None;
    }
    if let Some(value) = std::env::var(name).ok().filter(|value| !value.is_empty()) {
        return Some(value);
    }
    from_shell(&user_shell()?, name)
}

/// Only letters, digits and `_`, and not a digit first.
///
/// The name goes into the shell script text, so this check also keeps the
/// script safe: a name like `X; rm -rf ~` is not a name.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|rest| rest.is_ascii_alphanumeric() || rest == '_')
}

/// The shell of the user. launchd gives `SHELL` to apps too; without it, the
/// default shell of the platform.
#[cfg(unix)]
fn user_shell() -> Option<PathBuf> {
    let shell = std::env::var_os("SHELL")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let fallback = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    Some(shell.unwrap_or_else(|| PathBuf::from(fallback)))
}

/// Windows has no login shell: GUI apps get the user variables from the
/// system, so the process environment is all there is.
#[cfg(not(unix))]
fn user_shell() -> Option<PathBuf> {
    None
}

/// Asks the shell to print the variable between the marks.
///
/// `printf` is in sh, bash, zsh and fish, and the `"$NAME"` form also works
/// in all four.
fn from_shell(shell: &Path, name: &str) -> Option<String> {
    let script = format!(r#"printf '%s' '{MARK}' "${name}" '{MARK}'"#);
    let mut child = Command::new(shell)
        .args(["-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // The pipe is read in a thread: a profile that prints a lot would block
    // on a full pipe, and the wait below would never end.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }

    let text = reader.join().ok()?;
    let (_, rest) = text.split_once(MARK)?;
    let (value, _) = rest.split_once(MARK)?;
    Some(value.to_string()).filter(|value| !value.is_empty())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A "shell" that prints text of its own first, as a noisy profile does,
    /// and then gives the script to `sh`.
    fn noisy_shell(dir: &Path, variable: &str, value: &str) -> PathBuf {
        let path = dir.join("shell");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho 'приветствие профиля'\nexport {variable}='{value}'\nexec /bin/sh \"$@\"\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_value_comes_from_the_profile_past_its_noise() {
        let dir = tempfile::tempdir().unwrap();
        let shell = noisy_shell(dir.path(), "KKNOW_TEST_TOKEN", "ghp_секрет");

        assert_eq!(
            from_shell(&shell, "KKNOW_TEST_TOKEN").as_deref(),
            Some("ghp_секрет")
        );
    }

    #[test]
    fn an_unset_variable_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let shell = noisy_shell(dir.path(), "KKNOW_OTHER", "x");

        assert_eq!(from_shell(&shell, "KKNOW_SURELY_UNSET_VARIABLE"), None);
    }

    #[test]
    fn a_missing_shell_is_none() {
        assert_eq!(from_shell(Path::new("/нет/такой/оболочки"), "HOME"), None);
    }

    #[test]
    fn names_are_checked_before_they_reach_the_script() {
        assert!(valid_name("GITHUB_TOKEN"));
        assert!(valid_name("_x1"));
        assert!(!valid_name(""));
        assert!(!valid_name("1TOKEN"));
        assert!(!valid_name("X; rm -rf ~"));
        assert!(!valid_name("$HOME"));
        assert_eq!(variable("X; echo"), None);
    }
}
