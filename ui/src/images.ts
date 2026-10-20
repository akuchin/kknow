/**
 * Images from notes, in the preview and in live preview.
 *
 * The webview does not read the disk itself. So the core outputs
 * `<img data-src="path">` without `src`, and the window asks for the bytes with
 * a command. The core resolves the path: relative to the note, or by file name
 * across the vault. The bytes become a `data:` URL, not `blob:`: WebKit does
 * not show `blob:` images on a `tauri://` page, so the window would show a
 * broken image icon. URLs accumulate while one note is open and are dropped
 * when the note changes.
 *
 * Network images do not load on purpose: a note can come from someone else's
 * vault, and `![](https://…)` would tell a foreign server that it was opened.
 */

import { api } from "./api";
import { t } from "./i18n";

/** The same class the core sets (`IMAGE_CLASS`). */
export const IMAGE_CLASS = "note-image";

/** Who owns the images: a note in a vault or a separate file. */
type Owner = { file: false; vault: string; id: string } | { file: true; path: string };

let owner: Owner | null = null;
const cache = new Map<string, Promise<string | null>>();

const MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
  avif: "image/avif",
  bmp: "image/bmp",
  ico: "image/x-icon",
};

/** Another note is open: the URLs of the previous one are not needed. */
export function setImageOwner(next: Owner | null): void {
  if (JSON.stringify(next) === JSON.stringify(owner)) return;
  owner = next;
  cache.clear();
}

/** Decides by extension, the same way the core does. */
export function isImage(target: string): boolean {
  const ext = /\.([a-z0-9]+)(?:[?#].*)?$/i.exec(target.trim())?.[1]?.toLowerCase();
  return ext !== undefined && ext in MIME;
}

function isExternal(target: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(target);
}

/**
 * Command response as bytes. Usually it is an `ArrayBuffer`, but the Tauri IPC
 * fallback (`postMessage`) returns an array of numbers. `Blob` would join it
 * into the text "137,80,78,…", and the image would be broken.
 */
function toBytes(raw: ArrayBuffer | Uint8Array | number[]): Uint8Array<ArrayBuffer> {
  if (raw instanceof ArrayBuffer) return new Uint8Array(raw);
  return Uint8Array.from(raw);
}

function dataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/** A `data:` URL, or `null` if the image is missing. */
function imageUrl(target: string): Promise<string | null> {
  const known = cache.get(target);
  if (known) return known;

  const asked = owner;
  const loading = (async () => {
    if (!asked || isExternal(target)) return null;
    try {
      const bytes = asked.file
        ? await api.fileImage(asked.path, target)
        : await api.noteImage(asked.id, target);
      const ext = /\.([a-z0-9]+)(?:[?#].*)?$/i.exec(target)?.[1]?.toLowerCase() ?? "";
      return await dataUrl(new Blob([toBytes(bytes)], { type: MIME[ext] ?? "" }));
    } catch {
      return null;
    }
  })();
  cache.set(target, loading);
  return loading;
}

/**
 * A caption can carry a size: `diagram|300`, `diagram|300x200` or
 * just `300`. Parsed the same way as in the core.
 */
export function parseCaption(raw: string): { alt: string; width?: number; height?: number } {
  const size = (part: string) => /^\s*(\d+)(?:x(\d+))?\s*$/.exec(part);
  const bar = raw.lastIndexOf("|");
  const tail = bar >= 0 ? size(raw.slice(bar + 1)) : size(raw);
  if (!tail) return { alt: raw.trim() };
  return {
    alt: bar >= 0 ? raw.slice(0, bar).trim() : "",
    width: Number(tail[1]),
    height: tail[2] ? Number(tail[2]) : undefined,
  };
}

/** For the editor widget; the preview gets the element from the core. */
export function imageElement(target: string, caption: string): HTMLImageElement {
  const { alt, width, height } = parseCaption(caption);
  const img = document.createElement("img");
  img.className = IMAGE_CLASS;
  img.dataset.src = target;
  img.alt = alt;
  if (width) img.style.width = `${width}px`;
  if (height) img.style.height = `${height}px`;
  return img;
}

/**
 * Fills the images inside `root`. A missing image becomes a label with its
 * path: an empty frame explains nothing, the label shows what is missing.
 */
export function paintImages(root: ParentNode, loaded?: () => void): void {
  for (const img of root.querySelectorAll<HTMLImageElement>(`img.${IMAGE_CLASS}[data-src]`)) {
    if (img.src) continue;
    const target = img.dataset.src ?? "";
    img.title = t("editor.image.hint");
    img.onclick = () => openViewer(img);
    if (loaded) img.onload = loaded;
    const fail = (key: "image.external" | "image.missing" | "image.broken") => {
      const missing = document.createElement("span");
      missing.className = "note-image-missing";
      missing.textContent = t(key, { target });
      img.replaceWith(missing);
      loaded?.();
    };
    // The file exists, but the browser cannot decode it: a label is clearer than "?".
    img.onerror = () => fail("image.broken");
    void imageUrl(target).then((url) => {
      if (url) img.src = url;
      else fail(isExternal(target) ? "image.external" : "image.missing");
    });
  }
}

/**
 * Shows the image over the whole window on click. In the text it is small on
 * purpose, so it does not get in the way; small text in a screenshot is read here.
 */
function openViewer(img: HTMLImageElement): void {
  if (!img.src) return;
  const overlay = document.createElement("div");
  overlay.className = "image-viewer";
  overlay.role = "dialog";
  overlay.ariaLabel = img.alt || (img.dataset.src ?? "");

  const full = document.createElement("img");
  full.src = img.src;
  full.alt = img.alt;
  overlay.append(full);

  const close = () => {
    overlay.remove();
    document.removeEventListener("keydown", onKey, true);
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    close();
  };
  overlay.onclick = close;
  document.addEventListener("keydown", onKey, true);
  document.body.append(overlay);
}
