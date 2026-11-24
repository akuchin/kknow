use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use ts_rs::{Config, TS, TypeVisitor};

struct Collect<'a> {
    config: &'a Config,
    seen: HashSet<TypeId>,
    decls: BTreeMap<String, String>,
    names: BTreeMap<String, BTreeSet<String>>,
}

impl TypeVisitor for Collect<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if T::output_path().is_none() || !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        let mut decl = T::docs().unwrap_or_default();
        decl.push_str("export ");
        decl.push_str(&T::decl(self.config));
        let ident = T::ident(self.config);
        self.names
            .entry(last_segment(std::any::type_name::<T>()))
            .or_default()
            .insert(ident.clone());
        self.decls.insert(ident, decl);
        T::visit_dependencies(self);
    }
}

fn generate() -> String {
    let config = Config::new().with_large_int("number");
    let mut collect = Collect {
        config: &config,
        seen: HashSet::new(),
        decls: BTreeMap::new(),
        names: BTreeMap::new(),
    };

    collect.visit::<crate::commands::vaults::VaultInfo>();
    collect.visit::<crate::commands::vaults::VaultView>();
    collect.visit::<crate::mcp_http::McpStatus>();
    collect.visit::<crate::commands::vaults::VaultStatsView>();
    collect.visit::<crate::commands::search::GlobalResult>();
    collect.visit::<crate::commands::search::SearchResult>();
    collect.visit::<crate::commands::notes::NoteSummary>();
    collect.visit::<kknow_app::view::NoteView>();
    collect.visit::<kknow_app::proposals::Review>();
    collect.visit::<crate::commands::marks::BookmarkView>();
    collect.visit::<crate::commands::marks::TaskView>();
    collect.visit::<crate::commands::links::LinkTargetView>();
    collect.visit::<crate::commands::links::BacklinkLineView>();
    collect.visit::<crate::commands::links::MentionView>();
    collect.visit::<kknow_app::view::LinkPreview>();
    collect.visit::<crate::commands::trash::TrashEntryView>();
    collect.visit::<crate::commands::notes::RenamedNote>();
    collect.visit::<crate::commands::notes::SaveOutcome>();
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
    let commands = command_map(&collect.names).unwrap_or_else(|message| panic!("{message}"));
    if commands.contains("Channel<") {
        out.insert_str(
            0,
            "import type { Channel } from \"@tauri-apps/api/core\";\n\n",
        );
    }
    out.push('\n');
    out.push_str(&commands);
    out
}

fn last_segment(type_name: &str) -> String {
    let bare = type_name.split('<').next().unwrap_or(type_name);
    bare.rsplit("::").next().unwrap_or(bare).to_string()
}

type Names = BTreeMap<String, BTreeSet<String>>;

struct Command {
    name: String,
    args: String,
    result: String,
}

fn command_map(names: &Names) -> Result<String, String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let handlers = handler_paths(&read(&src.join("lib.rs")));
    let mut commands = Vec::new();
    for (module, name) in handlers {
        let path = module_file(&src, &module)
            .ok_or_else(|| format!("the module {module} of command {name} has no file"))?;
        let file = syn::parse_file(&read(&path))
            .map_err(|error| format!("{} does not parse: {error}", path.display()))?;
        let function = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(function) if function.sig.ident == name && is_command(function) => {
                    Some(function)
                }
                _ => None,
            })
            .ok_or_else(|| format!("command {name} is not a tauri command in {module}"))?;
        commands.push(convert_command(function, names)?);
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = String::from("export type Commands = {\n");
    for command in commands {
        out.push_str(&format!(
            "  {}: {{ args: {}; result: {} }};\n",
            command.name, command.args, command.result
        ));
    }
    out.push_str("};\n");
    Ok(out)
}

fn module_file(src: &Path, module: &str) -> Option<std::path::PathBuf> {
    let relative: std::path::PathBuf = module.split("::").collect();
    let plain = src.join(&relative).with_extension("rs");
    let nested = src.join(&relative).join("mod.rs");
    [plain, nested].into_iter().find(|path| path.is_file())
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn handler_paths(lib: &str) -> Vec<(String, String)> {
    let marker = "generate_handler![";
    let start = lib.find(marker).expect("generate_handler! in lib.rs");
    let body = &lib[start + marker.len()..];
    let body = &body[..body.find(']').expect("end of generate_handler!")];
    body.split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let (module, name) = path.rsplit_once("::").expect("module::command path");
            (module.to_string(), name.to_string())
        })
        .collect()
}

fn is_command(function: &syn::ItemFn) -> bool {
    function.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "command")
    })
}

const INJECTED: [&str; 5] = ["State", "AppHandle", "Window", "WebviewWindow", "Webview"];

fn convert_command(function: &syn::ItemFn, names: &Names) -> Result<Command, String> {
    let name = function.sig.ident.to_string();
    let unresolved = |ty: String| format!("command {name}: cannot resolve the type {ty}");
    let mut fields = Vec::new();
    let mut raw_body = false;
    for input in &function.sig.inputs {
        let syn::FnArg::Typed(typed) = input else {
            continue;
        };
        let syn::Pat::Ident(pattern) = &*typed.pat else {
            return Err(format!("command {name}: a parameter has no plain name"));
        };
        let head = head_segment(&typed.ty);
        if INJECTED.contains(&head.as_str()) {
            continue;
        }
        if head == "Request" {
            raw_body = true;
            continue;
        }
        let key = camel_case(&pattern.ident.to_string());
        let (ty, optional) = match inner_of(&typed.ty, "Option") {
            Some(inner) => (inner, true),
            None => (&*typed.ty, false),
        };
        let ts = ts_type(ty, names).map_err(unresolved)?;
        fields.push(if optional {
            format!("{key}?: {ts} | null")
        } else {
            format!("{key}: {ts}")
        });
    }
    let args = if raw_body {
        "Uint8Array".to_string()
    } else if fields.is_empty() {
        "Record<never, never>".to_string()
    } else {
        format!("{{ {} }}", fields.join(", "))
    };
    let result = match &function.sig.output {
        syn::ReturnType::Default => "void".to_string(),
        syn::ReturnType::Type(_, ty) => {
            let inner = inner_of(ty, "Result").unwrap_or(ty);
            if head_segment(inner) == "Response" {
                "ArrayBuffer".to_string()
            } else {
                ts_type(inner, names).map_err(unresolved)?
            }
        }
    };
    Ok(Command { name, args, result })
}

fn head_segment(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn generic_args(ty: &syn::Type) -> Vec<&syn::Type> {
    let syn::Type::Path(path) = ty else {
        return Vec::new();
    };
    let Some(segment) = path.path.segments.last() else {
        return Vec::new();
    };
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Vec::new();
    };
    arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect()
}

fn inner_of<'a>(ty: &'a syn::Type, wrapper: &str) -> Option<&'a syn::Type> {
    if head_segment(ty) != wrapper {
        return None;
    }
    generic_args(ty).first().copied()
}

fn camel_case(snake: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in snake.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn ts_type(ty: &syn::Type, names: &Names) -> Result<String, String> {
    match ty {
        syn::Type::Reference(reference) => ts_type(&reference.elem, names),
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Ok("void".to_string()),
        syn::Type::Tuple(tuple) => {
            let items: Result<Vec<_>, _> = tuple
                .elems
                .iter()
                .map(|elem| ts_type(elem, names))
                .collect();
            Ok(format!("[{}]", items?.join(", ")))
        }
        syn::Type::Path(_) => path_type(ty, names),
        _ => Err(describe(ty)),
    }
}

fn path_type(ty: &syn::Type, names: &Names) -> Result<String, String> {
    let head = head_segment(ty);
    let args = generic_args(ty);
    let nth = |index: usize| {
        args.get(index)
            .copied()
            .ok_or_else(|| describe(ty))
            .and_then(|inner| ts_type(inner, names))
    };
    match head.as_str() {
        "String" | "str" | "PathBuf" | "Path" => Ok("string".to_string()),
        "bool" => Ok("boolean".to_string()),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" | "f32"
        | "f64" => Ok("number".to_string()),
        "Vec" => Ok(format!("Array<{}>", nth(0)?)),
        "Option" => Ok(format!("{} | null", nth(0)?)),
        "Channel" => Ok(format!("Channel<{}>", nth(0)?)),
        "HashMap" | "BTreeMap" => Ok(format!("Record<string, {}>", nth(1)?)),
        _ => match names.get(&head) {
            Some(idents) if idents.len() == 1 => {
                Ok(idents.iter().next().cloned().unwrap_or_default())
            }
            _ => Err(describe(ty)),
        },
    }
}

fn describe(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        _ => "an unsupported type".to_string(),
    }
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

#[cfg(test)]
mod modules {
    use super::*;

    #[test]
    fn a_nested_path_resolves_to_a_file_or_a_mod_file() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert_eq!(
            module_file(&src, "commands::notes"),
            Some(src.join("commands/notes.rs"))
        );
        assert_eq!(
            module_file(&src, "commands"),
            Some(src.join("commands/mod.rs"))
        );
        assert_eq!(module_file(&src, "lock"), Some(src.join("lock.rs")));
        assert_eq!(module_file(&src, "commands::nowhere"), None);
    }

    #[test]
    fn a_listed_command_that_is_not_found_fails() {
        let missing = "export {}; generate_handler![commands::notes::no_such_command]";
        let paths = handler_paths(missing);
        assert_eq!(
            paths,
            [("commands::notes".to_string(), "no_such_command".to_string())]
        );
        let file = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/notes.rs"));
        let parsed = syn::parse_file(&file).unwrap();
        let found = parsed.items.iter().any(|item| {
            matches!(item, syn::Item::Fn(function)
                if function.sig.ident == "no_such_command" && is_command(function))
        });
        assert!(!found);
    }
}

#[cfg(test)]
mod converter {
    use super::*;

    fn names() -> Names {
        let mut names = Names::new();
        names.insert("Plain".into(), BTreeSet::from(["Plain".to_string()]));
        names.insert("Inner".into(), BTreeSet::from(["Renamed".to_string()]));
        names
    }

    fn convert(source: &str) -> Result<Command, String> {
        let function: syn::ItemFn = syn::parse_str(source).unwrap();
        convert_command(&function, &names())
    }

    #[test]
    fn injected_parameters_are_left_out_and_names_become_camel_case() {
        let command = convert(
            "#[tauri::command] fn open_it(state: State<'_, AppState>, note_id: String, app: AppHandle<R>) {}",
        )
        .unwrap();
        assert_eq!(command.args, "{ noteId: string }");
        assert_eq!(command.result, "void");
    }

    #[test]
    fn an_option_is_an_optional_key_with_null() {
        let command = convert("fn a(vault: Option<String>) -> Result<Vec<Plain>> {}").unwrap();
        assert_eq!(command.args, "{ vault?: string | null }");
        assert_eq!(command.result, "Array<Plain>");
    }

    #[test]
    fn a_channel_keeps_its_message_type() {
        let command = convert("fn a(lines: Channel<Plain>) -> Result<()> {}").unwrap();
        assert_eq!(command.args, "{ lines: Channel<Plain> }");
        assert_eq!(command.result, "void");
    }

    #[test]
    fn a_request_marks_a_raw_body_and_a_response_is_a_buffer() {
        let raw = convert("fn a(request: Request<'_>) -> Result<String> {}").unwrap();
        assert_eq!(raw.args, "Uint8Array");
        let response = convert("fn a() -> Result<Response> {}").unwrap();
        assert_eq!(response.args, "Record<never, never>");
        assert_eq!(response.result, "ArrayBuffer");
    }

    #[test]
    fn a_renamed_type_uses_the_name_of_ts_rs() {
        let command = convert("fn a() -> Result<Inner> {}").unwrap();
        assert_eq!(command.result, "Renamed");
    }

    #[test]
    fn an_unknown_type_fails_with_the_command_and_the_type() {
        let message = convert("fn open_it(value: Strange) {}").err().unwrap();
        assert!(message.contains("open_it"), "{message}");
        assert!(message.contains("Strange"), "{message}");
    }
}
