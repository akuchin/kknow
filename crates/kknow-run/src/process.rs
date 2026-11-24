use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::locate::{locate_any, well_known_bins};
use crate::outcome::{Outcome, RunEnd, Stream, Watch};
use crate::spec::CommandSpec;
use crate::target::{Target, environment};
use crate::{Error, Result, runner};

const CAPTURE_LIMIT: usize = 64 * 1024;

const FIRST_POLL: Duration = Duration::from_millis(1);
const MAX_POLL: Duration = Duration::from_millis(50);

const DRAIN: Duration = Duration::from_millis(500);

const DRAIN_LIMIT: Duration = Duration::from_secs(5);

fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub fn run(spec: &CommandSpec, target: &Target) -> Result<Outcome> {
    run_with(spec, target, &BTreeMap::new())
}

pub fn run_with(
    spec: &CommandSpec,
    target: &Target,
    shared: &BTreeMap<String, String>,
) -> Result<Outcome> {
    let stop = AtomicBool::new(false);
    let watch = Watch {
        line: &|_, _| {},
        stop: &stop,
    };
    run_watched(spec, target, shared, &watch)
}

pub fn run_watched(
    spec: &CommandSpec,
    target: &Target,
    shared: &BTreeMap<String, String>,
    watch: &Watch<'_>,
) -> Result<Outcome> {
    if !spec.applies_to(target) {
        return Err(Error::NotApplicable {
            command: spec.id.clone(),
        });
    }

    let runner =
        runner::for_kind(&spec.kind).ok_or_else(|| Error::UnknownKind(spec.kind.clone()))?;
    let names: Vec<&str> = match spec.program.as_deref() {
        Some(explicit) => vec![explicit],
        None => std::iter::once(runner.program())
            .chain(runner.also().iter().copied())
            .collect(),
    };
    let wanted = names[0];
    let program = locate_any(&names).ok_or_else(|| Error::NoProgram {
        program: wanted.to_string(),
        places: well_known_bins()
            .iter()
            .map(|dir| dir.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    })?;

    let mut command = Command::new(&program);
    #[cfg(windows)]
    command.env("CHERE_INVOKING", "1");
    command
        .args(runner.args(&spec.script, spec.login))
        .current_dir(target.working_dir(spec.cwd))
        .envs(shared)
        .envs(environment(target))
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    hide_console(&mut command);

    let started = Instant::now();
    let finished = wait(
        command,
        Duration::from_secs(spec.timeout_seconds),
        &program.to_string_lossy(),
        watch,
    )?;

    Ok(Outcome {
        command: spec.id.clone(),
        code: finished.code,
        stdout: finished.stdout,
        stderr: finished.stderr,
        end: finished.end,
        truncated: finished.truncated,
        millis: started.elapsed().as_millis() as u64,
    })
}

struct Finished {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    end: RunEnd,
    truncated: bool,
}

fn wait(
    mut command: Command,
    timeout: Duration,
    program: &str,
    watch: &Watch<'_>,
) -> Result<Finished> {
    let mut child = command.spawn().map_err(|source| Error::Spawn {
        program: program.to_string(),
        source,
    })?;
    let group = Group::of(&child);

    let out = child.stdout.take().expect("stdout is a pipe");
    let err = child.stderr.take().expect("stderr is a pipe");
    let (out_pipe, err_pipe) = (Pipe::of(&out), Pipe::of(&err));
    let ended = OnceLock::new();

    std::thread::scope(|scope| {
        let reading_out = scope.spawn(|| {
            capture(
                out,
                || out_pipe.has_more(&ended),
                |line| (watch.line)(Stream::Stdout, line),
            )
        });
        let reading_err = scope.spawn(|| {
            capture(
                err,
                || err_pipe.has_more(&ended),
                |line| (watch.line)(Stream::Stderr, line),
            )
        });

        let deadline = Instant::now() + timeout;
        let mut pause = FIRST_POLL;
        let mut end = RunEnd::Exited;

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(source) => return Err(Error::io(program, source)),
            }

            let stopped = watch.stop.load(Ordering::Relaxed);
            if stopped || Instant::now() >= deadline {
                group.kill(&mut child);
                end = if stopped {
                    RunEnd::Stopped
                } else {
                    RunEnd::TimedOut
                };
                break None;
            }

            std::thread::sleep(pause);
            pause = (pause * 2).min(MAX_POLL);
        };
        let _ = ended.set(Instant::now());

        let (stdout, cut_out) = reading_out.join().unwrap_or_default();
        let (stderr, cut_err) = reading_err.join().unwrap_or_default();

        Ok(Finished {
            code: status.and_then(|status| status.code()),
            stdout,
            stderr,
            end,
            truncated: cut_out || cut_err,
        })
    })
}

struct Group {
    #[cfg(windows)]
    job: Option<Job>,
}

impl Group {
    #[cfg(unix)]
    fn of(_: &std::process::Child) -> Self {
        Group {}
    }

    #[cfg(windows)]
    fn of(child: &std::process::Child) -> Self {
        Group {
            job: Job::holding(child),
        }
    }

    fn kill(&self, child: &mut std::process::Child) {
        #[cfg(unix)]
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(windows)]
struct Job(std::os::windows::io::OwnedHandle);

#[cfg(windows)]
impl Job {
    fn holding(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return None;
        }
        let job = unsafe { OwnedHandle::from_raw_handle(handle) };
        let joined = unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) };
        (joined != 0).then_some(Job(job))
    }

    fn terminate(&self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        unsafe {
            TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

#[derive(Clone, Copy)]
struct Pipe {
    #[cfg(unix)]
    fd: std::os::fd::RawFd,
    #[cfg(windows)]
    handle: isize,
}

impl Pipe {
    #[cfg(unix)]
    fn of(source: &impl std::os::fd::AsRawFd) -> Self {
        Pipe {
            fd: source.as_raw_fd(),
        }
    }

    #[cfg(windows)]
    fn of(source: &impl std::os::windows::io::AsRawHandle) -> Self {
        Pipe {
            handle: source.as_raw_handle() as isize,
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn of<T>(_: &T) -> Self {
        Pipe {}
    }

    #[cfg(unix)]
    fn has_more(self, ended: &OnceLock<Instant>) -> bool {
        let mut silent_since = None;
        loop {
            if ended.get().is_some_and(|at| at.elapsed() >= DRAIN_LIMIT) {
                return false;
            }
            let mut fd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, MAX_POLL.as_millis() as libc::c_int) };
            if ready > 0 {
                return true;
            }
            if ready < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return true;
            }
            if ended.get().is_none() {
                continue;
            }
            let since = *silent_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= DRAIN {
                return false;
            }
        }
    }

    #[cfg(windows)]
    fn has_more(self, ended: &OnceLock<Instant>) -> bool {
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;

        let mut silent_since = None;
        loop {
            if ended.get().is_some_and(|at| at.elapsed() >= DRAIN_LIMIT) {
                return false;
            }
            let mut waiting: u32 = 0;
            let peeked = unsafe {
                PeekNamedPipe(
                    self.handle as *mut std::ffi::c_void,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut waiting,
                    std::ptr::null_mut(),
                )
            };
            if peeked == 0 {
                return true;
            }
            if waiting > 0 {
                return true;
            }
            if ended.get().is_none() {
                std::thread::sleep(MAX_POLL);
                continue;
            }
            let since = *silent_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= DRAIN {
                return false;
            }
            std::thread::sleep(MAX_POLL);
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn has_more(self, _: &OnceLock<Instant>) -> bool {
        true
    }
}

fn capture(
    mut source: impl Read,
    mut has_more: impl FnMut() -> bool,
    line: impl Fn(&str),
) -> (String, bool) {
    let mut kept: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    let mut sent = 0;

    while has_more() {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => break,
        };

        let room = CAPTURE_LIMIT - kept.len();
        if read > room {
            truncated = true;
        }
        kept.extend_from_slice(&buffer[..read.min(room)]);

        while let Some(end) = kept[sent..].iter().position(|&byte| byte == b'\n') {
            line(&String::from_utf8_lossy(&kept[sent..sent + end]));
            sent += end + 1;
        }
    }
    if sent < kept.len() {
        line(&String::from_utf8_lossy(&kept[sent..]));
    }

    (String::from_utf8_lossy(&kept).into_owned(), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{CommandSet, Cwd};
    use crate::target::TargetKind;
    use crate::test_support::{note, spec};

    #[test]
    fn the_path_arrives_as_a_variable() {
        let dir = tempfile::tempdir().unwrap();
        let target = note(dir.path());

        let outcome = run(
            &spec("printf '%s|%s|%s' \"$KKNOW_PATH\" \"$KKNOW_NOTE\" \"$KKNOW_TARGET\""),
            &target,
        )
        .unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        assert_eq!(
            outcome.stdout,
            format!("{}|Docs/Index|note", target.path.display())
        );
    }

    #[test]
    fn a_name_that_looks_like_a_command_stays_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        let nasty = dir.path().join(if cfg!(windows) {
            "'; touch взломано; #.md"
        } else {
            "\"; touch взломано; #.md"
        });
        std::fs::write(&nasty, "# всё нормально\n").unwrap();
        target.path = nasty.clone();

        let outcome = run(&spec("printf '%s' \"$KKNOW_PATH\""), &target).unwrap();

        assert_eq!(outcome.stdout, nasty.to_string_lossy());
        assert!(
            !dir.path().join("взломано").exists(),
            "хвост имени файла выполнился как команда"
        );
    }

    #[test]
    fn frontmatter_fields_arrive_as_variables() {
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        target
            .fields
            .insert("confluence".to_string(), "123456789".to_string());
        target
            .fields
            .insert("confluence-id".to_string(), "второй".to_string());
        target.fields.insert("!!!".to_string(), "мимо".to_string());
        target.fields.insert("2026".to_string(), "мимо".to_string());

        let env = environment(&target);

        assert_eq!(env["KKNOW_FM_CONFLUENCE"], "123456789");
        assert_eq!(env["KKNOW_FM_CONFLUENCE_ID"], "второй");
        assert!(!env.values().any(|value| value == "мимо"), "{env:?}");
    }

    #[test]
    fn a_command_shows_up_only_where_its_field_is_filled_in() {
        let dir = tempfile::tempdir().unwrap();
        let mut wants_id = spec("echo");
        wants_id.requires = vec!["confluence".to_string()];

        let bare = note(dir.path());
        let mut filled = note(dir.path());
        filled
            .fields
            .insert("confluence".to_string(), "123456789".to_string());
        let mut empty = note(dir.path());
        empty.fields.insert("confluence".to_string(), String::new());

        assert!(wants_id.applies_to(&filled));
        assert!(!wants_id.applies_to(&bare), "поля нет — команды нет");
        assert!(
            !wants_id.applies_to(&empty),
            "пустое значение — всё равно что отсутствующее"
        );
        assert!(matches!(
            run(&wants_id, &bare),
            Err(Error::NotApplicable { .. })
        ));
    }

    #[test]
    fn the_page_id_reaches_the_command_line_as_an_argument() {
        let dir = tempfile::tempdir().unwrap();
        let mut target = note(dir.path());
        target
            .fields
            .insert("confluence".to_string(), "123456789".to_string());

        let mut push = spec("printf '%s|%s' \"$KKNOW_FM_CONFLUENCE\" \"$KKNOW_PATH\"");
        push.requires = vec!["confluence".to_string()];

        let outcome = run(&push, &target).unwrap();

        assert_eq!(
            outcome.stdout,
            format!("123456789|{}", target.path.display())
        );
    }

    #[test]
    fn a_login_shell_sees_the_path_of_the_one_who_configured_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut plain = spec("printf '%s' \"$PATH\"");
        plain
            .env
            .insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        let mut login = plain.clone();
        login.login = true;

        let bare = run(&plain, &note(dir.path())).unwrap();
        let profile = run(&login, &note(dir.path())).unwrap();

        assert!(
            bare.stdout.contains("/usr/bin:/bin"),
            "заданный PATH дошёл до скрипта: {}",
            bare.stdout
        );
        assert!(
            profile.stdout.len() >= bare.stdout.len(),
            "профиль не сделал PATH короче: {}",
            profile.stdout
        );
    }

    #[test]
    fn the_exit_code_and_stderr_come_back() {
        let dir = tempfile::tempdir().unwrap();

        let outcome = run(&spec("echo беда >&2; exit 3"), &note(dir.path())).unwrap();

        assert!(!outcome.ok());
        assert_eq!(outcome.code, Some(3));
        assert_eq!(outcome.stderr.trim(), "беда");
        assert_eq!(outcome.end, RunEnd::Exited);
    }

    #[test]
    fn work_starts_in_the_vault_by_default_and_may_start_at_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = note(dir.path());

        let at_vault = run(&spec("printf x > от-корня"), &target).unwrap();
        let mut at_target = spec("printf x > от-заметки");
        at_target.cwd = Cwd::Target;
        let at_note = run(&at_target, &target).unwrap();

        assert!(at_vault.ok(), "{at_vault:?}");
        assert!(at_note.ok(), "{at_note:?}");
        assert!(
            dir.path().join("от-корня").exists(),
            "по умолчанию работа начинается в корне хранилища"
        );
        assert!(
            dir.path().join("Docs").join("от-заметки").exists(),
            "с Cwd::Target работа начинается в папке заметки"
        );
    }

    #[test]
    fn a_hanging_command_is_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("начали");
        let script = format!("echo начали; touch '{}'; sleep 30", mark.display());

        for timeout in [1, 3, 10] {
            let mut hanging = spec(&script);
            hanging.timeout_seconds = timeout;

            let outcome = run(&hanging, &note(dir.path())).unwrap();

            assert_eq!(outcome.end, RunEnd::TimedOut, "{outcome:?}");
            assert_eq!(outcome.code, None, "процесс убит, кода возврата у него нет");
            assert!(
                outcome.millis < timeout * 1000 + 4000,
                "ждали не дольше отпущенного: {outcome:?}"
            );
            if !mark.exists() {
                continue;
            }
            assert!(
                outcome.stdout.contains("начали"),
                "успевшее напечататься — есть: {outcome:?}"
            );
            return;
        }
        panic!("оболочка не дошла до echo и за 10 секунд");
    }

    #[test]
    fn a_talkative_command_does_not_wedge_and_does_not_eat_memory() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run(
            &spec("yes 'строка подлиннее' | head -c 200000; echo; echo хвост >&2"),
            &note(dir.path()),
        )
        .unwrap();

        assert!(outcome.ok(), "{:?}", outcome.stderr);
        assert!(outcome.truncated);
        assert_eq!(outcome.stdout.len(), CAPTURE_LIMIT);
        assert_eq!(outcome.stderr.trim(), "хвост");
    }

    fn watched(script: &str, stop: &AtomicBool) -> (Outcome, Vec<(Stream, String)>) {
        let dir = tempfile::tempdir().unwrap();
        let lines = std::sync::Mutex::new(Vec::new());
        let watch = Watch {
            line: &|stream, text| lines.lock().unwrap().push((stream, text.to_string())),
            stop,
        };
        let outcome =
            run_watched(&spec(script), &note(dir.path()), &BTreeMap::new(), &watch).unwrap();
        (outcome, lines.into_inner().unwrap())
    }

    #[test]
    fn each_line_comes_with_its_stream() {
        let stop = AtomicBool::new(false);

        let (outcome, lines) = watched("echo раз; echo беда >&2; printf 'два\\nбез конца'", &stop);

        assert!(outcome.ok(), "{outcome:?}");
        let out: Vec<&str> = lines
            .iter()
            .filter(|(stream, _)| *stream == Stream::Stdout)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(out, ["раз", "два", "без конца"]);
        assert!(lines.contains(&(Stream::Stderr, "беда".to_string())));
        assert_eq!(outcome.stdout, "раз\nдва\nбез конца");
    }

    #[test]
    fn a_line_comes_while_the_command_still_runs() {
        let dir = tempfile::tempdir().unwrap();
        let stop = AtomicBool::new(false);
        let watch = Watch {
            line: &|_, text| {
                if text == "начали" {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            stop: &stop,
        };

        let outcome = run_watched(
            &spec("echo начали; sleep 30"),
            &note(dir.path()),
            &BTreeMap::new(),
            &watch,
        )
        .unwrap();

        assert_eq!(outcome.end, RunEnd::Stopped, "{outcome:?}");
        assert_eq!(outcome.code, None);
        assert!(outcome.millis < 20_000, "{outcome:?}");
        assert_eq!(outcome.stdout, "начали\n");
    }

    #[test]
    fn a_stop_kills_the_whole_group() {
        let stop = AtomicBool::new(true);

        let (outcome, _) = watched("sleep 30 | cat", &stop);

        assert_eq!(outcome.end, RunEnd::Stopped, "{outcome:?}");
        assert!(outcome.millis < 20_000, "{outcome:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_does_not_wait_for_a_stranger_that_holds_the_pipe() {
        let stop = AtomicBool::new(false);
        let dir = tempfile::tempdir().unwrap();
        let watch = Watch {
            line: &|_, text| {
                if text == "начали" {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            stop: &stop,
        };

        let outcome = run_watched(
            &spec(r#"perl -e '$| = 1; setpgrp(0, 0); print "начали\n"; sleep 20' & sleep 30"#),
            &note(dir.path()),
            &BTreeMap::new(),
            &watch,
        )
        .unwrap();

        assert_eq!(outcome.end, RunEnd::Stopped, "{outcome:?}");
        assert!(outcome.millis < 10_000, "{outcome:?}");
        assert_eq!(outcome.stdout, "начали\n");
    }

    #[test]
    fn the_end_of_the_process_does_not_cut_the_output() {
        let stop = AtomicBool::new(false);

        let (outcome, _) = watched("(sleep 0.2; echo поздно) & seq 1 5000", &stop);

        assert!(outcome.ok(), "{outcome:?}");
        assert!(
            outcome.stdout.contains("\n5000\n"),
            "{}",
            outcome.stdout.len()
        );
        assert!(outcome.stdout.ends_with("поздно\n"), "{outcome:?}");
    }

    #[test]
    fn a_cut_output_gives_only_the_kept_lines() {
        let stop = AtomicBool::new(false);

        let (outcome, lines) = watched("yes 'строка' | head -n 100000", &stop);

        assert!(outcome.truncated);
        let joined: Vec<&str> = lines.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined.join("\n"), outcome.stdout);
        assert!(lines.len() < 100_000);
    }

    #[test]
    fn the_shared_environment_of_the_file_reaches_every_command() {
        let dir = tempfile::tempdir().unwrap();
        let set = CommandSet::parse(
            r#"{
                 "env": {"SPACE": "DOCS", "SERVER": "wiki.example"},
                 "commands": [{
                   "id":"проба","name":"Проба","kind":"shell",
                   "script":"printf '%s|%s|%s' \"$SPACE\" \"$SERVER\" \"$KKNOW_TARGET\"",
                   "env": {"SERVER": "своё"}}]}"#,
        )
        .unwrap();

        let outcome = set.run("проба", &note(dir.path())).unwrap();

        assert_eq!(outcome.stdout, "DOCS|своё|note");
    }

    #[test]
    fn a_shared_environment_cannot_shadow_the_computed_variables() {
        let dir = tempfile::tempdir().unwrap();
        let set = CommandSet::parse(
            r#"{
                 "env": {"KKNOW_TARGET": "подмена"},
                 "commands": [{"id":"проба","name":"Проба","kind":"shell",
                   "script":"printf '%s' \"$KKNOW_TARGET\""}]}"#,
        )
        .unwrap();

        assert_eq!(set.run("проба", &note(dir.path())).unwrap().stdout, "note");
    }

    #[test]
    fn a_command_cannot_be_run_outside_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let mut narrow = spec("echo");
        narrow.targets = vec![TargetKind::Vault];

        assert!(matches!(
            run(&narrow, &note(dir.path())),
            Err(Error::NotApplicable { .. })
        ));
    }

    #[test]
    fn the_manifest_may_add_variables_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut with_env = spec("printf '%s' \"$SPACE\"");
        with_env.env.insert("SPACE".to_string(), "DOCS".to_string());

        let outcome = run(&with_env, &note(dir.path())).unwrap();

        assert_eq!(outcome.stdout, "DOCS");
    }

    #[test]
    fn python_runs_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut python = spec("import os; print(os.environ['KKNOW_NOTE'])");
        python.kind = "python".to_string();

        match run(&python, &note(dir.path())) {
            Ok(outcome) => {
                assert!(outcome.ok(), "{:?}", outcome.stderr);
                assert_eq!(outcome.stdout.trim(), "Docs/Index");
            }
            Err(Error::Spawn { .. }) => {}
            Err(other) => panic!("{other}"),
        }
    }
}
