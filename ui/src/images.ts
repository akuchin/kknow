import { api } from "./api";
import { t } from "./i18n";

export const IMAGE_CLASS = "note-image";

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

export function setImageOwner(next: Owner | null): void {
  if (JSON.stringify(next) === JSON.stringify(owner)) return;
  owner = next;
  cache.clear();
}

export function isImage(target: string): boolean {
  const ext = /\.([a-z0-9]+)(?:[?#].*)?$/i.exec(target.trim())?.[1]?.toLowerCase();
  return ext !== undefined && ext in MIME;
}

function isExternal(target: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(target);
}

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

function imageUrl(target: string, note?: string): Promise<string | null> {
  const key = note ? `${note}\n${target}` : target;
  const known = cache.get(key);
  if (known) return known;

  const asked = owner;
  const loading = (async () => {
    if (!asked || isExternal(target)) return null;
    try {
      const bytes =
        note && !asked.file
          ? await api.noteImage(note, target)
          : asked.file
            ? await api.fileImage(asked.path, target)
            : await api.noteImage(asked.id, target);
      const ext = /\.([a-z0-9]+)(?:[?#].*)?$/i.exec(target)?.[1]?.toLowerCase() ?? "";
      return await dataUrl(new Blob([toBytes(bytes)], { type: MIME[ext] ?? "" }));
    } catch {
      return null;
    }
  })();
  cache.set(key, loading);
  return loading;
}

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
    img.onerror = () => fail("image.broken");
    void imageUrl(target, img.dataset.note).then((url) => {
      if (url) img.src = url;
      else fail(isExternal(target) ? "image.external" : "image.missing");
    });
  }
}

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
