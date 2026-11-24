type Mermaid = typeof import("mermaid").default;
type Base = "light" | "dark";

export const DIAGRAM_LANG = "mermaid";

export const DIAGRAM_CLASS = "mermaid";

const SELECTOR = `pre.${DIAGRAM_CLASS}`;

let loading: Promise<Mermaid> | undefined;
let base: Base = "light";
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
    securityLevel: "strict",
    suppressErrorRendering: true,
    theme: base === "dark" ? "dark" : "default",
    fontFamily: style.getPropertyValue("--font-text").trim() || undefined,
  };
}

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

function source(block: HTMLElement): string {
  block.dataset.source ??= block.textContent ?? "";
  return block.dataset.source;
}

function showError(block: HTMLElement, text: string, error: unknown): void {
  block.textContent = text;
  block.dataset.error = message(error);
  delete block.dataset.drawn;
}

function message(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.split("\n")[0].slice(0, 200);
}
