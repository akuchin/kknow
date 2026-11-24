use std::path::{Path, PathBuf};

use kknow_app::vaults::{Scope, VaultRegistry, select};

use crate::cli::*;
use crate::error::{CliError, Result};

pub(crate) fn load_registry() -> Result<(VaultRegistry, PathBuf)> {
    let dir = kknow_app::vaults::config_dir().ok_or(CliError::NoConfigFolder)?;
    let loaded = kknow_app::vaults::open(&dir)?;
    if let Some(problem) = loaded.problem {
        eprintln!("warning: {problem}");
    }
    Ok((loaded.registry, VaultRegistry::path(dir)))
}

pub(crate) fn vault_path(wanted: &str) -> Result<PathBuf> {
    let path = PathBuf::from(wanted);
    if path.is_dir() {
        return Ok(path);
    }

    let (registry, _) = load_registry()?;
    select(registry.entries(), &Scope::Vault(wanted.to_string()))
        .ok()
        .and_then(|found| found.first().map(|entry| entry.path.clone()))
        .ok_or_else(|| CliError::NotAVault(wanted.to_string()))
}

pub(crate) fn vault_identity(path: &Path) -> (String, String) {
    let name = path
        .canonicalize()
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let found = load_registry().ok().and_then(|(registry, _)| {
        let canonical = path.canonicalize().ok()?;
        registry
            .entries()
            .iter()
            .find(|entry| entry.path == canonical)
            .map(|entry| (entry.id.clone(), entry.name.clone()))
    });

    found.unwrap_or((name.clone(), name))
}

pub(crate) fn vaults(action: VaultAction) -> Result<()> {
    let (mut registry, path) = load_registry()?;

    match action {
        VaultAction::List => {}
        VaultAction::Add { path: folder, name } => {
            let added = registry.add(&folder, name)?;
            println!("added vault {} ({})", added.name, added.id);
            registry.save(&path)?;
        }
        VaultAction::Rm { vault } => {
            let id = id_of(&registry, &vault)?;
            let removed = registry.remove(&id)?;
            println!(
                "vault {} is removed from the list; the files stay in {}",
                removed.name,
                removed.path.display()
            );
            registry.save(&path)?;
        }
        VaultAction::Use { vault } => {
            let id = id_of(&registry, &vault)?;
            registry.set_active(&id)?;
            registry.save(&path)?;
        }
        VaultAction::Label { vault, labels } => {
            let id = id_of(&registry, &vault)?;
            registry.set_labels(&id, labels.iter().map(String::as_str))?;
            registry.save(&path)?;
        }
    }

    show_vaults(&registry);
    Ok(())
}

pub(crate) fn id_of(registry: &VaultRegistry, wanted: &str) -> Result<String> {
    let found = select(registry.entries(), &Scope::Vault(wanted.to_string()))?;
    Ok(found[0].id.clone())
}

pub(crate) fn show_vaults(registry: &VaultRegistry) {
    if registry.is_empty() {
        println!("the vault list is empty: kknow vaults add <folder>");
        return;
    }

    let active = registry.active().map(|entry| entry.id.clone());
    for (index, entry) in registry.entries().iter().enumerate() {
        let mark = if active.as_deref() == Some(entry.id.as_str()) {
            "*"
        } else {
            " "
        };
        let hotkey = if index < kknow_app::vaults::HOTKEYS {
            format!("⌘{}", index + 1)
        } else {
            "  ".to_string()
        };
        let state = if entry.available() {
            ""
        } else {
            "  (folder not available)"
        };

        let labels: String = entry
            .labels
            .iter()
            .map(|label| format!("  #{label}"))
            .collect();
        println!(
            "{mark} {hotkey} {:<20} {}{labels}{state}",
            entry.name,
            entry.path.display()
        );
    }
}
