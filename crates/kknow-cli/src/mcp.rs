use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use crate::error::{CliError, Result};
use crate::vaults::{load_registry, vault_identity, vault_path};
use kknow_app::Workspace;
use kknow_app::mcp::{self, Host, PARSE_ERROR, VaultRef};
use kknow_app::vaults::{Scope, select};
use kknow_app::watch::VaultWatcher;
use serde_json::Value;

pub struct Served {
    pub vault: VaultRef,
    pub workspace: Workspace,
    pub watcher: Option<VaultWatcher>,
}

pub struct Vaults {
    refs: Vec<VaultRef>,
    served: Vec<Served>,
}

impl Vaults {
    pub fn new(served: Vec<Served>) -> Self {
        Vaults {
            refs: served.iter().map(|vault| vault.vault.clone()).collect(),
            served,
        }
    }

    fn catch_up(&mut self) {
        for vault in &mut self.served {
            let Some(watcher) = &vault.watcher else {
                continue;
            };
            let events = watcher.try_recv();
            if let Err(error) = vault.workspace.apply(&events) {
                eprintln!("warning: {}: {error}", vault.vault.name);
            }
        }
    }
}

impl Host for Vaults {
    fn vaults(&self) -> &[VaultRef] {
        &self.refs
    }

    fn read<R>(&self, index: usize, action: impl FnOnce(&Workspace) -> R) -> Result<R, String> {
        Ok(action(&self.served[index].workspace))
    }

    fn write<R>(
        &mut self,
        index: usize,
        action: impl FnOnce(&mut Workspace) -> R,
    ) -> Result<R, String> {
        Ok(action(&mut self.served[index].workspace))
    }
}

type Entry = (String, String, Vec<String>, PathBuf);

pub fn scope_of(vault: &str, labels: &[String]) -> Scope {
    if labels.is_empty() {
        Scope::Vault(vault.to_string())
    } else {
        Scope::of(None, labels)
    }
}

fn entries_of(vault: &str, labels: &[String]) -> Result<Vec<Entry>> {
    if labels.is_empty() {
        let path = vault_path(vault)?;
        let (id, name) = vault_identity(&path);
        let labels = load_registry()
            .ok()
            .and_then(|(registry, _)| registry.get(&id).map(|entry| entry.labels.clone()))
            .unwrap_or_default();
        return Ok(vec![(id, name, labels, path)]);
    }

    let (registry, _) = load_registry()?;
    let found = select(registry.entries(), &scope_of(vault, labels))?;
    if found.is_empty() {
        return Err(CliError::NoVaultWithLabel(labels.join(", ")));
    }
    Ok(found
        .into_iter()
        .filter(|entry| {
            let available = entry.available();
            if !available {
                eprintln!("warning: the folder is not available: {}", entry.name);
            }
            available
        })
        .map(|entry| {
            (
                entry.id.clone(),
                entry.name.clone(),
                entry.labels.clone(),
                entry.path.clone(),
            )
        })
        .collect())
}

pub fn run(vault: &str, labels: &[String]) -> Result<()> {
    let mut served = Vec::new();
    for (id, name, labels, path) in entries_of(vault, labels)? {
        let workspace = Workspace::open(&path).map_err(|source| CliError::Vault {
            vault: name.clone(),
            source,
        })?;
        let watcher = workspace
            .watch()
            .inspect_err(|error| eprintln!("warning: {name}: {error}"))
            .ok();
        served.push(Served {
            vault: VaultRef { id, name, labels },
            workspace,
            watcher,
        });
    }
    let names: Vec<&str> = served
        .iter()
        .map(|vault| vault.vault.name.as_str())
        .collect();
    eprintln!("kknow mcp: {}", names.join(", "));

    let mut vaults = Vaults::new(served);
    serve(
        &mut vaults,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )?;
    Ok(())
}

pub fn serve(vaults: &mut Vaults, input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        vaults.catch_up();

        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => mcp::handle_message(vaults, &message),
            Err(error) => Some(mcp::failure(Value::Null, PARSE_ERROR, &error.to_string())),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serve_answers_line_by_line_and_survives_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("План.md"), "- [ ] начать\n").unwrap();
        let mut vaults = Vaults::new(vec![Served {
            vault: VaultRef {
                id: "дом".to_string(),
                name: "Дом".to_string(),
                labels: Vec::new(),
            },
            workspace: Workspace::open(dir.path()).unwrap(),
            watcher: None,
        }]);
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            "\n",
            "не json\n",
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tasks","arguments":{}}}"#,
            "\n",
        );
        let mut output = Vec::new();

        serve(&mut vaults, input.as_bytes(), &mut output).unwrap();

        let replies: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies.len(), 3, "{replies:?}");
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[1]["error"]["code"], PARSE_ERROR);
        assert_eq!(replies[2]["result"]["content"][0]["text"], "План:1  начать");
    }

    #[test]
    fn a_batch_line_gets_one_line_with_an_array() {
        let dir = tempfile::tempdir().unwrap();
        let mut vaults = Vaults::new(vec![Served {
            vault: VaultRef {
                id: "home".to_string(),
                name: "Home".to_string(),
                labels: Vec::new(),
            },
            workspace: Workspace::open(dir.path()).unwrap(),
            watcher: None,
        }]);
        let batch = json!([
            { "jsonrpc": "2.0", "id": 1, "method": "ping" },
            { "jsonrpc": "2.0", "method": "notifications/initialized" },
            { "jsonrpc": "2.0", "id": "two", "method": "ping" },
        ]);
        let input = format!("{batch}\n");
        let mut output = Vec::new();

        serve(&mut vaults, input.as_bytes(), &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        let replies: Value = serde_json::from_str(text.trim()).unwrap();
        let ids: Vec<&Value> = replies
            .as_array()
            .unwrap()
            .iter()
            .map(|reply| &reply["id"])
            .collect();
        assert_eq!(ids, [&json!(1), &json!("two")]);
    }

    #[test]
    fn the_flags_make_the_same_scope_as_the_query_of_the_window() {
        let labels = vec!["#Work".to_string()];
        assert_eq!(scope_of("notes", &labels), Scope::of(None, &labels));
        assert_eq!(scope_of("notes", &[]), Scope::of(Some("notes"), &[]));
    }
}
