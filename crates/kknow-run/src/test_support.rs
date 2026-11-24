use std::collections::BTreeMap;
use std::path::Path;

use crate::spec::{CommandSet, CommandSpec};
use crate::target::{Target, TargetKind, VaultRef};

pub(crate) fn spec(script: &str) -> CommandSpec {
    CommandSet::parse(&format!(
        r#"{{"commands":[{{"id":"проба","name":"Проба","kind":"shell","script":{}}}]}}"#,
        serde_json::to_string(script).unwrap()
    ))
    .unwrap()
    .commands
    .remove(0)
}

pub(crate) fn note(dir: &Path) -> Target {
    let path = dir.join("Docs/Index.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# Оглавление\n").unwrap();

    Target {
        kind: TargetKind::Note,
        path,
        relative: "Docs/Index.md".to_string(),
        note: Some("Docs/Index".to_string()),
        fields: BTreeMap::new(),
        vault: Some(VaultRef {
            id: "work".to_string(),
            name: "Работа".to_string(),
        }),
        vault_root: dir.to_path_buf(),
    }
}
