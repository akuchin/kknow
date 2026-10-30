//! Git from the window: the connection, the changes panel and the sync steps.
//!
//! The algorithm is in `kknow_sync::git`. This module finds the program (the
//! app settings), the token (the keychain or a variable) and the lock: the
//! commands that touch the index take the vault lock, so a commit never meets
//! a running merge, and a note save never meets the merge either.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kknow_core::NoteId;
use kknow_sync::{Git, GitAuth, GitSettings, GitStatus, Merged, Provider, RemoteConfig};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::live::{self, VaultChanged};
use crate::secrets;
use crate::settings::SettingsState;
use crate::state::{AppState, Error, Result, VaultSlot};
use crate::sync::{SyncSettings, settings_of};

/// The `git` program from the app settings. kknow does not guess it: the
/// settings have a "Find" button, and the person confirms the path.
fn program(settings: &SettingsState) -> Result<PathBuf> {
    settings
        .get()
        .git_path
        .map(PathBuf::from)
        .ok_or(Error::GitPathNotSet)
}

/// The token for the repository at `url`.
///
/// `typed` is a token that the person just typed in the form; it wins over
/// the keychain. A missing token for the "token" or "variable" way is an
/// error: without it git would fall back to its own sign-in, and the person
/// would not see why their choice did not work.
fn token(
    settings: &GitSettings,
    url: Option<&str>,
    typed: Option<String>,
) -> Result<Option<String>> {
    let stored = match (settings.auth, typed) {
        (GitAuth::Token, Some(typed)) => Some(typed),
        (GitAuth::Token, None) => match url {
            Some(url) => secrets::load(url)?,
            None => None,
        },
        _ => None,
    };
    match settings.auth {
        GitAuth::System => Ok(None),
        GitAuth::Token | GitAuth::Env => kknow_app::git_token(settings, stored)
            .map(Some)
            .ok_or(Error::NoGitToken),
    }
}

/// git for a vault with git settings.
pub fn open<R: Runtime>(app: &AppHandle<R>, root: &Path, config: &RemoteConfig) -> Result<Git> {
    let Provider::Git(settings) = &config.provider else {
        return Err(Error::NotConfigured);
    };
    let program = program(&app.state::<SettingsState>())?;
    let token = token(settings, Some(&settings.url), None)?;
    Ok(Git::new(program, root).with_token(token))
}

fn git_of<R: Runtime>(app: &AppHandle<R>, slot: &VaultSlot) -> Result<Git> {
    let root = slot.read(|workspace| Ok(workspace.root().to_path_buf()))?;
    let config = RemoteConfig::load(&RemoteConfig::path(&root))?.ok_or(Error::NotConfigured)?;
    open(app, &root, &config)
}

/// Tells the window which notes a merge or a checkout changed.
///
/// The workspace already read them again, so the watcher does not report them
/// as new: this event is the only news for the window.
pub fn announce<R: Runtime>(app: &AppHandle<R>, slot: &VaultSlot, paths: &[String]) {
    let root = &slot.entry.path;
    let mut changed = VaultChanged {
        vault: slot.entry.id.clone(),
        updated: Vec::new(),
        removed: Vec::new(),
    };
    for path in paths {
        let Some(id) = NoteId::from_relative_path(Path::new(path)) else {
            continue;
        };
        if root.join(path).is_file() {
            changed.updated.push(id.as_str().to_string());
        } else {
            changed.removed.push(id.as_str().to_string());
        }
    }
    if !changed.is_empty() {
        let _ = app.emit(live::EVENT, changed);
    }
}

/// Where git is on this machine: a suggestion for the settings field.
#[tauri::command(async)]
pub fn git_locate() -> Option<String> {
    kknow_run::locate("git").map(|path| path.to_string_lossy().into_owned())
}

/// `git version` of a path: the check that the field points to git.
#[tauri::command(async)]
pub fn git_version(path: String) -> Result<String> {
    Ok(Git::new(path, std::env::temp_dir()).version()?)
}

/// Connects the vault to a repository, or changes how it signs in.
///
/// `url` is needed only for an empty vault: a vault that is already a
/// repository takes the address of `origin`. `token` = `None` keeps the token
/// in the keychain, as the S3 secret field does.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn connect_git<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: State<'_, SettingsState>,
    vault: Option<String>,
    url: Option<String>,
    auth: GitAuth,
    env: Option<String>,
    token: Option<String>,
) -> Result<SyncSettings> {
    let slot = state.slot(vault.as_deref())?;
    let root = slot.entry.path.clone();
    let probe = Git::new(program(&settings)?, &root);

    let url = tidy(url);
    let known = if Git::is_repository(&root) {
        probe.origin()?.or(url.clone())
    } else {
        url.clone()
    };
    let wanted = GitSettings {
        url: known.clone().unwrap_or_default(),
        auth,
        env: tidy(env),
    };
    let typed = tidy(token);
    let git = probe.with_token(self::token(&wanted, known.as_deref(), typed.clone())?);

    // A checkout writes the notes of the remote into the vault: the vault
    // lock keeps autosave away for that time.
    let url = slot.write(|workspace| {
        let url = git.attach(url.as_deref())?;
        workspace.reload()?;
        Ok(url)
    })?;
    let notes: Vec<String> = slot.read(|workspace| {
        Ok(workspace
            .notes()
            .map(|(id, _)| id.to_relative_path().to_string_lossy().into_owned())
            .collect())
    })?;
    announce(&app, &slot, &notes);

    let config = RemoteConfig::git(GitSettings { url, ..wanted });
    config.save(&RemoteConfig::path(&root))?;
    if let (GitAuth::Token, Some(typed)) = (auth, typed) {
        secrets::store(&config.location(), &typed)?;
    }
    settings_of(config)
}

/// The changes panel: the branch, the distance to the remote and the files.
#[tauri::command(async)]
pub fn git_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<GitStatus> {
    let slot = state.slot(vault.as_deref())?;
    let git = git_of(&app, &slot)?;
    slot.read(|_| Ok(git.status()?))
}

#[tauri::command(async)]
pub fn git_stage<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
    paths: Vec<String>,
) -> Result<GitStatus> {
    let slot = state.slot(vault.as_deref())?;
    let git = git_of(&app, &slot)?;
    slot.read(|_| {
        git.stage(&paths)?;
        Ok(git.status()?)
    })
}

#[tauri::command(async)]
pub fn git_unstage<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
    paths: Vec<String>,
) -> Result<GitStatus> {
    let slot = state.slot(vault.as_deref())?;
    let git = git_of(&app, &slot)?;
    slot.read(|_| {
        git.unstage(&paths)?;
        Ok(git.status()?)
    })
}

/// The diff of one file for the changes view; `staged` picks the side.
#[tauri::command(async)]
pub fn git_diff<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
    path: String,
    from: Option<String>,
    staged: bool,
) -> Result<String> {
    let slot = state.slot(vault.as_deref())?;
    let git = git_of(&app, &slot)?;
    slot.read(|_| Ok(git.diff(&path, from.as_deref(), staged)?))
}

#[tauri::command(async)]
pub fn git_commit<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
    message: String,
) -> Result<GitStatus> {
    let slot = state.slot(vault.as_deref())?;
    let git = git_of(&app, &slot)?;
    slot.read(|_| {
        git.commit(&message)?;
        Ok(git.status()?)
    })
}

/// The git steps of a sync: fetch and push without the vault, the merge
/// with it. Gives the merge and the count of pushed commits.
pub fn exchange<R: Runtime>(
    app: &AppHandle<R>,
    slot: &Arc<VaultSlot>,
    config: &RemoteConfig,
) -> Result<(Merged, usize)> {
    let git = open(app, &slot.entry.path, config)?;
    git.fetch()?;
    let merged = slot.write(|workspace| Ok(workspace.git_merge(&git)?))?;
    announce(app, slot, &merged.changed);
    let pushed = git.push()?;
    Ok((merged, pushed))
}

fn tidy(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    use kknow_app::settings::AppSettings;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use crate::sync_status::SyncStatuses;

    /// git with the configuration of the person who runs the tests hidden,
    /// as in the core tests. The window passes the program path, so a wrapper
    /// script gives it the environment.
    struct World {
        dir: tempfile::TempDir,
        git: PathBuf,
        remote: PathBuf,
    }

    impl World {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().join("home");
            fs::create_dir_all(&home).unwrap();
            fs::write(home.join("gitconfig"), "").unwrap();
            let git = dir.path().join("git");
            fs::write(
                &git,
                format!(
                    "#!/bin/sh\nexport HOME='{0}' GIT_CONFIG_GLOBAL='{0}/gitconfig' GIT_CONFIG_NOSYSTEM=1\nexec git \"$@\"\n",
                    home.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();

            let remote = dir.path().join("remote.git");
            fs::create_dir_all(&remote).unwrap();
            std::process::Command::new(&git)
                .args(["init", "-q", "--bare"])
                .current_dir(&remote)
                .status()
                .unwrap();
            World { dir, git, remote }
        }

        fn vault(&self, name: &str) -> PathBuf {
            let root = self.dir.path().join(name);
            fs::create_dir_all(&root).unwrap();
            root
        }

        fn app(&self, root: &Path) -> tauri::App<MockRuntime> {
            let app = mock_builder()
                .manage(crate::state::testing(&[root]))
                .manage(SyncStatuses::default())
                .manage(SettingsState::default())
                .build(mock_context(noop_assets()))
                .unwrap();
            app.state::<SettingsState>().use_file(
                None,
                AppSettings {
                    git_path: Some(self.git.to_string_lossy().into_owned()),
                    ..AppSettings::default()
                },
            );
            app
        }

        fn connect(&self, app: &tauri::App<MockRuntime>) -> SyncSettings {
            connect_git(
                app.handle().clone(),
                app.state(),
                app.state(),
                None,
                Some(self.remote.to_string_lossy().into_owned()),
                GitAuth::System,
                None,
                None,
            )
            .unwrap()
        }
    }

    #[test]
    fn a_note_travels_from_one_window_to_another() {
        let world = World::new();
        let laptop_root = world.vault("laptop");
        let laptop = world.app(&laptop_root);
        world.connect(&laptop);

        fs::write(laptop_root.join("Index.md"), "# Оглавление\n").unwrap();
        let status = git_status(laptop.handle().clone(), laptop.state(), None).unwrap();
        let paths: Vec<String> = status.changes.iter().map(|c| c.path.clone()).collect();
        assert_eq!(paths, vec!["Index.md".to_string()], "служебное скрыто");

        git_stage(laptop.handle().clone(), laptop.state(), None, paths).unwrap();
        let status = git_commit(
            laptop.handle().clone(),
            laptop.state(),
            None,
            "оглавление".into(),
        )
        .unwrap();
        assert!(status.changes.is_empty());
        assert_eq!(status.ahead, 1);

        let report = crate::sync::sync_now(laptop.handle().clone(), laptop.state(), None).unwrap();
        assert_eq!(report.pushed, 1);

        let desk_root = world.vault("desk");
        let desk = world.app(&desk_root);
        let settings = world.connect(&desk);
        assert_eq!(settings.location(), world.remote.to_string_lossy());
        assert_eq!(
            fs::read_to_string(desk_root.join("Index.md")).unwrap(),
            "# Оглавление\n"
        );
        let opened = desk
            .state::<AppState>()
            .slot(None)
            .unwrap()
            .read(|workspace| Ok(workspace.notes().len()))
            .unwrap();
        assert_eq!(opened, 1, "заметка из репозитория уже в индексе");
    }

    #[test]
    fn a_token_way_without_a_token_says_so() {
        let world = World::new();
        let root = world.vault("laptop");
        let app = world.app(&root);

        let result = connect_git(
            app.handle().clone(),
            app.state(),
            app.state(),
            None,
            Some("https://example.com/нет-токена.git".into()),
            GitAuth::Token,
            None,
            None,
        );

        assert!(matches!(result, Err(Error::NoGitToken)));
        assert!(!root.join(".git").exists());
    }

    #[test]
    fn without_a_git_path_the_window_asks_for_it() {
        let world = World::new();
        let root = world.vault("laptop");
        let app = world.app(&root);
        app.state::<SettingsState>()
            .use_file(None, AppSettings::default());

        let result = connect_git(
            app.handle().clone(),
            app.state(),
            app.state(),
            None,
            Some(world.remote.to_string_lossy().into_owned()),
            GitAuth::System,
            None,
            None,
        );

        assert!(matches!(result, Err(Error::GitPathNotSet)));
    }

    #[test]
    fn the_typed_token_goes_to_the_keychain_not_to_the_vault() {
        let world = World::new();
        let root = world.vault("laptop");
        let app = world.app(&root);
        let url = world.remote.to_string_lossy().into_owned();

        let saved = connect_git(
            app.handle().clone(),
            app.state(),
            app.state(),
            None,
            Some(url.clone()),
            GitAuth::Token,
            None,
            Some("ghp_тайна".into()),
        )
        .unwrap();

        assert!(saved.has_secret());
        assert_eq!(secrets::load(&url).unwrap().as_deref(), Some("ghp_тайна"));
        let written = fs::read_to_string(root.join(".kknow/remote.json")).unwrap();
        assert!(!written.contains("тайна"), "{written}");
        let config = fs::read_to_string(root.join(".git/config")).unwrap();
        assert!(!config.contains("тайна"), "{config}");
    }
}
