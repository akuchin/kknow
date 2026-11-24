use std::io::IsTerminal;

use crate::error::{CliError, Result};

pub(crate) fn passphrase(variable: &str, prompt: &str) -> Result<String> {
    secret(variable, prompt, "the storage is encrypted")
}

pub(crate) fn secret(variable: &str, prompt: &str, reason: &'static str) -> Result<String> {
    if let Ok(value) = std::env::var(variable) {
        return Ok(value);
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliError::NoTerminal {
            reason,
            variable: variable.to_string(),
        });
    }
    Ok(rpassword::prompt_password(prompt)?)
}

pub(crate) fn new_passphrase(variable: &str) -> Result<String> {
    new_secret(variable, "passphrase", "the storage is encrypted")
}

pub(crate) fn new_secret(variable: &str, noun: &str, reason: &'static str) -> Result<String> {
    if let Ok(value) = std::env::var(variable) {
        return Ok(value);
    }
    let first = secret(variable, &format!("New {noun}: "), reason)?;
    let second = secret(variable, &format!("The same {noun} again: "), reason)?;
    if first != second {
        return Err(CliError::SecretsDiffer(noun.to_string()));
    }
    Ok(first)
}
