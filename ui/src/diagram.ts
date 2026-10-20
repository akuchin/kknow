/**
 * Mermaid diagrams in the preview.
 *
 * The core outputs a ```` ```mermaid ```` block as `<pre class="mermaid">`
 * with the diagram text inside: it cannot draw it, because mermaid is
 * JavaScript. This module turns that text into SVG.
 *
 * The library is heavy, so it loads on demand: most notes have no diagrams,
 * and there is no reason to pay for it at startup.
 */

type Mermaid = typeof import("mermaid").default;
type Base = "light" | "dark";

/** Code block language that means a diagram; see `render::MERMAID_LANG`. */
export const DIAGRAM_LANG = "mermaid";

/** The class the core sets; see `render::MERMAID_CLASS`. */
export const DIAGRAM_CLASS = "mermaid";

const SELECTOR = `pre.${DIAGRAM_CLASS}`;

let loading: Promise<Mermaid> | undefined;
let base: Base = "light";
/** IDs must be unique: mermaid puts them into the SVG markup. */
let counter = 0;

async function load(): Promise<Mermaid> {
  if (!loading) {
    loading = import("mermaid").then(({ default: mermaid }) => {
      mermaid.initialize(config());
      return mermaid;
    });
  }
  return loading;
}

function config(): Parameters<Mermaid["initialize"]>[0] {
  const style = getComputedStyle(document.documentElement);
  return {
    startOnLoad: false,
    // The diagram comes from a note, possibly from someone else's vault: node
    // labels can contain markup, and it must not run.
    securityLevel: "strict",
    // Show the error in the block itself, not as a placeholder image at the end of body.
    suppressErrorRendering: true,
    theme: base === "dark" ? "dark" : "default",
    fontFamily: style.getPropertyValue("--font-text").trim() || undefined,
  };
}

/**
 * Draws all diagrams inside `root`.
 *
 * Drawing is async, and the preview can change meanwhile, so each write first
 * checks that the block is still in the document.
 */
export async function renderDiagrams(root: HTMLElement): Promise<void> {
  const blocks = [...root.querySelectorAll<HTMLElement>(SELECTOR)];
  if (blocks.length === 0) return;

  let mermaid: Mermaid;
  try {
    mermaid = await load();
  } catch (error) {
    for (const block of blocks) showError(block, source(block), error);
    return;
  }

  for (const block of blocks) {
    if (!block.isConnected || block.dataset.drawn === "true") continue;
    const text = source(block);

    try {
      const { svg } = await mermaid.render(`kknow-diagram-${(counter += 1)}`, text);
      if (!block.isConnected) return;
      block.innerHTML = svg;
      block.dataset.drawn = "true";
      delete block.dataset.error;
    } catch (error) {
      if (!block.isConnected) return;
      showError(block, text, error);
    }
  }
}

/**
 * Redraws diagrams for another theme: mermaid takes colors from its own config,
 * so a window theme change alone does not affect them.
 */
export async function setDiagramBase(next: Base, root: HTMLElement): Promise<void> {
  const changed = next !== base;
  base = next;
  if (!changed || !loading) return;

  const mermaid = await load();
  mermaid.initialize(config());

  for (const block of root.querySelectorAll<HTMLElement>(SELECTOR)) {
    block.textContent = source(block);
    delete block.dataset.drawn;
  }
  await renderDiagrams(root);
}

/** The first call reads the block text; later calls read `data-source`. */
function source(block: HTMLElement): string {
  block.dataset.source ??= block.textContent ?? "";
  return block.dataset.source;
}

/** An invalid diagram stays text, so the typo is easy to find. */
function showError(block: HTMLElement, text: string, error: unknown): void {
  block.textContent = text;
  block.dataset.error = message(error);
  delete block.dataset.drawn;
}

function message(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.split("\n")[0].slice(0, 200);
}
