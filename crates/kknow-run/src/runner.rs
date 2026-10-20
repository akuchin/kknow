//! What runs a script: the trait and its implementations.
//!
//! This follows the `SyncProvider` design of sync: the manifest picks the kind
//! at run time, not at build time. So the public API is a trait, and `sh`,
//! `fish` and `python3` are its implementations. A new kind (node, deno, a
//! custom launcher) is one line in [`KNOWN`]. It changes nothing in the run
//! code, the manifest or the UI.

/// How script text becomes a process.
///
/// The trait does not build `std::process::Command` itself. The manifest can
/// override the program (`program`), and a built command cannot change its
/// program. So an implementation only names the program and the arguments,
/// and [`crate::run`] builds the process.
pub trait Runner: Send + Sync {
    /// The kind name in the manifest.
    fn kind(&self) -> &'static str;

    /// The default program. It is looked up in `PATH` like any other program.
    fn program(&self) -> &'static str;

    /// The arguments, including the script text.
    ///
    /// `login` asks to read the user profile. This is the only way to get the
    /// user `PATH` when the app starts from Finder. An implementation without
    /// a profile (python) ignores the flag.
    fn args(&self, script: &str, login: bool) -> Vec<String>;
}

/// An interpreter that takes the script text as one argument.
///
/// All shells and python work like this: `sh -c '…'`, `fish -c '…'`,
/// `python3 -c '…'`. This is a property of these programs, not a
/// simplification. The trait also allows an implementation that needs a file
/// on disk or its own arguments.
struct Interpreter {
    kind: &'static str,
    program: &'static str,
    flag: &'static str,
    /// The flag that reads the profile; `None` if the program has no profile.
    login: Option<&'static str>,
}

impl Runner for Interpreter {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn program(&self) -> &'static str {
        self.program
    }

    fn args(&self, script: &str, login: bool) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        if login && let Some(flag) = self.login {
            args.push(flag.to_string());
        }
        args.push(self.flag.to_string());
        args.push(script.to_string());
        args
    }
}

/// The kinds that kknow can run.
///
/// `shell` is `sh`, not the shell of the user who started the app. A script
/// must behave the same for everyone and not depend on a local `.zshrc`.
/// For a specific shell, write `fish` or `zsh` explicitly.
static KNOWN: &[Interpreter] = &[
    Interpreter {
        kind: "shell",
        program: "sh",
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "bash",
        program: "bash",
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "fish",
        program: "fish",
        flag: "-c",
        // The flag exists but is rarely needed: fish reads `config.fish` and
        // its universal variables on every start, not only at login.
        login: Some("-l"),
    },
    Interpreter {
        kind: "zsh",
        program: "zsh",
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "python",
        program: "python3",
        flag: "-c",
        // Python has no profile: it gets `PATH` from its parent process.
        login: None,
    },
];

/// Finds the implementation for a kind name from the manifest.
pub fn for_kind(kind: &str) -> Option<&'static dyn Runner> {
    KNOWN
        .iter()
        .find(|runner| runner.kind == kind)
        .map(|runner| runner as &dyn Runner)
}

/// Names of all known kinds. The error message for a typo shows them.
pub fn kinds() -> Vec<&'static str> {
    KNOWN.iter().map(|runner| runner.kind).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_kind_resolves() {
        for kind in kinds() {
            let runner = for_kind(kind).expect("вид известен");
            assert_eq!(runner.kind(), kind);
            assert!(!runner.program().is_empty());
        }
    }

    #[test]
    fn a_typo_in_the_kind_is_not_a_runner() {
        assert!(for_kind("phyton").is_none());
        assert!(for_kind("").is_none());
    }

    #[test]
    fn the_script_goes_in_as_one_argument() {
        // The script text is not split on spaces. It goes as a whole, and the
        // interpreter does all the splitting.
        let runner = for_kind("python").expect("python известен");

        assert_eq!(runner.program(), "python3");
        assert_eq!(
            runner.args("print('привет мир')", false),
            ["-c", "print('привет мир')"]
        );
    }

    #[test]
    fn login_asks_the_shell_to_read_the_profile() {
        assert_eq!(
            for_kind("shell").unwrap().args("pwd", true),
            ["-l", "-c", "pwd"]
        );
        assert_eq!(
            for_kind("zsh").unwrap().args("pwd", true),
            ["-l", "-c", "pwd"]
        );
    }

    #[test]
    fn a_runner_without_a_profile_ignores_the_flag() {
        // Python has no profile, and it would read `-l` as a file name.
        assert_eq!(
            for_kind("python").unwrap().args("pass", true),
            ["-c", "pass"]
        );
    }

    #[test]
    fn shell_is_sh_not_the_login_shell() {
        assert_eq!(for_kind("shell").unwrap().program(), "sh");
        assert_eq!(for_kind("fish").unwrap().program(), "fish");
    }
}
