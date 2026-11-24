use kknow_app::Workspace;
use kknow_app::proposals::{Review, Status as ProposalStatus};

use crate::cli::*;
use crate::error::Result;

pub(crate) fn proposals(workspace: &mut Workspace, action: Option<ProposalAction>) -> Result<()> {
    match action.unwrap_or(ProposalAction::List { all: false }) {
        ProposalAction::List { all } => {
            let list: Vec<_> = workspace
                .proposals()?
                .into_iter()
                .filter(|proposal| all || proposal.status() == ProposalStatus::Pending)
                .collect();
            if list.is_empty() {
                println!("no proposals");
            }
            for proposal in list {
                println!("{}", proposal.describe());
            }
        }
        ProposalAction::Accept { id, only, feedback } => {
            let proposal = workspace.proposal(&id)?;
            let review = Review::accept_only(&proposal, &only, feedback);
            let (proposal, _) = workspace.review_proposal(&id, &review)?;
            println!("{}", proposal.describe());
        }
        ProposalAction::Reject { id, feedback } => {
            let proposal = workspace.proposal(&id)?;
            let review = Review::reject_all(&proposal, feedback);
            let (proposal, _) = workspace.review_proposal(&id, &review)?;
            println!("{}", proposal.describe());
        }
    }
    Ok(())
}
