import { HighlightStyle, LanguageDescription, type Language } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightCode, tags } from "@lezer/highlight";

export const highlightStyle = HighlightStyle.define([
  { tag: tags.heading, color: "var(--heading)" },
  { tag: tags.heading1, color: "var(--heading)", fontSize: "1.5em", fontWeight: "600" },
  { tag: tags.heading2, color: "var(--heading)", fontSize: "1.25em", fontWeight: "600" },
  { tag: tags.heading3, color: "var(--heading)", fontSize: "1.1em", fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.monospace, fontFamily: "var(--font-mono)" },
  { tag: tags.quote, color: "var(--text-muted)" },
  { tag: [tags.url, tags.labelName, tags.contentSeparator, tags.meta], color: "var(--text-muted)" },
  { tag: tags.keyword, color: "var(--code-keyword)" },
  {
    tag: [tags.string, tags.special(tags.string), tags.regexp, tags.escape],
    color: "var(--code-string)",
  },
  { tag: tags.comment, color: "var(--code-comment)" },
  { tag: [tags.number, tags.bool, tags.atom, tags.null], color: "var(--code-number)" },
  {
    tag: [tags.typeName, tags.className, tags.namespace, tags.macroName],
    color: "var(--code-type)",
  },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
    color: "var(--code-function)",
  },
  { tag: tags.invalid, color: "var(--danger)" },
]);

export const codeLanguages = languages;

const loaded = new Map<string, Language | null>();
const loading = new Map<string, Promise<Language | null>>();

function describe(name: string): LanguageDescription | null {
  return name ? LanguageDescription.matchLanguageName(languages, name, true) : null;
}

function load(name: string): Promise<Language | null> {
  const pending = loading.get(name);
  if (pending) return pending;

  const description = describe(name);
  const promise = description
    ? description.load().then(
        (support) => support.language,
        () => null,
      )
    : Promise.resolve<Language | null>(null);
  loading.set(name, promise);
  void promise.then((language) => loaded.set(name, language));
  return promise;
}

export function paintCode(el: HTMLElement, source: string, name: string): void {
  const key = name.toLowerCase();
  const language = loaded.get(key);

  if (language === undefined) {
    el.textContent = source;
    if (!describe(key)) {
      loaded.set(key, null);
      return;
    }
    void load(key).then(() => {
      if (el.isConnected) paintCode(el, source, key);
    });
    return;
  }
  if (!language) {
    el.textContent = source;
    return;
  }

  const fragment = document.createDocumentFragment();
  highlightCode(
    source,
    language.parser.parse(source),
    highlightStyle,
    (text, classes) => {
      if (!classes) {
        fragment.append(text);
        return;
      }
      const span = document.createElement("span");
      span.className = classes;
      span.textContent = text;
      fragment.append(span);
    },
    () => fragment.append("\n"),
  );
  el.replaceChildren(fragment);
}

export function paintCodeBlocks(root: HTMLElement): void {
  for (const code of root.querySelectorAll<HTMLElement>('pre > code[class*="language-"]')) {
    const name = [...code.classList].find((c) => c.startsWith("language-"))?.slice(9) ?? "";
    paintCode(code, code.textContent ?? "", name);
  }
}
