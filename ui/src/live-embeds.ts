import { StateEffect, StateField, type EditorState, type Range } from "@codemirror/state";
import { Decoration, EditorView, WidgetType } from "@codemirror/view";

import type { LinkPreview } from "./api";
import { t } from "./i18n";
import { icon } from "./icons";

export interface EmbedSource {
  load(target: string): Promise<LinkPreview | null>;
  paint(root: HTMLElement, done: () => void): void;
  open(target: string): void;
}

let source: EmbedSource | null = null;
const cache = new Map<string, Promise<LinkPreview | null>>();

export function setEmbedSource(next: EmbedSource): void {
  source = next;
}

export function clearEmbeds(): void {
  cache.clear();
}

export const embedsChanged = StateEffect.define<void>();

export const embedGeneration = StateField.define<number>({
  create: () => 0,
  update: (value, tr) =>
    tr.effects.some((effect) => effect.is(embedsChanged)) ? value + 1 : value,
});

function load(target: string): Promise<LinkPreview | null> {
  let loading = cache.get(target);
  if (!loading) {
    loading = source ? source.load(target).catch(() => null) : Promise.resolve(null);
    cache.set(target, loading);
  }
  return loading;
}

class EmbedWidget extends WidgetType {
  constructor(
    private readonly target: string,
    private readonly generation: number,
  ) {
    super();
  }

  eq(other: EmbedWidget): boolean {
    return other.target === this.target && other.generation === this.generation;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "embed cm-embed is-loading";
    box.dataset.target = this.target;

    const head = document.createElement("div");
    head.className = "embed-head";
    head.textContent = this.target;
    const body = document.createElement("div");
    body.className = "embed-body preview";

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "cm-diagram-edit";
    edit.title = t("editor.diagram.source");
    edit.ariaLabel = edit.title;
    edit.append(icon("code"));
    edit.onclick = (event) => {
      event.stopPropagation();
      view.dispatch({ selection: { anchor: view.posAtDOM(box) } });
      view.focus();
    };
    box.append(head, body, edit);

    box.onclick = (event) => {
      if (!document.getSelection()?.isCollapsed) return;
      if ((event.target as HTMLElement).closest("a, button")) event.preventDefault();
      source?.open(this.target);
    };

    void load(this.target).then((preview) => {
      box.classList.remove("is-loading");
      if (!preview) {
        box.classList.add("embed-stopped");
        body.textContent = t("hover.missing", { name: this.target.split("#")[0] });
      } else {
        head.textContent = preview.partMissing
          ? `${this.target} · ${t("hover.partMissing")}`
          : this.target;
        body.innerHTML = preview.html;
        source?.paint(body, () => view.requestMeasure());
      }
      view.requestMeasure();
    });
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

const ALONE = /^!\[\[([^\]\n]+)\]\]\s*$/;

export function addEmbed(
  state: EditorState,
  from: number,
  to: number,
  onLine: boolean,
  marks: Range<Decoration>[],
): boolean {
  if (!source || onLine) return false;
  const line = state.doc.lineAt(from);
  if (line.from !== from || !ALONE.test(line.text) || to > line.to) return false;
  const target = ALONE.exec(line.text)?.[1].split("|")[0].trim();
  if (!target || target.startsWith("#")) return false;

  const widget = new EmbedWidget(target, state.field(embedGeneration));
  marks.push(Decoration.replace({ widget, block: true }).range(line.from, line.to));
  return true;
}
