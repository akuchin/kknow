// The markup of the main window and the elements that the window code uses.
//
// The module does not touch the DOM on import: `main.ts` calls `initDom` at
// the point where it used to build the markup.

import type { ViewMode } from "../api";
import { shortcut, withShortcut } from "../actions";
import { iconMarkup, type IconName } from "../icons";
import { t } from "../i18n";
import { MODES } from "../note-mode";

const MODE_ICONS: Record<ViewMode, IconName> = { edit: "pencil", split: "split", read: "book" };

/** The root of the window: a grid of the bars, the sidebar and the panes. */
export let app: HTMLDivElement;

function query() {
  return {
    noteMenu: document.querySelector<HTMLButtonElement>("#note-menu")!,
    noteTitle: document.querySelector<HTMLSpanElement>("#note-title")!,
    noteFolder: document.querySelector<HTMLSpanElement>("#note-folder")!,
    noteFolderText: document.querySelector<HTMLSpanElement>("#note-folder-text")!,
    noteVaultLabel: document.querySelector<HTMLSpanElement>("#note-vault-label")!,
    toggleSidebar: document.querySelector<HTMLButtonElement>("#toggle-sidebar")!,
    navBack: document.querySelector<HTMLButtonElement>("#nav-back")!,
    navForward: document.querySelector<HTMLButtonElement>("#nav-forward")!,
    modeButtons: [...document.querySelectorAll<HTMLButtonElement>(".mode-switch button")],
    statusbar: document.querySelector<HTMLElement>("#statusbar")!,
    openPalette: document.querySelector<HTMLButtonElement>("#open-palette")!,
    newNote: document.querySelector<HTMLButtonElement>("#new-note")!,
    newNoteForm: document.querySelector<HTMLFormElement>("#new-note-form")!,
    newNoteName: document.querySelector<HTMLInputElement>("#new-note-name")!,
    addVault: document.querySelector<HTMLButtonElement>("#add-vault")!,
    noteList: document.querySelector<HTMLUListElement>("#note-list")!,
    labelFilter: document.querySelector<HTMLDivElement>("#label-filter")!,
    openFile: document.querySelector<HTMLButtonElement>("#open-file")!,
    fileList: document.querySelector<HTMLUListElement>("#file-list")!,
    editor: document.querySelector<HTMLElement>("#editor")!,
    preview: document.querySelector<HTMLElement>("#preview")!,
    toggleAside: document.querySelector<HTMLButtonElement>("#toggle-aside")!,
    asideOutline: document.querySelector<HTMLDetailsElement>("#aside-outline")!,
    asideOutlineList: document.querySelector<HTMLOListElement>("#aside-outline-list")!,
    asideOutlineCount: document.querySelector<HTMLSpanElement>("#aside-outline-count")!,
    asideBacklinks: document.querySelector<HTMLDetailsElement>("#aside-backlinks")!,
    asideBacklinksList: document.querySelector<HTMLUListElement>("#aside-backlinks-list")!,
    asideBacklinksCount: document.querySelector<HTMLSpanElement>("#aside-backlinks-count")!,
    asideMentions: document.querySelector<HTMLDetailsElement>("#aside-mentions")!,
    asideMentionsList: document.querySelector<HTMLUListElement>("#aside-mentions-list")!,
    asideMentionsCount: document.querySelector<HTMLSpanElement>("#aside-mentions-count")!,
    syncPanel: document.querySelector<HTMLElement>("#sync-panel")!,
    syncSummary: document.querySelector<HTMLParagraphElement>("#sync-summary")!,
    syncList: document.querySelector<HTMLUListElement>("#sync-list")!,
    syncHide: document.querySelector<HTMLButtonElement>("#sync-hide")!,
    gitView: document.querySelector<HTMLElement>("#git-view")!,
    gitSummary: document.querySelector<HTMLParagraphElement>("#git-summary")!,
    gitBody: document.querySelector<HTMLDivElement>("#git-body")!,
    gitSync: document.querySelector<HTMLButtonElement>("#git-sync")!,
    gitHide: document.querySelector<HTMLButtonElement>("#git-hide")!,
    gitDiffHead: document.querySelector<HTMLElement>("#git-diff-head")!,
    gitDiffBody: document.querySelector<HTMLPreElement>("#git-diff-body")!,
    runFloat: document.querySelector<HTMLElement>("#run-float")!,
    trashView: document.querySelector<HTMLElement>("#trash-view")!,
    trashViewTitle: document.querySelector<HTMLSpanElement>("#trash-view-title")!,
    trashViewAge: document.querySelector<HTMLSpanElement>("#trash-view-age")!,
    trashViewRestore: document.querySelector<HTMLButtonElement>("#trash-view-restore")!,
    trashViewClose: document.querySelector<HTMLButtonElement>("#trash-view-close")!,
    trashViewBody: document.querySelector<HTMLElement>("#trash-view-body")!,
    runTitle: document.querySelector<HTMLSpanElement>("#run-title")!,
    runSummary: document.querySelector<HTMLParagraphElement>("#run-summary")!,
    runOutput: document.querySelector<HTMLPreElement>("#run-output")!,
    runHide: document.querySelector<HTMLButtonElement>("#run-hide")!,
    runStop: document.querySelector<HTMLButtonElement>("#run-stop")!,
    toggleNotes: document.querySelector<HTMLButtonElement>("#toggle-notes")!,
    notesCount: document.querySelector<HTMLSpanElement>("#notes-count")!,
    toggleProposals: document.querySelector<HTMLButtonElement>("#toggle-proposals")!,
    proposalsCount: document.querySelector<HTMLSpanElement>("#proposals-count")!,
    noteTools: document.querySelector<HTMLElement>("#note-tools")!,
    noteBookmarks: document.querySelector<HTMLButtonElement>("#note-bookmarks")!,
    bookmarksCount: document.querySelector<HTMLSpanElement>("#bookmarks-count")!,
    runLast: document.querySelector<HTMLButtonElement>("#run-last")!,
    runMenu: document.querySelector<HTMLButtonElement>("#run-menu")!,
    noteBacklinks: document.querySelector<HTMLButtonElement>("#note-backlinks")!,
    backlinksCount: document.querySelector<HTMLSpanElement>("#backlinks-count")!,
    newFolder: document.querySelector<HTMLButtonElement>("#new-folder")!,
    collapseAll: document.querySelector<HTMLButtonElement>("#collapse-all")!,
    notesFloat: document.querySelector<HTMLElement>("#notes-float")!,
    notesList: document.querySelector<HTMLUListElement>("#notes-list")!,
    notesHide: document.querySelector<HTMLButtonElement>("#notes-hide")!,
    proposalsFloat: document.querySelector<HTMLElement>("#proposals-float")!,
    proposalsList: document.querySelector<HTMLUListElement>("#proposals-list")!,
    proposalsHide: document.querySelector<HTMLButtonElement>("#proposals-hide")!,
  };
}

/** Window elements by role. `initDom` fills it. */
export const el = {} as ReturnType<typeof query>;

/** Builds the window markup and finds its elements. Call it once, first. */
export function initDom(): void {
  app = document.querySelector<HTMLDivElement>("#app")!;
  app.innerHTML = `
  <header class="topbar">
    <!-- Above the sidebar: the sidebar toggle and navigation history. It has the
         sidebar width, so the note name sits above the editor. -->
    <div class="topbar-nav">
      <button id="toggle-sidebar" class="ghost bare" aria-expanded="true" aria-controls="sidebar">
        ${iconMarkup("sidebar")}
      </button>
      <span class="spacer"></span>
      <button id="nav-back" class="ghost bare" disabled
              title="${withShortcut(t("topbar.back"), "history.back")}"
              aria-label="${t("topbar.back")}">
        ${iconMarkup("back")}
      </button>
      <button id="nav-forward" class="ghost bare" disabled
              title="${withShortcut(t("topbar.forward"), "history.forward")}"
              aria-label="${t("topbar.forward")}">
        ${iconMarkup("chevron")}
      </button>
    </div>

    <div class="topbar-note">
      <button id="note-menu" class="ghost topbar-note-title" aria-haspopup="menu" disabled
              title="${t("topbar.note.menu")}">
        <span id="note-title">${t("topbar.note.empty")}</span>${iconMarkup("caret")}
      </button>
      <span id="note-folder" class="topbar-note-folder" hidden>
        <span class="vault-label" id="note-vault-label"></span>
        <span id="note-folder-text"></span>
      </span>
    </div>

    <div class="topbar-actions">
      <button id="toggle-notes" class="ghost" aria-pressed="false" hidden
              title="${t("topbar.notes.title")}">
        ${iconMarkup("comment")}<span id="notes-count"></span>
      </button>
      <button id="toggle-proposals" class="ghost" aria-pressed="false" hidden
              title="${t("topbar.proposals.title")}">
        ${iconMarkup("pencil")}<span id="proposals-count"></span>
      </button>
      <!-- Note toolbar: the frequent note actions that the note menu hides.
           The menu keeps them too; these buttons only make them one click
           away and show their state. -->
      <div id="note-tools" class="note-tools" role="toolbar"
           aria-label="${t("topbar.tools")}" hidden>
        <button id="note-bookmarks" class="ghost bare" aria-haspopup="menu"
                title="${t("topbar.bookmarks.title")}" aria-label="${t("topbar.bookmarks.title")}">
          ${iconMarkup("bookmark")}<span id="bookmarks-count"></span>
        </button>
        <div class="split-button">
          <button id="run-last" class="ghost bare" title="${t("topbar.commands.title")}"
                  aria-label="${t("topbar.commands.title")}">
            ${iconMarkup("play")}
          </button>
          <button id="run-menu" class="ghost bare" aria-haspopup="menu"
                  title="${t("topbar.commands.title")}" aria-label="${t("topbar.commands.title")}">
            ${iconMarkup("caret")}
          </button>
        </div>
        <button id="note-backlinks" class="ghost bare" aria-haspopup="menu"
                title="${t("topbar.backlinks.title")}" aria-label="${t("topbar.backlinks.title")}">
          ${iconMarkup("backlink")}<span id="backlinks-count"></span>
        </button>
      </div>
      <div class="mode-switch" role="group" aria-label="${t("topbar.mode")}"
           title="${withShortcut(t("topbar.mode"), "note.mode")}">
        ${MODES.map(
          (mode) => `
        <button data-mode="${mode}" aria-pressed="false" aria-label="${t(`mode.${mode}`)}"
                title="${t(`mode.${mode}.title`)}">
          ${iconMarkup(MODE_ICONS[mode])}
        </button>`,
        ).join("")}
      </div>
      <button id="open-palette" class="search-button"
              title="${withShortcut(t("topbar.search.title"), "palette.open")}">
        ${iconMarkup("search")}
        <span>${t("topbar.search")}</span>
        <kbd>${shortcut("palette.open")}</kbd>
      </button>
      <button id="toggle-aside" class="ghost bare" aria-expanded="true" aria-controls="note-aside"
              title="${withShortcut(t("topbar.aside"), "aside.toggle")}"
              aria-label="${t("topbar.aside")}" hidden>
        ${iconMarkup("aside")}
      </button>
    </div>
  </header>

  <aside class="sidebar" id="sidebar">
    <!-- Sidebar toolbar: actions on the tree. New note and folder go to the
         folder of the open note in the active vault, as ⌘N does. -->
    <div class="sidebar-head">
      <span class="sidebar-title">${t("sidebar.vaults")}</span>
      <div class="sidebar-tools" role="toolbar" aria-label="${t("sidebar.tools")}">
        <button id="new-note" class="ghost bare"
                title="${withShortcut(t("topbar.newNote.title"), "note.new")}"
                aria-label="${t("topbar.newNote.title")}">
          ${iconMarkup("filePlus")}
        </button>
        <button id="new-folder" class="ghost bare" title="${t("sidebar.newFolder.title")}"
                aria-label="${t("sidebar.newFolder.title")}">
          ${iconMarkup("folderPlus")}
        </button>
        <button id="collapse-all" class="ghost bare" title="${t("sidebar.collapseAll")}"
                aria-label="${t("sidebar.collapseAll")}">
          ${iconMarkup("collapse")}
        </button>
        <button id="add-vault" class="ghost bare" title="${t("menu.vault.add")}"
                aria-label="${t("menu.vault.add")}">
          ${iconMarkup("folder")}
        </button>
      </div>
    </div>
    <!-- The label filter: one row for all vaults, not labels on each row.
         It shows only when some vault has a label. -->
    <div id="label-filter" class="label-filter" role="group"
         aria-label="${t("sidebar.labels")}" hidden></div>
    <form id="new-note-form" class="new-note" hidden>
      <input id="new-note-name" type="text" placeholder="${t("sidebar.newNote.placeholder")}"
             autocomplete="off" />
    </form>
    <ul id="note-list" class="note-list"></ul>
    <!-- Separate files: markdown opened without a vault. They get their own
         section below the tree, not a root in it: a file has no folders, no
         search and no sync, which is what makes a row a vault. -->
    <section class="files" aria-labelledby="files-title">
      <div class="sidebar-head">
        <span id="files-title" class="sidebar-title">${t("sidebar.files")}</span>
        <button id="open-file" class="ghost bare" title="${withShortcut(t("sidebar.files.open"), "file.open")}"
                aria-label="${t("sidebar.files.open")}">
          ${iconMarkup("note")}
        </button>
      </div>
      <ul id="file-list" class="note-list file-list"></ul>
    </section>
  </aside>

  <main class="editor" id="editor"></main>

  <section class="preview-pane">
    <article id="preview" class="preview"></article>
    <section id="sync-panel" class="panel" hidden>
      <h2>
        ${t("panel.sync.title")}
        <button id="sync-hide" class="ghost bare" title="${t("panel.sync.hide")}"
                aria-label="${t("panel.sync.hide")}">
          ${iconMarkup("close")}
        </button>
      </h2>
      <p id="sync-summary" class="muted"></p>
      <ul id="sync-list"></ul>
    </section>
  </section>

  <!-- The side panel of the note, in all three modes: the outline, the
       backlinks and the mentions without a link. -->
  <aside id="note-aside" class="note-aside" aria-label="${t("aside.title")}">
    <details id="aside-outline" class="aside-section">
      <summary>${t("aside.outline")}<span id="aside-outline-count" class="aside-count"></span></summary>
      <ol id="aside-outline-list" class="aside-outline"></ol>
    </details>
    <details id="aside-backlinks" class="aside-section">
      <summary>${t("aside.backlinks")}<span id="aside-backlinks-count" class="aside-count"></span></summary>
      <ul id="aside-backlinks-list" class="aside-sources"></ul>
    </details>
    <details id="aside-mentions" class="aside-section">
      <summary>${t("aside.mentions")}<span id="aside-mentions-count" class="aside-count"></span></summary>
      <ul id="aside-mentions-list" class="aside-sources"></ul>
    </details>
  </aside>

  <!-- The changes of a git vault. Like the trash view, it covers the editor
       and the preview: a narrow list of files on the left (staged, changed,
       the commit message) and the diff of the picked file on the right. -->
  <section id="git-view" class="git-view" aria-labelledby="git-title" hidden>
    <div class="git-side">
      <h2>
        <span id="git-title">${t("panel.git.title")}</span>
        <span class="panel-tools">
          <button id="git-sync" class="ghost bare" title="${withShortcut(t("palette.action.sync"), "vault.sync")}"
                  aria-label="${t("palette.action.sync")}">
            ${iconMarkup("refresh")}
          </button>
          <button id="git-hide" class="ghost bare" title="${t("panel.git.hide")}"
                  aria-label="${t("panel.git.hide")}">
            ${iconMarkup("close")}
          </button>
        </span>
      </h2>
      <p id="git-summary" class="muted"></p>
      <div id="git-body" class="git-body"></div>
    </div>
    <section class="git-diff">
      <header id="git-diff-head" class="git-diff-head"></header>
      <pre id="git-diff-body" class="git-diff-body"></pre>
    </section>
  </section>

  <!-- A note from the trash, read-only. It covers the editor and the preview
       but does not replace the open note: Close shows that note again. -->
  <section id="trash-view" class="trash-view" aria-labelledby="trash-view-title" hidden>
    <header class="trash-view-head">
      ${iconMarkup("trash")}
      <span id="trash-view-title" class="trash-view-title"></span>
      <span class="trash-view-badge">${t("trashView.readOnly")}</span>
      <span id="trash-view-age" class="muted"></span>
      <span class="spacer"></span>
      <button id="trash-view-restore" class="primary">${t("sidebar.trash.restore")}</button>
      <button id="trash-view-close" class="ghost bare" title="${t("trashView.close")}"
              aria-label="${t("trashView.close")}">
        ${iconMarkup("close")}
      </button>
    </header>
    <article id="trash-view-body" class="preview"></article>
  </section>

  <footer class="statusbar" id="statusbar"></footer>

  <!-- Agent notes in the open text. A float over the editor, not a section of
       the right pane: the right pane hides with the preview, and the list is
       most useful during editing. -->
  <section id="notes-float" class="notes-float" role="dialog" aria-labelledby="notes-title" hidden>
    <h2>
      <span id="notes-title">${t("panel.notes.title")}</span>
      <button id="notes-hide" class="ghost bare" title="${t("panel.notes.hide")}"
              aria-label="${t("panel.notes.hide")}">
        ${iconMarkup("close")}
      </button>
    </h2>
    <ul id="notes-list" class="notes-list"></ul>
  </section>

  <section id="proposals-float" class="notes-float proposals-float" role="dialog"
           aria-labelledby="proposals-title" hidden>
    <h2>
      <span id="proposals-title">${t("panel.proposals.title")}</span>
      <button id="proposals-hide" class="ghost bare" title="${t("panel.notes.hide")}"
              aria-label="${t("panel.notes.hide")}">
        ${iconMarkup("close")}
      </button>
    </h2>
    <ul id="proposals-list" class="notes-list proposals-list"></ul>
  </section>

  <!-- Command output: a float near the status bar, not a panel in the right
       pane. Commands also run with the preview closed, and usually the "done"
       line in the status bar is enough. -->
  <section id="run-float" class="run-float" role="dialog" aria-labelledby="run-title" hidden>
    <h2>
      <span id="run-title">${t("panel.run.title")}</span>
      <span class="run-tools">
        <button id="run-stop" class="ghost bare" title="${t("panel.run.stop")}"
                aria-label="${t("panel.run.stop")}" hidden>
          ${iconMarkup("stop")}
        </button>
        <button id="run-hide" class="ghost bare" title="${t("panel.run.hide")}"
                aria-label="${t("panel.run.hide")}">
          ${iconMarkup("close")}
        </button>
      </span>
    </h2>
    <p id="run-summary" class="muted"></p>
    <pre id="run-output" class="run-output"></pre>
  </section>
`;
  Object.assign(el, query());
}
