import { api, type BacklinkLineView, type MentionView } from "../api";
import { t } from "../i18n";
import { currentHeading, type Heading } from "../outline";
import { bottomLine, scrollToLine, topLine } from "../preview-lines";
import { app, el } from "./dom";
import { editor } from "./editor";
import { goToHeading } from "./marks";
import { openNote, setBacklinks } from "./note";
import { state } from "./state";
import { fail, setStatus } from "./status";

const SHOWN = "kknow:aside";
const SECTIONS = "kknow:aside:sections";

const OUTLINE_DELAY = 250;

const LINES_SHOWN = 3;

type Section = "outline" | "backlinks" | "mentions";

let hidden = read(SHOWN) === "hidden";
let headings: Heading[] = [];
let outlineTimer: number | undefined;
let scrollFrame = 0;

let linksKey: string | null = null;
let mentionsKey: string | null = null;

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
  }
}

function noteKey(): string | null {
  const note = state.note;
  if (!note || note.file) return null;
  return `${state.vault?.id ?? ""}/${note.id}`;
}

function details(section: Section): HTMLDetailsElement {
  return { outline: el.asideOutline, backlinks: el.asideBacklinks, mentions: el.asideMentions }[
    section
  ];
}

function apply(): void {
  const visible = !hidden && state.note !== null;
  app.dataset.aside = visible ? "shown" : "hidden";
  el.toggleAside.setAttribute("aria-expanded", String(!hidden));
  el.toggleAside.hidden = state.note === null;
  if (visible) refresh();
}

export function toggleAside(): void {
  hidden = !hidden;
  write(SHOWN, hidden ? "hidden" : "shown");
  apply();
}

export function showAside(section: Section): void {
  if (hidden) toggleAside();
  const box = details(section);
  box.open = true;
  box.scrollIntoView({ block: "nearest" });
}

function refresh(): void {
  if (hidden || !state.note) return;
  const file = state.note.file === true;
  el.asideBacklinks.hidden = file;
  el.asideMentions.hidden = file;
  renderOutline();
  if (file) return;
  if (el.asideBacklinks.open && linksKey !== noteKey()) void loadBacklinks();
  if (el.asideMentions.open && mentionsKey !== noteKey()) void loadMentions();
}

export function refreshAsideLinks(): void {
  linksKey = null;
  mentionsKey = null;
  refresh();
}

function edited(): void {
  if (hidden) return;
  window.clearTimeout(outlineTimer);
  outlineTimer = window.setTimeout(renderOutline, OUTLINE_DELAY);
}

let drawn = "";

function renderOutline(): void {
  headings = editor.getHeadings();
  const key = JSON.stringify(headings);
  if (key === drawn && el.asideOutlineList.childElementCount > 0) {
    markCurrent();
    return;
  }
  drawn = key;
  const list = el.asideOutlineList;
  list.replaceChildren();
  el.asideOutlineCount.textContent = headings.length > 0 ? String(headings.length) : "";
  if (headings.length === 0) {
    list.append(empty(t("aside.outline.empty")));
    return;
  }
  const base = Math.min(...headings.map((heading) => heading.level));
  for (const heading of headings) {
    const item = document.createElement("li");
    item.style.setProperty("--depth", String(heading.level - base));
    const button = document.createElement("button");
    button.className = "aside-heading";
    button.textContent = heading.text || "…";
    button.title = heading.text;
    button.onclick = () => goToHeading(heading);
    item.append(button);
    list.append(item);
  }
  markCurrent();
}

function markCurrent(): void {
  scrollFrame = 0;
  if (hidden || headings.length === 0) return;
  const read = state.mode === "read";
  const scroller = read ? el.preview : editor.scroller;
  const top = read ? (topLine(el.preview) ?? 1) : editor.topLine();
  const atEnd =
    scroller.scrollTop > 0 &&
    scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 4;
  const bottom = !atEnd ? null : read ? bottomLine(el.preview) : editor.bottomLine();
  const current = currentHeading(headings, top, bottom);
  const items = el.asideOutlineList.children;
  for (const [index, item] of [...items].entries()) {
    const on = index === current;
    item.classList.toggle("is-current", on);
    if (on) item.firstElementChild?.setAttribute("aria-current", "location");
    else item.firstElementChild?.removeAttribute("aria-current");
  }
}

function onScroll(): void {
  if (hidden || scrollFrame) return;
  scrollFrame = window.requestAnimationFrame(markCurrent);
}

function empty(text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "empty";
  item.textContent = text;
  return item;
}

function lineText(line: { before: string; text: string; after: string }): HTMLElement {
  const text = document.createElement("span");
  text.className = "aside-line-text";
  const mark = document.createElement("mark");
  mark.textContent = line.text;
  text.append(line.before, mark, line.after);
  return text;
}

async function openAt(id: string, line: number): Promise<void> {
  await openNote(id);
  if (state.note?.id !== id) return;
  if (state.mode === "read") scrollToLine(el.preview, line);
  else editor.goToLine(line);
}

function bySource<T extends { id: string; title: string }>(rows: T[]): [T, T[]][] {
  const groups = new Map<string, [T, T[]]>();
  for (const row of rows) {
    const group = groups.get(row.id);
    if (group) group[1].push(row);
    else groups.set(row.id, [row, [row]]);
  }
  return [...groups.values()];
}

function fillLines<T>(list: HTMLUListElement, rows: T[], make: (row: T) => HTMLLIElement): void {
  for (const row of rows.slice(0, LINES_SHOWN)) list.append(make(row));
  const rest = rows.slice(LINES_SHOWN);
  if (rest.length === 0) return;
  const item = document.createElement("li");
  const more = document.createElement("button");
  more.className = "link aside-more";
  more.textContent = t("aside.more", { count: rest.length });
  more.onclick = () => item.replaceWith(...rest.map(make));
  item.append(more);
  list.append(item);
}

function sourceItem(first: { id: string; title: string }): [HTMLLIElement, HTMLUListElement] {
  const item = document.createElement("li");
  item.className = "aside-source";
  const title = document.createElement("button");
  title.className = "link";
  title.textContent = first.title;
  title.title = first.id;
  title.onclick = () => void openNote(first.id);
  const lines = document.createElement("ul");
  item.append(title, lines);
  return [item, lines];
}

async function loadBacklinks(): Promise<void> {
  const key = noteKey();
  const note = state.note;
  if (!key || !note) return;
  linksKey = key;
  try {
    const rows = await api.noteBacklinks(note.id);
    if (noteKey() !== key) return;
    renderBacklinks(rows);
  } catch (error) {
    linksKey = null;
    fail(error);
  }
}

function renderBacklinks(rows: BacklinkLineView[]): void {
  const list = el.asideBacklinksList;
  list.replaceChildren();
  const sources = bySource(rows);
  el.asideBacklinksCount.textContent = sources.length > 0 ? String(sources.length) : "";
  if (rows.length === 0) {
    list.append(empty(t("aside.backlinks.empty")));
    return;
  }
  for (const [first, lines] of sources) {
    const [item, sub] = sourceItem(first);
    fillLines(sub, lines, (line) => {
      const row = document.createElement("li");
      const button = document.createElement("button");
      button.className = "aside-line";
      button.title = t("aside.line", { line: line.line });
      button.append(lineText(line));
      button.onclick = () => void openAt(line.id, line.line);
      row.append(button);
      return row;
    });
    list.append(item);
  }
}

async function loadMentions(): Promise<void> {
  const key = noteKey();
  const note = state.note;
  if (!key || !note) return;
  mentionsKey = key;
  el.asideMentionsList.replaceChildren(empty(t("aside.mentions.loading")));
  try {
    const rows = await api.unlinkedMentions(note.id);
    if (noteKey() !== key) return;
    renderMentions(rows);
  } catch (error) {
    mentionsKey = null;
    fail(error);
  }
}

function renderMentions(rows: MentionView[]): void {
  const list = el.asideMentionsList;
  list.replaceChildren();
  const sources = bySource(rows);
  el.asideMentionsCount.textContent = sources.length > 0 ? String(sources.length) : "";
  if (rows.length === 0) {
    list.append(empty(t("aside.mentions.empty")));
    return;
  }
  for (const [first, mentions] of sources) {
    const [item, sub] = sourceItem(first);
    fillLines(sub, mentions, (mention) => {
      const row = document.createElement("li");
      row.className = "aside-mention";
      const open = document.createElement("button");
      open.className = "aside-line";
      open.title = t("aside.line", { line: mention.line });
      open.append(lineText(mention));
      open.onclick = () => void openAt(mention.id, mention.line);
      const link = document.createElement("button");
      link.className = "ghost aside-link";
      link.textContent = t("aside.mentions.link");
      link.title = t("aside.mentions.link.title", { text: mention.text });
      link.onclick = () => void linkMention(mention, link);
      row.append(open, link);
      return row;
    });
    list.append(item);
  }
}

async function linkMention(mention: MentionView, button: HTMLButtonElement): Promise<void> {
  const note = state.note;
  if (!note || note.file) return;
  button.disabled = true;
  try {
    const backlinks = await api.linkMention(mention, note.id);
    if (state.note?.id !== note.id) return;
    setStatus(t("status.mentionLinked", { name: mention.title }));
    linksKey = null;
    mentionsKey = null;
    setBacklinks(backlinks);
  } catch (error) {
    button.disabled = false;
    mentionsKey = null;
    refresh();
    fail(error);
  }
}

export function initAside(): void {
  el.toggleAside.onclick = () => toggleAside();

  const open = new Set<string>(["outline", "backlinks"]);
  try {
    const saved: unknown = JSON.parse(read(SECTIONS) ?? "null");
    if (Array.isArray(saved)) {
      open.clear();
      for (const section of saved) open.add(String(section));
    }
  } catch {
  }
  for (const section of ["outline", "backlinks", "mentions"] as const) {
    const box = details(section);
    box.open = open.has(section);
    box.addEventListener("toggle", () => {
      if (box.open) open.add(section);
      else open.delete(section);
      write(SECTIONS, JSON.stringify([...open]));
      refresh();
    });
  }

  editor.onText(edited);
  editor.scroller.addEventListener("scroll", onScroll, { passive: true });
  el.preview.addEventListener("scroll", onScroll, { passive: true });
  state.on("note", () => queueMicrotask(apply));
  state.on("mode", () => queueMicrotask(markCurrent));
  apply();
}
