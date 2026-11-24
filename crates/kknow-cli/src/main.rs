use std::process::ExitCode;

use clap::Parser;
use kknow_app::Workspace;

use crate::cli::{Cli, Command, SyncAction, TrashAction, VaultAction};
use crate::commands::{commands, run_command};
use crate::error::Result;
use crate::lock::{lock, unlock};
use crate::marks::{bookmark, bookmarks, tasks};
use crate::notes::{
    backlinks, broken, links, list, orphans, render, search, show, stats, tags,
    warn_about_duplicates,
};
use crate::observe::{themes, watch};
use crate::proposals::proposals;
use crate::syncing::{change_passphrase, destination, encrypt, sync};
use crate::trash::trash;
use crate::vaults::{vault_identity, vault_path, vaults};

mod cli;
mod commands;
mod error;
mod lock;
mod marks;
mod mcp;
mod notes;
mod observe;
mod proposals;
mod secret;
mod syncing;
mod trash;
mod vaults;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    if let Command::Vaults { action } = cli.command {
        vaults(action.unwrap_or(VaultAction::List))?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::Mcp { labels } = &cli.command {
        mcp::run(&cli.vault, labels)?;
        return Ok(ExitCode::SUCCESS);
    }

    match &cli.command {
        Command::Commands {
            file: Some(file),
            example,
            ..
        } => {
            let target = kknow_app::commands::file_target(file)?;
            commands(Some(target), *example)?;
            return Ok(ExitCode::SUCCESS);
        }
        Command::Run {
            command,
            file: Some(file),
            ..
        } => {
            let target = kknow_app::commands::file_target(file)?;
            return run_command(command, &target);
        }
        _ => {}
    }

    let path = vault_path(&cli.vault)?;
    let (id, name) = vault_identity(&path);
    let mut workspace = Workspace::open(&path)?;

    let mut code = ExitCode::SUCCESS;
    match cli.command {
        Command::List => list(&workspace),
        Command::Stats => stats(&workspace),
        Command::Links { note } => links(&workspace, &note)?,
        Command::Backlinks { note } => backlinks(&workspace, &note)?,
        Command::Tags => tags(&workspace),
        Command::Broken => broken(&workspace),
        Command::Orphans => orphans(&workspace),
        Command::Show { note } => show(&mut workspace, &note)?,
        Command::Render { note } => render(&workspace, &note)?,
        Command::New { name } => {
            let id = workspace.create(&name)?;
            warn_about_duplicates(&workspace);
            println!("created note {id}");
        }
        Command::Lock { note } => lock(&mut workspace, &note)?,
        Command::Unlock { note } => unlock(&mut workspace, &note)?,
        Command::Search { query, limit } => search(&workspace, &query.join(" "), limit)?,
        Command::Themes => themes(&workspace),
        Command::Bookmarks => bookmarks(&workspace),
        Command::Bookmark { note, line, name } => {
            bookmark(&mut workspace, &note, line, name.as_deref())?
        }
        Command::Tasks { all } => tasks(&workspace, all),
        Command::Proposals { action } => proposals(&mut workspace, action)?,
        Command::Commands {
            target,
            kind,
            example,
            ..
        } => {
            let target = if target.is_some() || kind.is_some() {
                let target = kknow_app::commands::resolve(
                    &workspace,
                    &id,
                    &name,
                    kind.map(Into::into),
                    target.as_deref(),
                )?;
                Some(target)
            } else {
                None
            };
            commands(target, example)?
        }
        Command::Run {
            command,
            target,
            kind,
            ..
        } => {
            let target = kknow_app::commands::resolve(
                &workspace,
                &id,
                &name,
                kind.map(Into::into),
                target.as_deref(),
            )?;
            code = run_command(&command, &target)?
        }
        Command::Vaults { .. } => unreachable!(),
        Command::Trash { action } => trash(&mut workspace, action.unwrap_or(TrashAction::List))?,
        Command::Watch => watch(&mut workspace)?,
        Command::Mcp { .. } => unreachable!(),
        Command::Sync {
            dry_run,
            on_conflict,
            folder,
            s3,
            action,
        } => {
            let mut config = destination(&workspace, folder, s3)?;
            if let Some(choice) = on_conflict {
                config.policy = choice.into();
            }
            match action {
                None => sync(&mut workspace, config, dry_run)?,
                Some(SyncAction::Encrypt) => encrypt(&workspace, config)?,
                Some(SyncAction::Passphrase) => change_passphrase(&config)?,
            }
        }
    }

    Ok(code)
}
