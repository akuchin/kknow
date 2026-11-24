use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const THEMES_DIR: &str = ".kknow/themes";

pub const GLOBAL_DIR: &str = "themes";

pub fn global_dir(config_dir: impl AsRef<Path>) -> PathBuf {
    config_dir.as_ref().join(GLOBAL_DIR)
}

const BUILT_IN: &[(&str, &str)] = &[
    ("gruvbox", include_str!("../themes/gruvbox.json")),
    (
        "gruvbox-light",
        include_str!("../themes/gruvbox-light.json"),
    ),
];

pub const VARIABLES: &[&str] = &[
    "bg",
    "bg-soft",
    "bg-raised",
    "border",
    "text",
    "text-muted",
    "heading",
    "accent",
    "accent-soft",
    "danger",
    "selection",
    "code-keyword",
    "code-string",
    "code-comment",
    "code-number",
    "code-type",
    "code-function",
    "font-text",
    "font-mono",
];

const FUNCTIONS: &[&str] = &[
    "rgb",
    "rgba",
    "hsl",
    "hsla",
    "oklch",
    "oklab",
    "color-mix",
    "var",
];

const MAX_VALUE: usize = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Base {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ThemeSource"))]
pub enum Source {
    #[default]
    Vault,
    Global,
    BuiltIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub base: Base,
    pub source: Source,
    pub shadows_global: bool,
    pub variables: BTreeMap<String, String>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ThemeWarning"))]
pub enum Warning {
    UnknownVariable { variable: String },
    EmptyValue { variable: String },
    TooLong { variable: String, max: u32 },
    BadCharacter { variable: String, character: String },
    Function { variable: String, function: String },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::UnknownVariable { variable } => {
                write!(f, "{variable}: the interface has no such variable")
            }
            Warning::EmptyValue { variable } => write!(f, "{variable}: the value is empty"),
            Warning::TooLong { variable, max } => {
                write!(
                    f,
                    "{variable}: longer than {max} bytes, this is not a color"
                )
            }
            Warning::BadCharacter {
                variable,
                character,
            } => write!(f, "{variable}: character {character} is not allowed"),
            Warning::Function { variable, function } => {
                write!(
                    f,
                    "{variable}: function {function}(…) is not allowed in a theme"
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    Json(String),
    NoName,
}

impl Invalid {
    fn at(self, path: String) -> Problem {
        match self {
            Invalid::Json(detail) => Problem::File { path, detail },
            Invalid::NoName => Problem::NoName { path },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ThemeProblem"))]
pub enum Problem {
    File { path: String, detail: String },
    NoName { path: String },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::File { path, detail } => write!(f, "{path}: {detail}"),
            Problem::NoName { path } => write!(f, "{path}: the theme has no name"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ThemeSet {
    pub themes: Vec<Theme>,
    pub problems: Vec<Problem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    base: Base,
    #[serde(default)]
    variables: BTreeMap<String, String>,
}

impl Theme {
    pub fn parse(id: impl Into<String>, json: &str) -> Result<Self, Invalid> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|error| Invalid::Json(error.to_string()))?;

        let name = manifest.name.trim().to_string();
        if name.is_empty() {
            return Err(Invalid::NoName);
        }

        let mut variables = BTreeMap::new();
        let mut warnings = Vec::new();

        for (raw_name, value) in manifest.variables {
            let name = raw_name.trim().trim_start_matches("--").to_string();

            if !VARIABLES.contains(&name.as_str()) {
                warnings.push(Warning::UnknownVariable { variable: raw_name });
                continue;
            }
            if let Err(warning) = check_value(&raw_name, &value) {
                warnings.push(warning);
                continue;
            }

            variables.insert(name, value.trim().to_string());
        }

        Ok(Theme {
            id: id.into(),
            name,
            author: manifest.author,
            base: manifest.base,
            source: Source::Vault,
            shadows_global: false,
            variables,
            warnings,
        })
    }
}

fn check_value(variable: &str, value: &str) -> Result<(), Warning> {
    let variable = variable.to_string();
    let value = value.trim();

    if value.is_empty() {
        return Err(Warning::EmptyValue { variable });
    }
    if value.len() > MAX_VALUE {
        return Err(Warning::TooLong {
            variable,
            max: MAX_VALUE as u32,
        });
    }
    if let Some(bad) = value
        .chars()
        .find(|symbol| ";{}<>\\/@".contains(*symbol) || symbol.is_control())
    {
        return Err(Warning::BadCharacter {
            variable,
            character: format!("{bad:?}"),
        });
    }

    let mut rest = value;
    while let Some(open) = rest.find('(') {
        let head = &rest[..open];
        let start = head
            .char_indices()
            .rev()
            .find(|(_, symbol)| !(symbol.is_ascii_alphanumeric() || *symbol == '-'))
            .map_or(0, |(at, symbol)| at + symbol.len_utf8());
        let name = &head[start..];

        if !FUNCTIONS.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(Warning::Function {
                variable,
                function: name.to_string(),
            });
        }
        rest = &rest[open + 1..];
    }

    Ok(())
}

pub fn load(vault_root: Option<&Path>, global: Option<&Path>) -> ThemeSet {
    let mut set = ThemeSet::default();

    for (id, json) in BUILT_IN {
        match Theme::parse(*id, json) {
            Ok(mut theme) => {
                theme.source = Source::BuiltIn;
                set.themes.push(theme);
            }
            Err(invalid) => set.problems.push(invalid.at(id.to_string())),
        }
    }

    if let Some(dir) = global {
        load_dir(dir, Source::Global, &mut set);
    }
    if let Some(root) = vault_root {
        load_dir(&root.join(THEMES_DIR), Source::Vault, &mut set);
    }

    set.themes.sort_by(|left, right| left.name.cmp(&right.name));
    set
}

fn load_dir(dir: &Path, source: Source, set: &mut ThemeSet) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            set.problems.push(Problem::File {
                path: dir.display().to_string(),
                detail: error.to_string(),
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let id = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        match std::fs::read_to_string(&path).map_err(|error| Invalid::Json(error.to_string())) {
            Ok(json) => match Theme::parse(id, &json) {
                Ok(mut theme) => {
                    theme.source = source;
                    theme.shadows_global = set
                        .themes
                        .iter()
                        .any(|known| known.id == theme.id && known.source == Source::Global);
                    set.themes.retain(|known| known.id != theme.id);
                    set.themes.push(theme);
                }
                Err(invalid) => set.problems.push(invalid.at(path.display().to_string())),
            },
            Err(invalid) => set.problems.push(invalid.at(path.display().to_string())),
        }
    }
}

pub fn share(vault_root: &Path, id: &str, global: &Path) -> std::io::Result<PathBuf> {
    let plain = !id.is_empty()
        && !id.starts_with('.')
        && Path::new(id).file_name() == Some(std::ffi::OsStr::new(id));
    if !plain {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("not a theme name: {id}"),
        ));
    }

    let file = format!("{id}.json");
    let source = vault_root.join(THEMES_DIR).join(&file);
    let target = global.join(&file);

    std::fs::create_dir_all(global)?;
    std::fs::copy(&source, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn theme(json: &str) -> Theme {
        Theme::parse("проба", json).expect("манифест разобран")
    }

    fn own(set: &ThemeSet) -> Vec<&Theme> {
        set.themes
            .iter()
            .filter(|theme| theme.source != Source::BuiltIn)
            .collect()
    }

    #[test]
    fn parses_a_manifest() {
        let parsed = theme(
            r##"{
                "name": "Сепия",
                "author": "kknow",
                "base": "dark",
                "variables": { "bg": "#f4ecd8", "--text": "rgb(59 50 38)" }
            }"##,
        );

        assert_eq!(parsed.id, "проба");
        assert_eq!(parsed.name, "Сепия");
        assert_eq!(parsed.author.as_deref(), Some("kknow"));
        assert_eq!(parsed.base, Base::Dark);
        assert_eq!(parsed.variables["bg"], "#f4ecd8");
        assert_eq!(
            parsed.variables["text"], "rgb(59 50 38)",
            "`--` — та же переменная"
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    }

    #[test]
    fn an_unknown_variable_becomes_a_warning_not_a_failure() {
        let parsed = theme(r##"{"name":"Проба","variables":{"bgg":"#fff","bg":"#eee"}}"##);

        assert_eq!(parsed.variables.len(), 1);
        assert_eq!(parsed.warnings.len(), 1);
        assert!(
            parsed.warnings[0]
                == Warning::UnknownVariable {
                    variable: "bgg".into()
                },
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn a_theme_without_a_name_is_not_a_theme() {
        assert!(Theme::parse("проба", r#"{"variables":{}}"#).is_err());
        assert!(Theme::parse("проба", r#"{"name":"  "}"#).is_err());
        assert!(Theme::parse("проба", "не json").is_err());
    }

    #[test]
    fn a_theme_cannot_reach_the_network() {
        let parsed = theme(
            r##"{"name":"Проба","variables":{
                "bg": "url(tracker.png)",
                "bg-soft": "image-set(\"a.png\" 1x)",
                "text": "#000"
            }}"##,
        );

        assert_eq!(parsed.variables.len(), 1, "{:?}", parsed.variables);
        assert_eq!(parsed.warnings.len(), 2);
    }

    #[test]
    fn colour_functions_are_allowed() {
        for value in ["rgb(1 2 3)", "rgba(1,2,3,0.5)", "hsl(210 40% 30%)"] {
            assert!(check_value("bg", value).is_ok(), "{value}");
        }
    }

    #[test]
    fn a_value_cannot_carry_more_css_with_it() {
        for value in [
            "#fff; background: red",
            "red } body { display: none",
            "@import 'other.css'",
            "red /* хвост */",
            "",
        ] {
            assert!(check_value("bg", value).is_err(), "{value}");
        }
    }

    #[test]
    fn font_stacks_survive_the_check() {
        assert!(check_value("font-text", "\"PT Sans\", -apple-system, sans-serif").is_ok());
    }

    #[test]
    fn loads_themes_from_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let themes = dir.path().join(THEMES_DIR);
        fs::create_dir_all(&themes).unwrap();
        fs::write(themes.join("sepia.json"), r#"{"name":"Сепия"}"#).unwrap();
        fs::write(
            themes.join("night.json"),
            r#"{"name":"Ночь","base":"dark"}"#,
        )
        .unwrap();
        fs::write(themes.join("сломанная.json"), "{").unwrap();
        fs::write(themes.join("заметка.md"), "# не тема").unwrap();

        let set = load(Some(dir.path()), None);
        let themes = own(&set);

        let names: Vec<&str> = themes.iter().map(|theme| theme.name.as_str()).collect();
        assert_eq!(names, ["Ночь", "Сепия"], "список отсортирован по имени");
        assert_eq!(themes[0].id, "night");
        assert_eq!(set.problems.len(), 1, "{:?}", set.problems);
    }

    #[test]
    fn a_vault_without_themes_is_normal() {
        let dir = tempfile::tempdir().unwrap();

        let set = load(Some(dir.path()), None);

        assert!(own(&set).is_empty());
        assert!(set.problems.is_empty(), "{:?}", set.problems);
    }

    #[test]
    fn global_themes_are_seen_from_every_vault() {
        let vault = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();
        fs::write(global.path().join("sepia.json"), r#"{"name":"Сепия"}"#).unwrap();

        let set = load(Some(vault.path()), Some(global.path()));
        let themes = own(&set);

        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "sepia");
        assert_eq!(themes[0].source, Source::Global);
        assert_eq!(
            own(&load(None, Some(global.path()))).len(),
            1,
            "и без хранилища вовсе"
        );
    }

    #[test]
    fn a_vault_theme_overrides_a_global_one_with_the_same_id() {
        let vault = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();
        fs::create_dir_all(vault.path().join(THEMES_DIR)).unwrap();
        fs::write(global.path().join("gruvbox.json"), r#"{"name":"Общая"}"#).unwrap();
        fs::write(global.path().join("sepia.json"), r#"{"name":"Сепия"}"#).unwrap();
        fs::write(
            vault.path().join(THEMES_DIR).join("gruvbox.json"),
            r#"{"name":"Своя"}"#,
        )
        .unwrap();

        let set = load(Some(vault.path()), Some(global.path()));

        let themes = own(&set);
        let seen: Vec<(&str, Source)> = themes
            .iter()
            .map(|theme| (theme.name.as_str(), theme.source))
            .collect();
        assert_eq!(seen, [("Своя", Source::Vault), ("Сепия", Source::Global)]);
        assert!(themes[0].shadows_global);
        assert!(!themes[1].shadows_global);
    }

    #[test]
    fn sharing_copies_the_file_as_it_is() {
        let vault = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let global = global_dir(config.path());
        fs::create_dir_all(vault.path().join(THEMES_DIR)).unwrap();
        let json = r##"{"name":"Сепия","variables":{"bg":"#f4ecd8","future-var":"red"}}"##;
        fs::write(vault.path().join(THEMES_DIR).join("sepia.json"), json).unwrap();

        let target = share(vault.path(), "sepia", &global).unwrap();

        assert_eq!(target, global.join("sepia.json"));
        assert_eq!(fs::read_to_string(&target).unwrap(), json, "байт в байт");
        let other = tempfile::tempdir().unwrap();
        assert_eq!(
            own(&load(Some(other.path()), Some(&global)))[0].source,
            Source::Global
        );
        let home = load(Some(vault.path()), Some(&global));
        let home = own(&home);
        assert_eq!(home.len(), 1, "своя и общая — одна тема");
        assert!(home[0].shadows_global);
    }

    #[test]
    fn built_in_themes_come_without_any_files() {
        let set = load(None, None);

        let ids: Vec<&str> = set.themes.iter().map(|theme| theme.id.as_str()).collect();
        assert_eq!(ids, ["gruvbox", "gruvbox-light"], "by name: Dark < Light");
        assert!(set.problems.is_empty(), "{:?}", set.problems);
        for theme in &set.themes {
            assert_eq!(theme.source, Source::BuiltIn);
            assert!(
                theme.warnings.is_empty(),
                "{}: {:?}",
                theme.id,
                theme.warnings
            );
            assert!(!theme.variables.is_empty(), "{}", theme.id);
        }
    }

    #[test]
    fn a_file_with_the_same_id_replaces_a_built_in_theme() {
        let vault = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();
        fs::write(global.path().join("gruvbox.json"), r#"{"name":"Общая"}"#).unwrap();

        let set = load(None, Some(global.path()));
        let gruvbox: Vec<&Theme> = set.themes.iter().filter(|t| t.id == "gruvbox").collect();
        assert_eq!(gruvbox.len(), 1);
        assert_eq!(gruvbox[0].source, Source::Global);

        fs::create_dir_all(vault.path().join(THEMES_DIR)).unwrap();
        fs::write(
            vault.path().join(THEMES_DIR).join("gruvbox-light.json"),
            r#"{"name":"Своя"}"#,
        )
        .unwrap();
        let set = load(Some(vault.path()), None);
        let light: Vec<&Theme> = set
            .themes
            .iter()
            .filter(|t| t.id == "gruvbox-light")
            .collect();
        assert_eq!(light.len(), 1);
        assert_eq!(light[0].source, Source::Vault);
        assert!(
            !light[0].shadows_global,
            "у встроенной темы нет общей копии, которую можно обновить"
        );
    }

    #[test]
    fn sharing_refuses_a_path_instead_of_an_id() {
        let vault = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();

        for id in ["../secret", "a/b", "", "..", ".hidden"] {
            let error = share(vault.path(), id, global.path()).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{id}");
        }
    }

    #[test]
    fn sharing_a_missing_theme_is_an_error() {
        let vault = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();

        assert!(share(vault.path(), "nope", global.path()).is_err());
    }
}
