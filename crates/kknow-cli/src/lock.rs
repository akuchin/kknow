use kknow_app::Workspace;

use crate::error::Result;
use crate::notes::*;
use crate::secret::*;

pub(crate) const LOCK_VARIABLE: &str = "KKNOW_LOCK_PASSPHRASE";

pub(crate) fn open_lock(workspace: &mut Workspace) -> Result<()> {
    if workspace.lock_state().password {
        let typed = secret(
            LOCK_VARIABLE,
            "Vault password or recovery code: ",
            "the note needs the vault password",
        )?;
        workspace.unlock_notes(&typed)?;
        return Ok(());
    }
    println!("The vault has no password for locked notes yet.");
    let password = new_secret(LOCK_VARIABLE, "password", "the vault needs a password")?;
    let code = workspace.set_lock_password(&password)?;
    println!();
    println!("Recovery code: {}", code.display().as_str());
    println!();
    println!("Write it down and keep it in a safe place. It opens the locked notes if");
    println!("you lose the password. kknow shows it only now. Without the password and");
    println!("the code, nobody can read the locked notes, and neither can kknow.");
    println!();
    Ok(())
}

pub(crate) fn lock(workspace: &mut Workspace, raw: &str) -> Result<()> {
    let id = find(workspace, raw)?.clone();
    if workspace.is_locked(&id) {
        return Err(kknow_app::Error::AlreadyLocked(id).into());
    }
    open_lock(workspace)?;
    workspace.lock_note(&id)?;
    println!("locked: {id}");
    Ok(())
}

pub(crate) fn unlock(workspace: &mut Workspace, raw: &str) -> Result<()> {
    let id = find(workspace, raw)?.clone();
    if !workspace.is_locked(&id) {
        return Err(kknow_app::Error::NotLocked(id).into());
    }
    open_lock(workspace)?;
    workspace.remove_lock(&id)?;
    println!("unlocked: {id}");
    Ok(())
}
