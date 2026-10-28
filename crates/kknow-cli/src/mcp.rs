//! `kknow mcp`: the MCP server on stdin and stdout.
//!
//! The protocol is in `kknow_app::mcp`; here is only the stdio transport.
//! The agent starts `kknow mcp --vault <vault>` (one vault) or
//! `kknow mcp --label <label>` (all vaults with the label) as a child process.
//! Each message is one line of JSON-RPC 2.0. The window has the same server
//! over HTTP; this one works on a machine without the window.
//!
//! stdout belongs to the protocol. Nothing else may print there: one stray
//! line breaks the session. Warnings go to stderr.

use std::io::{self, BufRead, Write};

use kknow_app::Workspace;
use kknow_app::mcp::{self, Host, PARSE_ERROR, VaultRef};
use kknow_app::watch::VaultWatcher;
use serde_json::Value;

/// One vault that the server serves.
pub struct Served {
    pub vault: VaultRef,
    pub workspace: Workspace,
    /// Without a watcher the server still works, but sees only its own writes.
    pub watcher: Option<VaultWatcher>,
}

/// The vaults of the process. They are taken once, at the start.
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

    /// Takes the file changes that the watchers collected. The window or the
    /// agent itself can change notes between two calls, and a search must not
    /// show the old text.
    fn catch_up(&mut self) {
        for vault in &mut self.served {
            let Some(watcher) = &vault.watcher else {
                continue;
            };
            let events = watcher.try_recv();
            if let Err(error) = vault.workspace.apply(&events) {
                eprintln!("предупреждение: {}: {error}", vault.vault.name);
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

/// Reads requests until stdin closes. The client closes it to stop the server.
pub fn serve(vaults: &mut Vaults, input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        vaults.catch_up();

        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => mcp::handle(vaults, &message),
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
}
