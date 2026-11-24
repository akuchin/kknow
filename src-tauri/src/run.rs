use std::collections::BTreeMap;

use kknow_run::spec::Problem;
use kknow_run::{CommandSpec, Manifest, Outcome, Stream, Target, TargetKind, Watch};
use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

use crate::state::{AppState, Error, Result};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CommandView {
    id: String,
    name: String,
    kind: String,
    confirm: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CommandList {
    commands: Vec<CommandView>,
    problems: Vec<Problem>,
}

fn target_of(
    state: &AppState,
    vault: Option<&str>,
    kind: &str,
    target: Option<&str>,
    file: Option<&str>,
) -> Result<Target> {
    let kind = TargetKind::parse(kind)
        .ok_or_else(|| Error::Run(kknow_run::Error::UnknownKind(kind.to_string())))?;

    if let Some(file) = file {
        if kind != TargetKind::Note {
            return Err(Error::UnknownFile(file.to_string()));
        }
        return Ok(kknow_app::commands::file_target(&state.known_file(file)?)?);
    }

    let slot = state.slot(vault)?;
    slot.read(|workspace| {
        Ok(kknow_app::commands::resolve(
            workspace,
            &slot.entry.id,
            &slot.entry.name,
            Some(kind),
            target,
        )?)
    })
}

#[tauri::command(async)]
pub fn list_commands(
    state: State<'_, AppState>,
    kind: String,
    target: Option<String>,
    vault: Option<String>,
    file: Option<String>,
) -> Result<CommandList> {
    let set = state.commands()?;
    let resolved = target_of(
        &state,
        vault.as_deref(),
        &kind,
        target.as_deref(),
        file.as_deref(),
    )?;

    Ok(CommandList {
        commands: set
            .for_target(&resolved)
            .into_iter()
            .map(|spec| CommandView {
                id: spec.id.clone(),
                name: spec.name.clone(),
                kind: spec.kind.clone(),
                confirm: spec.confirm,
            })
            .collect(),
        problems: set.problems,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RunLine {
    stream: Stream,
    text: String,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn run_command(
    state: State<'_, AppState>,
    command: String,
    kind: String,
    target: Option<String>,
    vault: Option<String>,
    file: Option<String>,
    run: String,
    lines: Channel<RunLine>,
) -> Result<Outcome> {
    let set = state.commands()?;
    if set.get(&command).is_none() {
        return Err(Error::Run(kknow_run::Error::Unknown(command)));
    }

    let resolved = target_of(
        &state,
        vault.as_deref(),
        &kind,
        target.as_deref(),
        file.as_deref(),
    )?;

    let stop = state.start_run(&run);
    let watch = Watch {
        line: &|stream, text| {
            let _ = lines.send(RunLine {
                stream,
                text: text.to_string(),
            });
        },
        stop: &stop,
    };
    let outcome = set.run_watched(&command, &resolved, &watch);
    state.end_run(&run);
    Ok(outcome?)
}

#[tauri::command]
pub fn stop_command(state: State<'_, AppState>, run: String) {
    state.stop_run(&run);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "CommandManifest"))]
pub struct ManifestView {
    path: String,
    commands: Vec<CommandSpec>,
    env: BTreeMap<String, String>,
    problems: Vec<Problem>,
    kinds: Vec<String>,
    broken: Option<String>,
}

impl ManifestView {
    fn new(path: &std::path::Path, manifest: Manifest, broken: Option<String>) -> Self {
        ManifestView {
            path: path.to_string_lossy().into_owned(),
            problems: manifest.problems(),
            commands: manifest.commands,
            env: manifest.env,
            kinds: kknow_run::runner::kinds()
                .into_iter()
                .map(str::to_string)
                .collect(),
            broken,
        }
    }
}

#[tauri::command(async)]
pub fn edit_commands(state: State<'_, AppState>) -> Result<ManifestView> {
    let path = state.commands_path().ok_or(Error::NoConfig)?;

    Ok(match Manifest::load(&path) {
        Ok(manifest) => ManifestView::new(&path, manifest, None),
        Err(kknow_run::Error::Manifest { reason, .. }) => {
            ManifestView::new(&path, Manifest::default(), Some(reason))
        }
        Err(other) => return Err(other.into()),
    })
}

#[tauri::command(async)]
pub fn save_commands(
    state: State<'_, AppState>,
    commands: Vec<CommandSpec>,
    env: BTreeMap<String, String>,
) -> Result<ManifestView> {
    let path = state.commands_path().ok_or(Error::NoConfig)?;

    let manifest = Manifest {
        version: kknow_run::spec::VERSION,
        env,
        commands,
    };
    manifest.save(&path)?;

    Ok(ManifestView::new(&path, manifest, None))
}

#[tauri::command(async)]
pub fn reveal_commands(state: State<'_, AppState>) -> Result<()> {
    let path = state.commands_path().ok_or(Error::NoConfig)?;

    if !path.exists() {
        kknow_run::spec::defaults().save(&path)?;
    }
    crate::commands::reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Docs")).unwrap();
        fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();
        fs::write(dir.path().join("Docs/Свод.md"), "# Свод\n").unwrap();
        dir
    }

    fn app(root: &Path, commands: &str) -> (tempfile::TempDir, tauri::App<MockRuntime>) {
        let config = tempfile::tempdir().unwrap();
        let file = config.path().join("commands.json");
        fs::write(&file, commands).unwrap();

        let state = crate::state::testing(&[root]);
        state.use_commands(Some(file));

        let app = mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано");
        (config, app)
    }

    fn run_plain(
        state: State<'_, AppState>,
        command: String,
        kind: String,
        target: Option<String>,
        vault: Option<String>,
        file: Option<String>,
    ) -> Result<Outcome> {
        let lines = Channel::new(|_| Ok(()));
        run_command(
            state,
            command,
            kind,
            target,
            vault,
            file,
            "проба".into(),
            lines,
        )
    }

    const SAMPLE: &str = r#"{"commands":[
        {"id":"echo","name":"Показать путь","kind":"shell",
         "script":"printf '%s|%s' \"$KKNOW_PATH\" \"$KKNOW_TARGET\"","targets":["note"]},
        {"id":"here","name":"Где я","kind":"shell","script":"printf x > от-корня","targets":["folder","vault"]},
        {"id":"плохая","name":"Сломанная","kind":"phyton","script":"…"}]}"#;

    #[test]
    fn only_commands_for_this_kind_of_target_are_listed() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        let note =
            list_commands(app.state(), "note".into(), Some("Index".into()), None, None).unwrap();
        let vault = list_commands(app.state(), "vault".into(), None, None, None).unwrap();

        assert_eq!(note.commands.len(), 1);
        assert_eq!(note.commands[0].id, "echo");
        assert_eq!(note.commands[0].name, "Показать путь");
        assert_eq!(vault.commands.len(), 1);
        assert_eq!(vault.commands[0].id, "here");
    }

    #[test]
    fn a_typo_in_the_file_reaches_the_window() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        let listed = list_commands(app.state(), "vault".into(), None, None, None).unwrap();

        assert_eq!(listed.problems.len(), 1);
        assert!(
            matches!(&listed.problems[0], Problem::UnknownKind { kind, .. } if kind == "phyton"),
            "{:?}",
            listed.problems
        );
    }

    #[test]
    fn the_path_reaches_the_script() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        let outcome = run_plain(
            app.state(),
            "echo".into(),
            "note".into(),
            Some("Docs/Свод".into()),
            None,
            None,
        )
        .unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        let (path, kind) = outcome.stdout.split_once('|').expect("две части");
        assert!(Path::new(path).ends_with("Docs/Свод.md"), "{path}");
        assert_eq!(kind, "note");
    }

    #[test]
    fn a_folder_target_starts_the_command_in_the_vault() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        let outcome = run_plain(
            app.state(),
            "here".into(),
            "folder".into(),
            Some("Docs".into()),
            None,
            None,
        )
        .unwrap();

        assert!(outcome.ok(), "{outcome:?}");
        assert!(
            dir.path().join("от-корня").exists(),
            "работа началась в корне хранилища"
        );
    }

    #[test]
    fn a_command_cannot_reach_outside_the_vault() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        for target in ["..", "/etc", "Docs/../.."] {
            assert!(
                run_plain(
                    app.state(),
                    "here".into(),
                    "folder".into(),
                    Some(target.into()),
                    None,
                    None,
                )
                .is_err(),
                "{target}"
            );
        }
    }

    #[test]
    fn a_command_cannot_be_run_on_a_target_it_does_not_apply_to() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        assert!(matches!(
            run_plain(app.state(), "echo".into(), "vault".into(), None, None, None),
            Err(Error::Run(kknow_run::Error::NotApplicable { .. }))
        ));
        assert!(matches!(
            run_plain(
                app.state(),
                "нет такой".into(),
                "vault".into(),
                None,
                None,
                None
            ),
            Err(Error::Run(kknow_run::Error::Unknown(_)))
        ));
    }

    #[test]
    fn a_standalone_file_gets_the_note_commands() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);
        let outside = tempfile::tempdir().unwrap();
        let readme = outside.path().join("README.md");
        fs::write(&readme, "# Проект\n").unwrap();
        let raw = || Some(readme.to_string_lossy().into_owned());

        assert!(list_commands(app.state(), "note".into(), None, None, raw()).is_err());

        let state = app.state::<AppState>();
        state.remember_file(&readme).unwrap();
        let path = state.known_file(&raw().unwrap()).unwrap();
        let file = || Some(path.to_string_lossy().into_owned());

        let listed = list_commands(app.state(), "note".into(), None, None, file()).unwrap();
        let ids: Vec<&str> = listed
            .commands
            .iter()
            .map(|view| view.id.as_str())
            .collect();
        assert_eq!(ids, ["echo"]);

        let outcome = run_plain(
            app.state(),
            "echo".into(),
            "note".into(),
            None,
            None,
            file(),
        )
        .unwrap();
        assert!(outcome.ok(), "{outcome:?}");
        assert_eq!(outcome.stdout, format!("{}|note", path.display()));

        assert!(list_commands(app.state(), "vault".into(), None, None, file()).is_err());
    }

    #[test]
    fn an_edited_file_takes_effect_without_a_restart() {
        let dir = vault();
        let config = tempfile::tempdir().unwrap();
        let file = config.path().join("commands.json");
        fs::write(&file, r#"{"commands":[]}"#).unwrap();

        let state = crate::state::testing(&[dir.path()]);
        state.use_commands(Some(file.clone()));
        let app = mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано");

        assert!(
            list_commands(app.state(), "vault".into(), None, None, None)
                .unwrap()
                .commands
                .is_empty()
        );

        fs::write(&file, SAMPLE).unwrap();

        assert_eq!(
            list_commands(app.state(), "vault".into(), None, None, None)
                .unwrap()
                .commands
                .len(),
            1,
            "файл правят руками — перезапуск ради этого не нужен"
        );
    }

    #[test]
    fn the_form_sees_the_broken_command_and_saves_it_back() {
        let dir = vault();
        let (_config, app) = app(dir.path(), SAMPLE);

        let opened = edit_commands(app.state()).unwrap();

        assert_eq!(opened.commands.len(), 3, "в меню их две, в форме — все");
        assert_eq!(opened.problems.len(), 1);
        assert!(opened.broken.is_none());
        assert!(opened.kinds.contains(&"shell".to_string()));

        let mut commands = opened.commands;
        commands[2].kind = "python".to_string();
        let saved = save_commands(app.state(), commands, BTreeMap::new()).unwrap();

        assert!(saved.problems.is_empty());
        assert_eq!(edit_commands(app.state()).unwrap().commands.len(), 3);
        let listed = list_commands(app.state(), "vault".into(), None, None, None).unwrap();
        assert!(listed.problems.is_empty());
        assert!(listed.commands.iter().any(|view| view.id == "плохая"));
    }

    #[test]
    fn a_broken_file_reaches_the_form_as_a_complaint() {
        let dir = vault();
        let (_config, app) = app(dir.path(), "{ это не json");

        let opened = edit_commands(app.state()).unwrap();

        assert!(opened.broken.is_some());
        assert!(opened.commands.is_empty());
    }

    #[test]
    fn no_file_yet_shows_the_shipped_commands() {
        let dir = vault();
        let config = tempfile::tempdir().unwrap();
        let state = crate::state::testing(&[dir.path()]);
        state.use_commands(Some(config.path().join("commands.json")));
        let app = mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано");

        let opened = edit_commands(app.state()).unwrap();

        assert_eq!(opened.commands, kknow_run::spec::defaults().commands);
        assert!(opened.broken.is_none());
    }

    #[test]
    fn no_file_means_no_commands() {
        let dir = vault();
        let app = mock_builder()
            .manage(crate::state::testing(&[dir.path()]))
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано");

        let listed = list_commands(app.state(), "vault".into(), None, None, None).unwrap();

        assert!(listed.commands.is_empty());
        assert!(listed.problems.is_empty());
    }

    #[test]
    fn lines_come_through_the_channel_and_stop_ends_the_run() {
        let dir = vault();
        let (_config, app) = app(
            dir.path(),
            r#"{"commands":[{"id":"долгая","name":"Долгая","kind":"shell",
                "script":"echo начали; sleep 30"}]}"#,
        );
        let (sent, got) = std::sync::mpsc::channel::<String>();
        let lines = Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                let _ = sent.send(json);
            }
            Ok(())
        });

        let handle = app.handle().clone();
        let running = std::thread::spawn(move || {
            run_command(
                handle.state(),
                "долгая".into(),
                "vault".into(),
                None,
                None,
                None,
                "запуск-1".into(),
                lines,
            )
        });

        let first = got
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("строка пришла до конца команды");
        assert!(first.contains("начали"), "{first}");
        assert!(first.contains("stdout"), "{first}");

        stop_command(app.state(), "запуск-1".into());
        let outcome = running.join().unwrap().unwrap();

        assert!(outcome.stopped, "{outcome:?}");
        assert!(outcome.millis < 20_000, "{outcome:?}");
        assert!(!app.state::<AppState>().stop_run("запуск-1"));
    }
}
