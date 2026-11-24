import type { LinkPreview } from "./api";
import { t } from "./i18n";

const SHOW_DELAY = 300;
const HIDE_DELAY = 250;

const GAP = 12;

export interface HoverHost {
  load(target: string): Promise<LinkPreview | null>;
  paint(root: HTMLElement): void;
  open(preview: LinkPreview): void;
  follow(anchor: HTMLAnchorElement): void;
}

export class HoverPreview {
  private readonly host: HoverHost;
  private readonly popup: HTMLElement;
  private readonly cache = new Map<string, Promise<LinkPreview | null>>();
  private target: string | null = null;
  private showTimer: number | undefined;
  private hideTimer: number | undefined;
  private inside = false;

  constructor(host: HoverHost) {
    this.host = host;
    this.popup = document.createElement("section");
    this.popup.className = "hover-preview";
    this.popup.role = "dialog";
    this.popup.hidden = true;
    this.popup.addEventListener("mouseenter", () => {
      this.inside = true;
      window.clearTimeout(this.hideTimer);
    });
    this.popup.addEventListener("mouseleave", () => {
      this.inside = false;
      this.scheduleHide();
    });
    this.popup.addEventListener("click", (event) => {
      const anchor = (event.target as HTMLElement).closest("a");
      if (!anchor) return;
      event.preventDefault();
      this.hide();
      this.host.follow(anchor);
    });
    document.body.append(this.popup);

    document.addEventListener(
      "keydown",
      (event) => {
        if (event.key !== "Escape" || this.popup.hidden) return;
        event.stopPropagation();
        this.hide();
      },
      true,
    );
    document.addEventListener("mousedown", (event) => {
      if (!this.popup.contains(event.target as Node)) this.hide();
    });
  }

  get visible(): boolean {
    return !this.popup.hidden;
  }

  point(target: string | null, x: number, y: number): void {
    if (target === null) {
      window.clearTimeout(this.showTimer);
      if (!this.popup.hidden) this.scheduleHide();
      else this.target = null;
      return;
    }
    window.clearTimeout(this.hideTimer);
    if (target === this.target) return;
    this.target = target;
    window.clearTimeout(this.showTimer);
    this.showTimer = window.setTimeout(() => void this.show(target, x, y), SHOW_DELAY);
  }

  hide(): void {
    window.clearTimeout(this.showTimer);
    window.clearTimeout(this.hideTimer);
    this.target = null;
    this.inside = false;
    this.popup.hidden = true;
    this.popup.replaceChildren();
  }

  clear(): void {
    this.cache.clear();
  }

  private scheduleHide(): void {
    window.clearTimeout(this.hideTimer);
    this.hideTimer = window.setTimeout(() => {
      if (!this.inside) this.hide();
    }, HIDE_DELAY);
  }

  private async show(target: string, x: number, y: number): Promise<void> {
    let loading = this.cache.get(target);
    if (!loading) {
      loading = this.host.load(target).catch(() => null);
      this.cache.set(target, loading);
    }
    const preview = await loading;
    if (this.target !== target) return;
    this.render(target, preview);
    this.place(x, y);
  }

  private render(target: string, preview: LinkPreview | null): void {
    const head = document.createElement("header");
    head.className = "hover-preview-head";
    const body = document.createElement("article");
    body.className = "preview hover-preview-body";

    if (!preview) {
      const missing = document.createElement("p");
      missing.className = "muted";
      missing.textContent = t("hover.missing", { name: target.split("#")[0] });
      head.append(missing);
      this.popup.replaceChildren(head);
      this.popup.hidden = false;
      return;
    }

    const title = document.createElement("button");
    title.className = "link";
    title.textContent = preview.title;
    title.title = t("hover.open");
    title.onclick = () => {
      this.hide();
      this.host.open(preview);
    };
    head.append(title);
    if (preview.locked) {
      const note = document.createElement("p");
      note.className = "muted hover-locked";
      note.textContent = t("hover.locked");
      this.popup.replaceChildren(head, note);
      this.popup.hidden = false;
      return;
    }
    if (preview.partMissing) {
      const note = document.createElement("span");
      note.className = "muted";
      note.textContent = t("hover.partMissing");
      head.append(note);
    }
    body.innerHTML = preview.html;
    this.popup.replaceChildren(head, body);
    this.popup.hidden = false;
    this.host.paint(body);
  }

  private place(x: number, y: number): void {
    const box = this.popup.getBoundingClientRect();
    const left = x + GAP + box.width > innerWidth ? Math.max(GAP, x - GAP - box.width) : x + GAP;
    const top = y + GAP + box.height > innerHeight ? Math.max(GAP, y - GAP - box.height) : y + GAP;
    this.popup.style.left = `${left}px`;
    this.popup.style.top = `${top}px`;
  }
}
