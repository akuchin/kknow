use std::time::{Duration, SystemTime};

use kknow_app::Workspace;

use crate::cli::*;
use crate::error::Result;

pub(crate) fn trash(workspace: &mut Workspace, action: TrashAction) -> Result<()> {
    match action {
        TrashAction::List => {
            let entries = workspace.trash().entries()?;
            if entries.is_empty() {
                println!("the trash is empty");
            }
            let now = SystemTime::now();
            for entry in entries {
                let age = now.duration_since(entry.deleted).unwrap_or_default();
                println!(
                    "{}  {}  ({}, notes: {})",
                    entry.id,
                    entry.path,
                    days_ago(age),
                    entry.notes.len()
                );
            }
        }
        TrashAction::Restore { entry } => {
            let restored = workspace.restore(&entry)?;
            for id in &restored {
                println!("restored {id}");
            }
        }
        TrashAction::Empty => {
            let count = workspace.trash().empty()?;
            println!("deleted from the trash forever: {count}");
        }
    }
    Ok(())
}

pub(crate) fn days_ago(age: Duration) -> String {
    match age.as_secs() / (24 * 60 * 60) {
        0 => "today".to_string(),
        days => format!("{days} d ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_age_is_in_whole_days() {
        assert_eq!(days_ago(Duration::from_secs(60)), "today");
        assert_eq!(
            days_ago(Duration::from_secs(3 * 24 * 60 * 60 + 5)),
            "3 d ago"
        );
    }
}
