use std::path::{Path, PathBuf};

use kknow_sync::{Git, GitAuth, GitSettings};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitClientError {
    #[error("no token is set")]
    NoToken,

    #[error("variable {0} is not set")]
    VariableNotSet(String),
}

pub fn git_token(settings: &GitSettings, stored: Option<String>) -> Option<String> {
    match settings.auth {
        GitAuth::System => None,
        GitAuth::Token => stored,
        GitAuth::Env => settings.env.as_deref().and_then(kknow_run::login::variable),
    }
}

pub fn git_client(
    program: impl Into<PathBuf>,
    root: &Path,
    settings: &GitSettings,
    stored: Option<String>,
) -> Result<Git, GitClientError> {
    let token = git_token(settings, stored);
    if settings.auth != GitAuth::System && token.is_none() {
        return Err(match settings.auth {
            GitAuth::Env => GitClientError::VariableNotSet(
                settings
                    .env
                    .clone()
                    .unwrap_or_else(|| "with the token".to_string()),
            ),
            _ => GitClientError::NoToken,
        });
    }
    Ok(Git::new(program, root)
        .with_token(token)
        .with_merge(crate::merge::merge_shared))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(auth: GitAuth, env: Option<&str>) -> GitSettings {
        GitSettings {
            auth,
            env: env.map(str::to_string),
            ..GitSettings::default()
        }
    }

    fn client(settings: &GitSettings, stored: Option<&str>) -> Result<Git, GitClientError> {
        git_client(
            "git",
            Path::new("/vault"),
            settings,
            stored.map(str::to_string),
        )
    }

    #[test]
    fn system_auth_needs_no_token() {
        assert!(client(&settings(GitAuth::System, None), None).is_ok());
    }

    #[test]
    fn token_auth_needs_the_stored_token() {
        let token = settings(GitAuth::Token, None);
        assert!(client(&token, Some("secret")).is_ok());
        assert_eq!(client(&token, None).unwrap_err(), GitClientError::NoToken);
    }

    #[test]
    fn env_auth_needs_the_variable() {
        let unset = settings(GitAuth::Env, Some("KKNOW_TEST_GIT_TOKEN_UNSET"));
        assert_eq!(
            client(&unset, None).unwrap_err(),
            GitClientError::VariableNotSet("KKNOW_TEST_GIT_TOKEN_UNSET".to_string())
        );
        let set = settings(GitAuth::Env, Some("PATH"));
        assert!(client(&set, None).is_ok());
        let nameless = settings(GitAuth::Env, None);
        assert_eq!(
            client(&nameless, None).unwrap_err().to_string(),
            "variable with the token is not set"
        );
    }
}
