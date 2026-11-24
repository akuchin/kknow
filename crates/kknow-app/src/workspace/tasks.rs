use kknow_core::NoteId;
use kknow_index::{AgentComment, Task};

use crate::*;

impl Workspace {
    pub fn tasks(&self) -> Vec<(&NoteId, &Task)> {
        self.parsed
            .iter()
            .flat_map(|(id, note)| note.tasks.iter().map(move |task| (id, task)))
            .collect()
    }

    pub fn set_task(&mut self, id: &NoteId, line: usize, done: bool) -> Result<String> {
        let content = self.read(id)?;
        self.write_task(id, content, line, done)
    }

    pub fn set_task_unlocked(&mut self, id: &NoteId, line: usize, done: bool) -> Result<String> {
        let content = self.open_text(id)?;
        self.write_task(id, content, line, done)
    }

    pub(crate) fn write_task(
        &mut self,
        id: &NoteId,
        content: String,
        line: usize,
        done: bool,
    ) -> Result<String> {
        let changed = kknow_index::parse::set_task(&content, line, done)
            .ok_or_else(|| Error::NotATask(id.clone(), line))?;
        if changed != content {
            self.save(id, &changed)?;
        }
        Ok(changed)
    }

    pub fn agent_comments(&self) -> Vec<(&NoteId, &AgentComment)> {
        self.parsed
            .iter()
            .flat_map(|(id, note)| note.agent_comments.iter().map(move |comment| (id, comment)))
            .collect()
    }

    pub fn resolve_agent_comment(
        &mut self,
        id: &NoteId,
        line: usize,
        text: &str,
    ) -> Result<String> {
        let content = self.read(id)?;
        let changed = kknow_index::parse::remove_agent_comment(&content, line, text)
            .ok_or_else(|| Error::NotAnAgentComment(id.clone(), line, text.trim().to_string()))?;
        self.save(id, &changed)?;
        Ok(changed)
    }
}
