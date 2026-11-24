pub trait Runner: Send + Sync {
    fn kind(&self) -> &'static str;

    fn program(&self) -> &'static str;

    fn also(&self) -> &'static [&'static str] {
        &[]
    }

    fn args(&self, script: &str, login: bool) -> Vec<String>;
}

struct Interpreter {
    kind: &'static str,
    program: &'static str,
    also: &'static [&'static str],
    flag: &'static str,
    login: Option<&'static str>,
}

impl Runner for Interpreter {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn program(&self) -> &'static str {
        self.program
    }

    fn also(&self) -> &'static [&'static str] {
        self.also
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

#[cfg(windows)]
const PYTHON_ALSO: &[&str] = &["python"];
#[cfg(not(windows))]
const PYTHON_ALSO: &[&str] = &[];

static KNOWN: &[Interpreter] = &[
    Interpreter {
        kind: "shell",
        program: "sh",
        also: &[],
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "bash",
        program: "bash",
        also: &[],
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "fish",
        program: "fish",
        also: &[],
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "zsh",
        program: "zsh",
        also: &[],
        flag: "-c",
        login: Some("-l"),
    },
    Interpreter {
        kind: "python",
        program: "python3",
        also: PYTHON_ALSO,
        flag: "-c",
        login: None,
    },
];

pub fn for_kind(kind: &str) -> Option<&'static dyn Runner> {
    KNOWN
        .iter()
        .find(|runner| runner.kind == kind)
        .map(|runner| runner as &dyn Runner)
}

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
