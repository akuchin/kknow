/**
 * Syntax highlighting: markdown in the editor and code inside blocks.
 *
 * One style serves the whole window: CodeMirror uses it through
 * `syntaxHighlighting`, and so do the code block widget and `<pre><code>` in
 * the preview. The style has its own classes, and CodeMirror mounts the style
 * sheet into the whole document, not only the editor, so the same classes work
 * outside it too. Colors are theme variables, so a theme change recolors code.
 *
 * Languages come from `@codemirror/language-data`: a grammar loads the first
 * time its language appears, so we do not pay for all of them at once.
 */

import { HighlightStyle, LanguageDescription, type Language } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightCode, tags } from "@lezer/highlight";

export const highlightStyle = HighlightStyle.define([
  // Highlighting takes one rule per tag, the most specific one, so the color
  // repeats on heading1–3; the general `heading` applies to levels 4–6.
  { tag: tags.heading, color: "var(--heading)" },
  { tag: tags.heading1, color: "var(--heading)", fontSize: "1.5em", fontWeight: "600" },
  { tag: tags.heading2, color: "var(--heading)", fontSize: "1.25em", fontWeight: "600" },
  { tag: tags.heading3, color: "var(--heading)", fontSize: "1.1em", fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.monospace, fontFamily: "var(--font-mono)" },
  { tag: tags.quote, color: "var(--text-muted)" },
  // Markup details: URLs, fence language, rules, attributes in code.
  { tag: [tags.url, tags.labelName, tags.contentSeparator, tags.meta], color: "var(--text-muted)" },
  // Code inside blocks.
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

/** Code block languages for the markdown parser; it loads grammars from them itself. */
export const codeLanguages = languages;

/** `null`: the language is unknown or its grammar failed to load. */
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

/**
 * Puts the block source into `el`, highlighted as language `name`.
 *
 * If the language is not loaded yet, the source is plain text first. It gets
 * colors when the grammar arrives, if the element is still in the document.
 * An unknown language stays plain text.
 */
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

/** Highlights code blocks in preview HTML: the core sets `class="language-…"`. */
export function paintCodeBlocks(root: HTMLElement): void {
  for (const code of root.querySelectorAll<HTMLElement>('pre > code[class*="language-"]')) {
    const name = [...code.classList].find((c) => c.startsWith("language-"))?.slice(9) ?? "";
    paintCode(code, code.textContent ?? "", name);
  }
}
