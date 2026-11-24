use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use ts_rs::{Config, TS, TypeVisitor};

struct Collect<'a> {
    config: &'a Config,
    seen: HashSet<TypeId>,
    decls: BTreeMap<String, String>,
}

impl TypeVisitor for Collect<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if T::output_path().is_none() || !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        let mut decl = T::docs().unwrap_or_default();
        decl.push_str("export ");
        decl.push_str(&T::decl(self.config));
        self.decls.insert(T::ident(self.config), decl);
        T::visit_dependencies(self);
    }
}

fn generate() -> String {
    let config = Config::new().with_large_int("number");
    let mut collect = Collect {
        config: &config,
        seen: HashSet::new(),
        decls: BTreeMap::new(),
    };

    collect.visit::<crate::commands::VaultInfo>();
    collect.visit::<crate::commands::VaultView>();
    collect.visit::<crate::mcp_http::McpStatus>();
    collect.visit::<crate::commands::VaultStatsView>();
    collect.visit::<crate::commands::GlobalResult>();
    collect.visit::<crate::commands::SearchResult>();
    collect.visit::<crate::commands::NoteSummary>();
    collect.visit::<crate::commands::NoteView>();
    collect.visit::<kknow_app::proposals::Review>();
    collect.visit::<crate::commands::BookmarkView>();
    collect.visit::<crate::commands::TaskView>();
    collect.visit::<crate::commands::LinkTargetView>();
    collect.visit::<crate::commands::BacklinkLineView>();
    collect.visit::<crate::commands::MentionView>();
    collect.visit::<crate::commands::LinkPreview>();
    collect.visit::<crate::commands::TrashEntryView>();
    collect.visit::<crate::commands::RenamedNote>();
    collect.visit::<crate::commands::SaveOutcome>();
    collect.visit::<crate::sync::SyncSettings>();
    collect.visit::<crate::sync::SyncReport>();
    collect.visit::<crate::sync::EncryptionOn>();
    collect.visit::<crate::sync_status::SyncStatus>();
    collect.visit::<kknow_sync::GitStatus>();
    collect.visit::<kknow_sync::GitAuth>();
    collect.visit::<crate::keymap::KeymapView>();
    collect.visit::<crate::association::Association>();
    collect.visit::<crate::files::Opened>();
    collect.visit::<crate::files::FileEntry>();
    collect.visit::<crate::files::FileView>();
    collect.visit::<crate::files::FileSaveOutcome>();
    collect.visit::<crate::live::VaultChanged>();
    collect.visit::<crate::updater::UpdateStatus>();
    collect.visit::<crate::run::CommandList>();
    collect.visit::<crate::run::ManifestView>();
    collect.visit::<crate::run::RunLine>();
    collect.visit::<crate::errors::ErrorView>();
    collect.visit::<crate::lock::LockStateView>();
    collect.visit::<crate::lock::UnlockedBy>();
    collect.visit::<kknow_app::theme::ThemeSet>();
    collect.visit::<kknow_app::settings::AppSettings>();
    collect.visit::<kknow_run::Outcome>();
    collect.visit::<kknow_sync::RemoteConfig>();

    let mut out = String::new();
    for decl in collect.decls.values() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(decl);
        out.push('\n');
    }
    out
}

#[test]
fn bindings_match_the_rust_types() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/bindings.ts");
    let fresh = generate();

    if std::env::var_os("KKNOW_BLESS").is_some() {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        lines(&current) == lines(&fresh),
        "ui/src/bindings.ts is stale: KKNOW_BLESS=1 cargo test -p kknow-desktop bindings"
    );
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}
