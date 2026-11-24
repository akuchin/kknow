use kknow_app::Workspace;
use kknow_app::theme::{self, Base, Source};

use crate::error::Result;

pub(crate) fn themes(workspace: &Workspace) {
    let global = kknow_app::vaults::config_dir().map(theme::global_dir);
    let set = workspace.themes(global.as_deref());

    if set.themes.is_empty() && set.problems.is_empty() {
        match &global {
            Some(global) => println!(
                "no themes: put a manifest in {} or in {}",
                theme::THEMES_DIR,
                global.display()
            ),
            None => println!("no themes: put a manifest in {}", theme::THEMES_DIR),
        }
        return;
    }

    for theme in &set.themes {
        let base = match theme.base {
            Base::Light => "light",
            Base::Dark => "dark",
        };
        let source = match theme.source {
            Source::Vault => "vault",
            Source::Global => "shared",
            Source::BuiltIn => "built-in",
        };
        println!(
            "{}  \"{}\" ({base}, {source}), variables: {}",
            theme.id,
            theme.name,
            theme.variables.len()
        );
        for warning in &theme.warnings {
            println!("     ⚠ {warning}");
        }
    }

    for problem in &set.problems {
        println!("not a theme: {problem}");
    }
}

pub(crate) fn watch(workspace: &mut Workspace) -> Result<()> {
    let watcher = workspace.watch()?;
    println!(
        "watching {}; press Ctrl+C to stop",
        workspace.root().display()
    );

    while let Some(events) = watcher.recv() {
        let changes = workspace.apply(&events)?;
        for id in &changes.updated {
            println!("changed  {id}");
        }
        for id in &changes.removed {
            println!("deleted  {id}");
        }
    }

    Ok(())
}
