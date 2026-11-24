use std::path::PathBuf;

use kknow_app::Workspace;
use kknow_app::git_client::git_client;
use kknow_sync::{GitSettings, Provider, RemoteConfig, S3Settings};

use crate::cli::*;
use crate::error::{CliError, Result};
use crate::secret::*;

pub(crate) fn destination(
    workspace: &Workspace,
    folder: Option<PathBuf>,
    s3: S3Args,
) -> Result<RemoteConfig> {
    if let Some(path) = folder {
        return Ok(RemoteConfig::folder(path));
    }

    if let Some(bucket) = s3.bucket {
        return Ok(RemoteConfig::s3(S3Settings {
            bucket,
            endpoint: s3.endpoint,
            region: s3.region,
            prefix: s3.prefix,
            access_key_id: std::env::var("KKNOW_S3_ACCESS_KEY").ok(),
        }));
    }

    RemoteConfig::load(&RemoteConfig::path(workspace.root()))?.ok_or(CliError::NoStorage)
}

pub(crate) fn sync(
    workspace: &mut Workspace,
    mut config: RemoteConfig,
    dry_run: bool,
) -> Result<()> {
    println!("storage: {}", config.location());

    if let Provider::Git(settings) = &config.provider {
        return sync_git(workspace, settings, dry_run);
    }

    let secret = s3_secret(&config);

    let storage = kknow_sync::storage(&config, secret.clone())?;
    let encrypted = config.encrypted || kknow_sync::encrypted::is_encrypted(&*storage)?;
    let key = if encrypted {
        let passphrase = passphrase("KKNOW_SYNC_PASSPHRASE", "Passphrase or recovery code: ")?;
        config.encrypted = true;
        Some(kknow_sync::encrypted::unlock(&*storage, &passphrase)?)
    } else {
        None
    };

    let engine = kknow_app::merge::sync_engine(&config, secret, key.as_ref())?;

    if dry_run {
        let plan = workspace.sync_plan(&engine)?;
        if plan.is_empty() {
            println!("everything is in sync");
        } else {
            print!("{plan}");
        }
        return Ok(());
    }

    let report = workspace.sync(&engine)?;
    if report.is_empty() {
        println!("everything is in sync");
        return Ok(());
    }

    let counts = [
        ("uploaded", report.uploaded.len()),
        ("downloaded", report.downloaded.len()),
        ("moved to trash", report.removed_local.len()),
        ("deleted in the cloud", report.removed_remote.len()),
    ];
    for (label, count) in counts.iter().filter(|(_, count)| *count > 0) {
        println!("{label}: {count}");
    }

    for conflict in &report.conflicts {
        println!(
            "conflict: {}; the other version is saved as {}",
            conflict.id, conflict.copy
        );
    }

    Ok(())
}

pub(crate) fn s3_secret(config: &RemoteConfig) -> Option<String> {
    match &config.provider {
        Provider::S3(_) => std::env::var("KKNOW_S3_SECRET_KEY").ok(),
        Provider::Folder { .. } | Provider::Git(_) => None,
    }
}

pub(crate) fn encrypt(workspace: &Workspace, mut config: RemoteConfig) -> Result<()> {
    println!("storage: {}", config.location());
    let storage = kknow_sync::storage(&config, s3_secret(&config))?;
    let passphrase = new_passphrase("KKNOW_SYNC_PASSPHRASE")?;
    let (_, code) = kknow_sync::encrypted::init(&*storage, &passphrase)?;

    let path = RemoteConfig::path(workspace.root());
    let saved = RemoteConfig::load(&path)?;
    if saved.is_some_and(|saved| saved.location() == config.location()) {
        config.encrypted = true;
        config.save(&path)?;
    }

    println!("The storage is encrypted.");
    println!();
    println!("Recovery code: {}", code.display().as_str());
    println!();
    println!("Write it down and keep it in a safe place. It opens the storage if you");
    println!("lose the passphrase. kknow shows it only now. Without the passphrase and");
    println!("the code, nobody can read the notes, and neither can kknow.");
    Ok(())
}

pub(crate) fn change_passphrase(config: &RemoteConfig) -> Result<()> {
    println!("storage: {}", config.location());
    let storage = kknow_sync::storage(config, s3_secret(config))?;
    let current = passphrase(
        "KKNOW_SYNC_PASSPHRASE",
        "Current passphrase or recovery code: ",
    )?;
    let new = new_passphrase("KKNOW_SYNC_NEW_PASSPHRASE")?;
    kknow_sync::encrypted::change_passphrase(&*storage, &current, &new)?;
    println!("The passphrase is changed. Other devices keep working.");
    Ok(())
}

pub(crate) fn sync_git(
    workspace: &mut Workspace,
    settings: &GitSettings,
    dry_run: bool,
) -> Result<()> {
    let program = std::env::var("KKNOW_GIT").unwrap_or_else(|_| "git".to_string());
    let stored = std::env::var("KKNOW_GIT_TOKEN").ok();
    let git = git_client(program, workspace.root(), settings, stored)?;

    if dry_run {
        let status = git.status()?;
        if let Some(branch) = &status.branch {
            println!("branch: {branch}");
        }
        println!("commits not pushed: {}", status.ahead);
        if status.changes.is_empty() {
            println!("no uncommitted changes");
        }
        for change in &status.changes {
            let place = if change.staged.is_some() {
                "staged"
            } else {
                "not staged"
            };
            println!("{place}: {}", change.path);
        }
        return Ok(());
    }

    let report = workspace.git_sync(&git)?;
    if report.merged.changed.is_empty() && report.pushed == 0 {
        println!("everything is in sync");
    }
    if !report.merged.changed.is_empty() {
        println!("files changed: {}", report.merged.changed.len());
    }
    if report.pushed > 0 {
        println!("commits pushed: {}", report.pushed);
    }
    for conflict in &report.merged.conflicts {
        println!(
            "conflict: {}; the other version is saved as {}",
            conflict.path, conflict.copy
        );
    }
    Ok(())
}
