use std::path::Path;

use kknow_core::NoteId;
use kknow_sync::{
    Fetched, Git, GitReport, Merged, Plan as SyncPlan, Report as SyncReport, Side, Staged,
    SyncEngine, SyncProvider,
};

use crate::watch::VaultEvent;
use crate::*;

impl Workspace {
    pub fn sync<P: SyncProvider>(&mut self, engine: &SyncEngine<P>) -> Result<SyncReport> {
        let report = engine.run(&mut self.vault)?;
        self.absorb(&report)?;
        Ok(report)
    }

    pub fn sync_stage<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<Staged> {
        Ok(engine.stage(&self.vault)?)
    }

    pub fn sync_finish<P: SyncProvider>(
        &mut self,
        engine: &SyncEngine<P>,
        fetched: Fetched,
    ) -> Result<SyncReport> {
        let report = engine.commit_local(&mut self.vault, fetched)?;
        self.absorb(&report)?;
        Ok(report)
    }

    pub(crate) fn absorb(&mut self, report: &SyncReport) -> Result<()> {
        let events: Vec<VaultEvent> = report
            .local_updated()
            .map(|id| VaultEvent::Changed(id.clone()))
            .chain(
                report
                    .local_removed()
                    .map(|id| VaultEvent::Removed(id.clone())),
            )
            .collect();
        self.apply(&events)?;
        if report
            .files_local
            .iter()
            .any(|key| key == bookmarks::BOOKMARKS_PATH)
        {
            self.load_bookmarks()?;
        }
        Ok(())
    }

    pub fn git_merge(&mut self, git: &Git) -> Result<Merged> {
        let merged = git.merge()?;
        if !merged.changed.is_empty() {
            self.reload()?;
        }
        Ok(merged)
    }

    pub fn git_discard(
        &mut self,
        git: &Git,
        path: &str,
        from: Option<&str>,
        side: Side,
    ) -> Result<()> {
        let relative = Path::new(path);
        if !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(kknow_core::Error::OutsideVault(relative.to_path_buf()).into());
        }
        if self.vault.root().join(relative).is_file() {
            self.trash().put(relative)?;
        }
        git.discard(path, from, side)?;
        self.reload()
    }

    pub fn git_sync(&mut self, git: &Git) -> Result<GitReport> {
        git.fetch()?;
        let merged = self.git_merge(git)?;
        let pushed = git.push()?;
        Ok(GitReport { merged, pushed })
    }

    pub fn conflicts(&self) -> Vec<&NoteId> {
        self.parsed
            .keys()
            .filter(|id| kknow_sync::is_conflict_copy(id))
            .collect()
    }

    pub fn sync_plan<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<SyncPlan> {
        Ok(engine.plan(&self.vault)?)
    }
}
