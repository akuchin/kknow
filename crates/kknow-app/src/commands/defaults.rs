use std::collections::BTreeMap;

use kknow_run::TargetKind;
use kknow_run::spec::{CommandSpec, Cwd, DEFAULT_TIMEOUT, Manifest, VERSION};

pub fn defaults() -> Manifest {
    Manifest {
        version: VERSION,
        env: BTreeMap::new(),
        commands: vec![confluence(), agent()],
    }
}

pub fn confluence() -> CommandSpec {
    CommandSpec {
        id: "confluence".to_string(),
        name: "Update the Confluence page".to_string(),
        kind: "fish".to_string(),
        script: "confluence-tools replace $KKNOW_FM_CONFLUENCE \"$KKNOW_PATH\"".to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        requires: vec!["confluence".to_string()],
        login: false,
        program: None,
        cwd: Cwd::Vault,
        timeout_seconds: DEFAULT_TIMEOUT,
        confirm: false,
        env: BTreeMap::new(),
    }
}

pub const AGENT_VAR: &str = "AGENT";

pub const MODE_VAR: &str = "MODE";

pub fn agent() -> CommandSpec {
    CommandSpec {
        id: "agent".to_string(),
        name: "Process comments with an agent".to_string(),
        kind: "shell".to_string(),
        script: AGENT_SCRIPT.to_string(),
        targets: vec![TargetKind::Note],
        vaults: Vec::new(),
        patterns: Vec::new(),
        requires: Vec::new(),
        login: false,
        program: None,
        cwd: Cwd::Target,
        timeout_seconds: 3600,
        confirm: true,
        env: BTreeMap::from([
            (AGENT_VAR.to_string(), "claude".to_string()),
            (MODE_VAR.to_string(), "edit".to_string()),
        ]),
    }
}

pub const AGENT_SCRIPT: &str = r#"PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

format="Each comment has this format: <!-- @agent «TARGET»: INSTRUCTION -->, or <!-- @agent#ID «TARGET»: INSTRUCTION --> with a thread ID.
TARGET is the text that the comment refers to. Find it just before the comment.
A comment without «TARGET» refers to its whole paragraph."

doubt="When you are not sure, do not guess.
- If the instruction is unclear, or sources disagree, ask the person.
- If there are several good ways to do it, give the person the variants to choose from.
- Do the comments that are clear, and leave the others open."

case "$MODE" in
  ""|edit)
    prompt="Process the review comments in $KKNOW_PATH.

$format

For each comment:
1. Read the instruction and the context around TARGET.
2. If you can do the change, edit the text. Keep the style and format of the document.
3. Find all related places in the document and make them consistent with the change.
   Related places are: the same term or fact, references to it, links and anchors to changed headings.
4. Remove each comment that you processed.

A comment with #ID has a thread: the file $KKNOW_VAULT/.kknow/threads/ID.json. Read it before the comment.
- \"status\": \"question\" means the person has not answered your question yet. Skip the comment.
- \"status\": \"answered\" means the last message from \"me\" answers your question. Do the task with this answer.

$doubt
To ask or to give variants, keep the comment and add just after it:
<!-- @agent-note: QUESTION --> or <!-- @agent-note: VARIANTS: 1) ... 2) ... -->.
kknow moves the note into the thread of the comment, and the person answers there.

At the end, write a short list: each change with its location, and each comment that you did not process."
    claude_tools="Read,Edit(./**)"
    copilot_tool="write($KKNOW_PATH)"
    ;;
  propose)
    prompt="Process the review comments of the note $KKNOW_NOTE in the kknow vault $KKNOW_VAULT_NAME.
Do not edit files. Work only through the tools of the kknow MCP server.

$format

1. Call agent_comments with note=$KKNOW_NOTE to get the comments and their threads.
   Skip comments with the status question or review: they wait for the person.
   A comment with the status answered has the answer of the person in its thread.
2. For each comment, call claim_comment. It gives the comment ID.
3. Read the note with read_note. Plan the change and the related places that must stay consistent.
4. Call propose_edit with the edits, a short reason and comment=ID.
   Each old text must occur exactly once in the note: take more words around it if needed.

$doubt
To ask, call reply_comment with question=true. To give variants, make one propose_edit for each variant
and say in each reason which variant it is.

At the end, write a short list: each proposal, each question, and each comment that you did not process."
    claude_tools="Read,mcp__kknow"
    copilot_tool="kknow"
    ;;
  *) echo "MODE=\"${MODE}\": set edit or propose" >&2; exit 2 ;;
esac

# The threads are in the vault root, above the folder of the note.
set --
if [ -n "$KKNOW_VAULT" ]; then set -- --add-dir "$KKNOW_VAULT"; fi

case "$AGENT" in
  claude) exec claude -p "$prompt" --allowedTools "$claude_tools" "$@" ;;
  copilot) exec copilot -p "$prompt" --silent --allow-tool "$copilot_tool" "$@" ;;
  *) echo "AGENT=\"${AGENT}\": set claude or copilot" >&2; exit 2 ;;
esac
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use kknow_run::CommandSet;
    use std::fs;

    #[test]
    fn the_agent_is_claude_unless_the_user_picks_another() {
        let set = CommandSet::from(&defaults());
        let agent = set.get("agent").unwrap();

        assert_eq!(agent.env[AGENT_VAR], "claude");
        assert_eq!(agent.env[MODE_VAR], "edit");
        assert!(agent.script.contains("claude)"));
        assert!(agent.script.contains("copilot)"));
    }

    fn run_agent(mode: &str) -> (bool, String) {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("claude");
        fs::write(
            &fake,
            "#!/bin/sh\nfor arg in \"$@\"; do echo \"[$arg]\"; done\n",
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();

        let set = CommandSet::from(&defaults());
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&set.get("agent").unwrap().script)
            .env("HOME", home.path())
            .env("AGENT", "claude")
            .env("MODE", mode)
            .env("KKNOW_PATH", "/vault/План.md")
            .env("KKNOW_NOTE", "План")
            .env("KKNOW_VAULT_NAME", "Личное")
            .env("KKNOW_VAULT", "/my vault")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        (output.status.success(), text)
    }

    #[test]
    fn the_agent_edits_the_file_by_default_and_asks_when_in_doubt() {
        let (ok, args) = run_agent("edit");

        assert!(ok, "{args}");
        assert!(args.contains("[Read,Edit(./**)]"), "{args}");
        assert!(args.contains("do not guess"), "{args}");
        assert!(args.contains("@agent-note: VARIANTS"), "{args}");
        assert!(args.contains("/my vault/.kknow/threads/ID.json"), "{args}");
        assert!(args.contains(r#""status": "answered""#), "{args}");
        assert!(args.contains("[--add-dir]\n[/my vault]"), "{args}");
    }

    #[test]
    fn in_propose_mode_the_agent_has_only_the_kknow_tools() {
        let (ok, args) = run_agent("propose");

        assert!(ok, "{args}");
        assert!(args.contains("[Read,mcp__kknow]"), "{args}");
        assert!(!args.contains("Edit("), "{args}");
        assert!(args.contains("note=План"), "{args}");
        assert!(args.contains("propose_edit"), "{args}");
        assert!(args.contains("reply_comment with question=true"), "{args}");
    }

    #[test]
    fn an_unknown_mode_is_refused() {
        let (ok, text) = run_agent("yolo");

        assert!(!ok);
        assert!(text.contains(r#"MODE="yolo""#), "{text}");
    }
}
