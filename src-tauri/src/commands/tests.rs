use crate::state::{AppState, Error};
use kknow_app::Workspace;
use kknow_app::bookmarks::Mark;
use kknow_app::theme::{self, ThemeSet};
use kknow_core::NoteId;
use std::fs;
use tauri::Manager;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

use super::links::*;
use super::marks::*;
use super::notes::*;
use super::search::*;
use super::trash::*;
use super::vaults::*;
use super::windows::*;

fn app(roots: &[&std::path::Path]) -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(crate::state::testing(roots))
        .build(mock_context(noop_assets()))
        .expect("мок-приложение собрано")
}

fn vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Projects")).unwrap();
    fs::write(
        dir.path().join("Index.md"),
        "# Оглавление\n\n[[Projects/kknow|мой проект]]\n",
    )
    .unwrap();
    fs::write(dir.path().join("Projects/kknow.md"), "# kknow\n\n#rust\n").unwrap();
    dir
}

fn direct(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open(dir.path()).unwrap()
}

fn weak_lock(workspace: &mut Workspace) {
    let weak = kknow_crypto::KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };
    workspace
        .set_lock_password_with("correct horse", weak)
        .unwrap();
    workspace
        .lock_note(&NoteId::parse("Index").unwrap())
        .unwrap();
}

#[test]
fn load_note_of_a_locked_note_is_the_shared_view() {
    let window_dir = vault();
    let app = app(&[window_dir.path()]);
    let slot = app.state::<AppState>().slot(None).unwrap();
    slot.write(|workspace| {
        weak_lock(workspace);
        Ok(())
    })
    .unwrap();
    let other = vault();
    let mut workspace = direct(&other);
    weak_lock(&mut workspace);

    let from_window = load_note(app.state(), "Index".into(), None).unwrap();
    let shared = workspace
        .load_note(&NoteId::parse("Index").unwrap())
        .unwrap();

    assert!(shared.locked);
    assert_eq!(
        serde_json::to_value(&from_window).unwrap(),
        serde_json::to_value(&shared).unwrap()
    );
}

#[test]
fn load_note_adopts_an_agent_answer_as_the_shared_view_does() {
    let text = "Demo zero<!-- @agent \u{ab}zero\u{bb}: start -->\n<!-- @agent-note: VARIANTS: 1) a -->\nDemo one\n";
    let window_dir = tempfile::tempdir().unwrap();
    fs::write(window_dir.path().join("Demo.md"), text).unwrap();
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("Demo.md"), text).unwrap();
    let app = app(&[window_dir.path()]);
    let mut workspace = direct(&other);

    let from_window = load_note(app.state(), "Demo".into(), None).unwrap();
    let shared = workspace
        .load_note(&NoteId::parse("Demo").unwrap())
        .unwrap();

    assert!(!from_window.content.contains("@agent-note"));
    assert_eq!(from_window.threads.len(), 1);
    assert_eq!(shared.threads.len(), 1);
    assert_eq!(
        from_window.threads[0].messages[1].text,
        shared.threads[0].messages[1].text
    );
    assert_eq!(from_window.threads[0].messages.len(), 2);
    assert_eq!(
        from_window.html.contains("@agent"),
        shared.html.contains("@agent")
    );
}

#[test]
fn lists_notes_with_titles_folders_and_tags() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let notes = list_notes(app.state(), None).unwrap();

    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].id, "Index");
    assert_eq!(notes[0].title, "Оглавление");
    assert_eq!(notes[0].folder, None);
    assert_eq!(notes[1].folder.as_deref(), Some("Projects"));
    assert_eq!(notes[1].tags, ["rust"]);
}

#[test]
fn bookmarks_come_with_the_note_and_move_with_saves() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let marks = set_bookmarks(
        app.state(),
        "Index".into(),
        vec![Mark::new(3, Some("проект"))],
        "# Оглавление\n\n[[Projects/kknow|мой проект]]\n".into(),
        None,
    )
    .unwrap();
    assert_eq!(marks, [Mark::new(3, Some("проект"))]);
    assert_eq!(
        load_note(app.state(), "Index".into(), None)
            .unwrap()
            .bookmarks,
        [Mark::new(3, Some("проект"))]
    );

    let SaveOutcome::Saved { note } = save_note(
        app.state(),
        "Index".into(),
        "# Оглавление\n\nновая строка\n[[Projects/kknow|мой проект]]\n".into(),
        None,
        Some(vec![Mark::new(4, Some("проект"))]),
        None,
    )
    .unwrap() else {
        panic!("без base сохранение безусловное");
    };
    assert_eq!(note.bookmarks, [Mark::new(4, Some("проект"))]);

    let listed = list_bookmarks(app.state(), None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Оглавление");
    assert_eq!(listed[0].line, 4);
    assert_eq!(listed[0].text, "[[Projects/kknow|мой проект]]");
    assert_eq!(listed[0].name.as_deref(), Some("проект"));
}

#[test]
fn tasks_are_listed_and_set_through_the_note() {
    let dir = vault();
    fs::write(
        dir.path().join("Index.md"),
        "# Оглавление\n\n- [ ] прочитать\n",
    )
    .unwrap();
    let app = app(&[dir.path()]);

    let listed = list_tasks(app.state(), None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Оглавление");
    assert_eq!((listed[0].line, listed[0].done), (3, false));
    assert_eq!(listed[0].text, "прочитать");

    let note = set_task(app.state(), "Index".into(), 3, true, None).unwrap();
    assert_eq!(note.content, "# Оглавление\n\n- [x] прочитать\n");
    assert!(list_tasks(app.state(), None).unwrap()[0].done);
    assert!(set_task(app.state(), "Index".into(), 1, true, None).is_err());
}

#[test]
fn a_deleted_note_comes_back_from_the_trash() {
    let dir = vault();
    let app = app(&[dir.path()]);

    delete_note(app.state(), "Projects/kknow".into(), None).unwrap();
    let listed = list_trash(app.state(), None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "Projects/kknow.md");
    assert_eq!(listed[0].notes, ["Projects/kknow.md"]);
    let html = trash_note(
        app.state(),
        listed[0].id.clone(),
        "Projects/kknow.md".into(),
        None,
    )
    .unwrap();
    assert!(html.contains("kknow"), "{html}");

    let restored = restore_trash(app.state(), listed[0].id.clone(), None).unwrap();
    assert_eq!(restored, ["Projects/kknow"]);
    assert!(dir.path().join("Projects/kknow.md").is_file());
    assert!(list_trash(app.state(), None).unwrap().is_empty());

    delete_note(app.state(), "Index".into(), None).unwrap();
    let entry = list_trash(app.state(), None).unwrap()[0].id.clone();
    delete_trash_entry(app.state(), entry, None).unwrap();
    assert!(list_trash(app.state(), None).unwrap().is_empty());

    delete_note(app.state(), "Projects/kknow".into(), None).unwrap();
    assert_eq!(empty_trash(app.state(), None).unwrap(), 1);
    assert!(list_trash(app.state(), None).unwrap().is_empty());
}

#[test]
fn rename_note_returns_the_new_id_and_moves_the_file() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let renamed = rename_note(
        app.state(),
        "Projects/kknow".into(),
        "Архив/kknow-old".into(),
        None,
    )
    .unwrap();
    let id = renamed.id;

    assert_eq!(id, "Архив/kknow-old");
    assert_eq!(renamed.relinked, 1, "ссылка из оглавления");
    assert!(dir.path().join("Архив/kknow-old.md").is_file());
    assert!(!dir.path().join("Projects/kknow.md").exists());
    assert!(load_note(app.state(), id, None).is_ok());
}

#[test]
fn rename_note_refuses_a_bad_name() {
    let dir = vault();
    let app = app(&[dir.path()]);

    assert!(rename_note(app.state(), "Index".into(), "../away".into(), None).is_err());
    assert!(rename_note(app.state(), "Index".into(), "Projects/kknow".into(), None).is_err());
    assert!(dir.path().join("Index.md").is_file());
}

#[test]
fn duplicate_note_makes_a_copy_next_to_the_note() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let copy = duplicate_note(app.state(), "Projects/kknow".into(), None).unwrap();

    assert_eq!(copy, "Projects/kknow copy");
    assert_eq!(list_notes(app.state(), None).unwrap().len(), 3);
}

#[test]
fn rename_folder_moves_the_notes_inside() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let relinked = rename_folder(app.state(), "Projects".into(), "Work".into(), None).unwrap();

    assert_eq!(relinked, 1, "ссылка из оглавления");
    let notes = list_notes(app.state(), None).unwrap();
    assert!(notes.iter().any(|note| note.id == "Work/kknow"));
    assert!(
        list_folders(app.state(), None)
            .unwrap()
            .contains(&"Work".to_string())
    );
}

#[test]
fn reveal_folder_refuses_paths_outside_the_vault() {
    let dir = vault();
    let app = app(&[dir.path()]);

    assert!(matches!(
        reveal_folder(app.state(), "../".into(), None),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        reveal_folder(app.state(), "Nope".into(), None),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn loads_a_note_with_html_and_backlinks() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let view = load_note(app.state(), "Projects/kknow".into(), None).unwrap();

    assert_eq!(view.title, "kknow");
    assert!(view.content.starts_with("# kknow"));
    assert!(view.html.contains("kknow</h1>"), "{}", view.html);
    assert_eq!(view.backlinks.len(), 1);
    assert_eq!(view.backlinks[0].id, "Index");
    assert_eq!(view.backlinks[0].label.as_deref(), Some("мой проект"));
}

#[test]
fn short_name_finds_the_note() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let view = load_note(app.state(), "kknow".into(), None).unwrap();
    assert_eq!(view.id, "Projects/kknow");
}

#[test]
fn saving_updates_links_immediately() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let SaveOutcome::Saved { note: view } = save_note(
        app.state(),
        "Projects/kknow".into(),
        "# kknow\n\nтеперь ссылаюсь на [[Index]]\n".into(),
        None,
        None,
        None,
    )
    .unwrap() else {
        panic!("без base сохранение безусловное");
    };

    assert_eq!(view.outgoing.len(), 1);
    assert_eq!(view.outgoing[0].id, "Index");

    let index = load_note(app.state(), "Index".into(), None).unwrap();
    assert_eq!(index.backlinks.len(), 1);
}

#[test]
fn saving_over_an_outside_edit_keeps_both_versions() {
    let dir = vault();
    let app = app(&[dir.path()]);
    let base = load_note(app.state(), "Index".into(), None)
        .unwrap()
        .content;
    fs::write(dir.path().join("Index.md"), "# Правка агента\n").unwrap();

    let SaveOutcome::Conflict { note, copy } = save_note(
        app.state(),
        "Index".into(),
        "# Правка в окне\n".into(),
        Some(base),
        None,
        None,
    )
    .unwrap() else {
        panic!("чужая правка должна была остановить запись");
    };

    assert_eq!(note.content, "# Правка агента\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("Index.md")).unwrap(),
        note.content
    );
    let mine = load_note(app.state(), copy, None).unwrap();
    assert_eq!(mine.content, "# Правка в окне\n");
}

#[test]
fn preview_renders_unsaved_text_without_touching_disk() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let html = preview(
        app.state(),
        "Index".into(),
        "черновик [[kknow]]".into(),
        None,
    )
    .unwrap();

    assert!(html.contains("kknow://note/Projects/kknow"), "{html}");
    assert_eq!(
        fs::read_to_string(dir.path().join("Index.md")).unwrap(),
        "# Оглавление\n\n[[Projects/kknow|мой проект]]\n"
    );
}

#[test]
fn note_backlinks_give_the_line_of_each_link() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let lines = note_backlinks(app.state(), "Projects/kknow".into(), None).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].id, "Index");
    assert_eq!(lines[0].title, "Оглавление");
    assert_eq!(lines[0].line, 3);
    assert_eq!(lines[0].text, "[[Projects/kknow|мой проект]]");
}

#[test]
fn a_mention_becomes_a_link() {
    let dir = vault();
    fs::write(dir.path().join("Notes.md"), "about kknow today\n").unwrap();
    let app = app(&[dir.path()]);

    let mentions = unlinked_mentions(app.state(), "Projects/kknow".into(), None).unwrap();
    assert_eq!(mentions.len(), 1);
    let mention = &mentions[0];
    assert_eq!(
        (mention.id.as_str(), mention.text.as_str()),
        ("Notes", "kknow")
    );

    let backlinks = link_mention(
        app.state(),
        mention.id.clone(),
        mention.start,
        mention.end,
        mention.text.clone(),
        "Projects/kknow".into(),
        None,
    )
    .unwrap();
    assert_eq!(backlinks.len(), 2);
    assert_eq!(
        fs::read_to_string(dir.path().join("Notes.md")).unwrap(),
        "about [[kknow]] today\n"
    );
}

#[test]
fn a_link_preview_shows_the_section_or_the_whole_note() {
    let dir = vault();
    fs::write(
        dir.path().join("Plan.md"),
        "# Plan\n\n## Goals\n\nShip it.\n\n## Done\n\nNothing.\n",
    )
    .unwrap();
    let app = app(&[dir.path()]);

    let goals = link_preview(app.state(), "Index".into(), "Plan#Goals|g".into(), None)
        .unwrap()
        .unwrap();
    assert_eq!((goals.id.as_str(), goals.line), ("Plan", Some(3)));
    assert!(goals.html.contains("Ship it.") && !goals.html.contains("Nothing."));
    assert!(!goals.part_missing);

    let ghost = link_preview(app.state(), "Index".into(), "Plan#Ghost".into(), None)
        .unwrap()
        .unwrap();
    assert!(ghost.part_missing && ghost.html.contains("Nothing."));

    let missing = link_preview(app.state(), "Index".into(), "Nobody".into(), None).unwrap();
    assert!(missing.is_none());
}

#[test]
fn link_target_finds_the_note_and_its_headings() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let found = link_target(app.state(), "Index".into(), "kknow#kknow".into(), None)
        .unwrap()
        .unwrap();
    assert_eq!(found.id, "Projects/kknow");
    assert_eq!(found.line, Some(1));
    assert_eq!(found.headings[0].text, "kknow");

    let missing = link_target(app.state(), "Index".into(), "Ghost".into(), None).unwrap();
    assert!(missing.is_none());
}

#[test]
fn creating_and_deleting_a_note() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let created = create_note(app.state(), "Daily/2026-07-18".into(), None).unwrap();
    assert_eq!(created.id, "Daily/2026-07-18");
    assert!(dir.path().join("Daily/2026-07-18.md").is_file());
    assert_eq!(list_notes(app.state(), None).unwrap().len(), 3);

    delete_note(app.state(), "Daily/2026-07-18".into(), None).unwrap();
    assert!(!dir.path().join("Daily/2026-07-18.md").exists());
    assert_eq!(list_notes(app.state(), None).unwrap().len(), 2);
}

#[test]
fn missing_note_returns_an_error_not_a_panic() {
    let dir = vault();
    let app = app(&[dir.path()]);

    assert!(matches!(
        load_note(app.state(), "нет такой".into(), None),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn search_finds_notes_by_content() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let results = search(app.state(), "rust".into(), None, None).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "Projects/kknow");
    assert_eq!(results[0].folder.as_deref(), Some("Projects"));
}

#[test]
fn search_sees_a_note_saved_a_moment_ago() {
    let dir = vault();
    let app = app(&[dir.path()]);

    save_note(
        app.state(),
        "Index".into(),
        "# Оглавление\n\nупомянул компиляторы\n".into(),
        None,
        None,
        None,
    )
    .unwrap();

    let results = search(app.state(), "компиляторы".into(), None, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "Index");
}

#[test]
fn search_on_empty_query_returns_nothing() {
    let dir = vault();
    let app = app(&[dir.path()]);

    assert!(
        search(app.state(), "  ".into(), None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn commands_report_an_empty_registry() {
    let app = app(&[]);

    assert!(current_vault(app.state()).is_none());
    assert!(matches!(list_notes(app.state(), None), Err(Error::NoVault)));
}

#[test]
fn themes_come_from_the_vault_and_a_vault_without_them_is_fine() {
    let dir = vault();
    let app = app(&[dir.path()]);

    let own = |set: ThemeSet| -> Vec<theme::Theme> {
        set.themes
            .into_iter()
            .filter(|theme| theme.source != theme::Source::BuiltIn)
            .collect()
    };
    assert!(own(list_themes(app.state(), None).unwrap()).is_empty());

    fs::create_dir_all(dir.path().join(".kknow/themes")).unwrap();
    fs::write(
        dir.path().join(".kknow/themes/sepia.json"),
        r##"{"name":"Сепия","base":"light","variables":{"bg":"#f4ecd8"}}"##,
    )
    .unwrap();

    let themes = own(list_themes(app.state(), None).unwrap());
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].id, "sepia");
    assert_eq!(themes[0].variables["bg"], "#f4ecd8");
}

#[test]
fn global_themes_are_seen_from_any_vault_and_without_one() {
    let first = vault();
    let second = vault();
    let global = tempfile::tempdir().unwrap();
    let app = app(&[first.path(), second.path()]);
    app.state::<AppState>()
        .use_themes(Some(global.path().join("themes")));

    fs::create_dir_all(first.path().join(".kknow/themes")).unwrap();
    fs::write(
        first.path().join(".kknow/themes/sepia.json"),
        r#"{"name":"Сепия"}"#,
    )
    .unwrap();
    let first_id = app.state::<AppState>().entries()[0].id.clone();
    let second_id = app.state::<AppState>().entries()[1].id.clone();

    let own = |set: ThemeSet| -> Vec<theme::Theme> {
        set.themes
            .into_iter()
            .filter(|theme| theme.source != theme::Source::BuiltIn)
            .collect()
    };
    assert!(own(list_themes(app.state(), Some(second_id.clone())).unwrap()).is_empty());

    share_theme(app.state(), "sepia".into(), Some(first_id)).unwrap();

    let themes = own(list_themes(app.state(), Some(second_id)).unwrap());
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].source, theme::Source::Global);
    assert!(matches!(
        share_theme(app.state(), "../x".into(), None),
        Err(Error::ShareTheme(_))
    ));

    let empty = self::app(&[]);
    empty
        .state::<AppState>()
        .use_themes(Some(global.path().join("themes")));
    assert_eq!(own(list_themes(empty.state(), None).unwrap()).len(), 1);
}

#[test]
fn reveal_resolves_a_note_path_and_falls_back_to_the_vault() {
    let dir = vault();
    let app = app(&[dir.path()]);
    let state = app.state::<AppState>();

    state
        .slot(None)
        .unwrap()
        .read(|workspace| {
            assert_eq!(
                reveal_target(workspace, Some("kknow"))?,
                workspace.root().join("Projects/kknow.md")
            );
            assert_eq!(reveal_target(workspace, None)?, workspace.root());
            assert!(matches!(
                reveal_target(workspace, Some("нет такой")),
                Err(Error::NotFound(_))
            ));
            Ok(())
        })
        .unwrap();
}

fn other_vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Смета.md"),
        "# Смета\n\nрабочие расходы и компиляторы\n",
    )
    .unwrap();
    dir
}

#[test]
fn every_command_works_with_the_active_vault_by_default() {
    let mine = vault();
    let work = other_vault();
    let app = app(&[mine.path(), work.path()]);
    let ids: Vec<String> = app
        .state::<AppState>()
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();

    assert_eq!(list_notes(app.state(), None).unwrap().len(), 2);
    assert_eq!(
        list_notes(app.state(), Some(ids[1].clone())).unwrap().len(),
        1,
        "чужое хранилище доступно по имени"
    );

    let switched = activate_vault(app.state(), ids[1].clone()).unwrap();
    assert_eq!(switched.id, ids[1]);
    assert_eq!(list_notes(app.state(), None).unwrap()[0].id, "Смета");
}

#[test]
fn a_vault_can_be_added_and_forgotten() {
    let mine = vault();
    let work = other_vault();
    let app = app(&[mine.path()]);

    let added = add_vault(app.state(), work.path().to_string_lossy().into_owned()).unwrap();
    assert!(list_vaults(app.state()).iter().any(|view| view.active));
    assert_eq!(list_vaults(app.state()).len(), 2);
    assert_eq!(current_vault(app.state()).unwrap().id, added.id);

    let left = remove_vault(app.state(), added.id).unwrap();
    assert_eq!(left.len(), 1);
    assert!(work.path().join("Смета.md").is_file(), "файлы остаются");
}

#[test]
fn search_covers_every_vault_and_says_where_it_found() {
    let mine = vault();
    let work = other_vault();
    let app = app(&[mine.path(), work.path()]);
    let ids: Vec<String> = app
        .state::<AppState>()
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();

    save_note(
        app.state(),
        "Index".into(),
        "# Оглавление\n\nсвои компиляторы\n".into(),
        None,
        None,
        None,
    )
    .unwrap();

    let found = search_all(app.state(), "компиляторы".into(), None, None).unwrap();

    assert_eq!(found.len(), 2, "находки из обоих хранилищ");
    assert_eq!(found[0].vault, ids[0]);
    assert_eq!(found[0].id, "Index");
    assert_eq!(found[1].vault, ids[1]);
    assert_eq!(found[1].id, "Смета");
    assert!(!found[1].vault_name.is_empty());

    let found = search_all(
        app.state(),
        "компиляторы".into(),
        None,
        Some(vec![ids[1].clone()]),
    )
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].vault, ids[1]);
}

#[test]
fn a_global_search_opens_vaults_it_has_not_touched_yet() {
    let mine = vault();
    let work = other_vault();
    let app = app(&[mine.path(), work.path()]);
    let state = app.state::<AppState>();
    let ids: Vec<String> = state
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();

    assert!(state.opened(&ids[1]).is_none(), "второе ещё не открывали");
    search_all(app.state(), "расходы".into(), None, None).unwrap();

    assert!(state.opened(&ids[1]).is_some(), "поиск открыл его сам");
}

#[test]
fn hotkeys_point_at_vaults_in_list_order() {
    let mine = vault();
    let work = other_vault();
    let app = app(&[mine.path(), work.path()]);
    let ids: Vec<String> = app
        .state::<AppState>()
        .entries()
        .iter()
        .map(|entry| entry.id.clone())
        .collect();

    assert_eq!(vault_by_hotkey(app.state(), 1).as_ref(), Some(&ids[0]));
    assert_eq!(vault_by_hotkey(app.state(), 2).as_ref(), Some(&ids[1]));
    assert!(vault_by_hotkey(app.state(), 3).is_none());
}

#[test]
fn duplicate_links_appear_once_in_backlinks() {
    let dir = vault();
    let app = app(&[dir.path()]);

    save_note(
        app.state(),
        "Index".into(),
        "[[kknow]] и снова [[Projects/kknow]] и ещё ![[kknow]]\n".into(),
        None,
        None,
        None,
    )
    .unwrap();

    let view = load_note(app.state(), "kknow".into(), None).unwrap();
    assert_eq!(view.backlinks.len(), 1);
}
