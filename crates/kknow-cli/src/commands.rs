use std::process::ExitCode;

use kknow_run::{CommandSet, CommandSpec, Outcome, RunEnd, Target, TargetKind};

use crate::error::{CliError, Result};

pub(crate) fn commands(target: Option<Target>, example: bool) -> Result<()> {
    let path = kknow_app::commands::config_path().ok_or(CliError::NoConfigFolder)?;

    if example {
        if path.exists() {
            return Err(CliError::FileExists(path));
        }
        kknow_app::commands::save_manifest(&path, &kknow_app::commands::defaults())?;
        println!("default commands written: {}", path.display());
    }

    let set = CommandSet::load(&path, kknow_app::commands::defaults())?;

    let picked: Vec<&CommandSpec> = if let Some(target) = &target {
        let kind = if target.standalone() {
            "file outside a vault"
        } else {
            of(target.kind)
        };
        println!("target: {} ({kind})", target.path.display());
        set.for_target(target)
    } else {
        set.commands.iter().collect()
    };

    if picked.is_empty() && set.problems.is_empty() {
        if set.is_empty() {
            println!("no commands: file {} is empty", path.display());
        } else {
            println!(
                "none of the {} commands in the file apply to this target",
                set.commands.len()
            );
            println!("`kknow commands` without a target shows which command applies where");
        }
        return Ok(());
    }

    for spec in picked {
        println!("{}  «{}» ({})", spec.id, spec.name, spec.kind);
        println!("     {}", scope(spec));
    }

    for problem in &set.problems {
        println!("not a command: {problem}");
    }
    Ok(())
}

pub(crate) fn scope(spec: &CommandSpec) -> String {
    let kinds: Vec<&str> = spec.targets.iter().map(|kind| of(*kind)).collect();
    let mut parts = vec![format!("targets: {}", kinds.join(", "))];

    if !spec.vaults.is_empty() {
        parts.push(format!("vaults: {}", spec.vaults.join(", ")));
    }
    if !spec.patterns.is_empty() {
        parts.push(format!("paths: {}", spec.patterns.join(", ")));
    }
    if !spec.requires.is_empty() {
        parts.push(format!("requires fields: {}", spec.requires.join(", ")));
    }
    parts.join(" · ")
}

pub(crate) fn of(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Note => "note",
        TargetKind::Folder => "folder",
        TargetKind::Vault => "vault",
    }
}

pub(crate) fn run_command(command: &str, target: &Target) -> Result<ExitCode> {
    let set = kknow_app::commands::load()?;
    if set.get(command).is_none() {
        return Err(CliError::CommandNotFound(command.to_string()));
    }

    let outcome = set.run(command, target)?;
    report(&outcome)
}

pub(crate) fn report(outcome: &Outcome) -> Result<ExitCode> {
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);

    if outcome.truncated {
        eprintln!("(output is cut: only the start is shown)");
    }
    if outcome.end == RunEnd::TimedOut {
        eprintln!("the command ran out of time and was stopped");
        return Ok(ExitCode::from(124));
    }

    match outcome.code {
        Some(0) => Ok(ExitCode::SUCCESS),
        Some(code) => Ok(ExitCode::from(u8::try_from(code).unwrap_or(1))),
        None => Err(CliError::StoppedBySignal),
    }
}
