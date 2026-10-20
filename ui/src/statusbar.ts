/**
 * Status bar along the bottom edge of the window.
 *
 * The header shows what is open; the status bar shows state. The left side is
 * about the vault: which one is active, when it last synced (sync sits next to
 * the name because it belongs to the vault, not the window), and the message.
 * The right side is about the note and the window: the last command result,
 * problems in other vaults, words, scale, and one button for theme, language
 * and settings.
 *
 * The full file path is gone: the header shows the folder, and "Reveal in
 * Finder" and "Copy path" are in the note menu. The path was the longest
 * segment and was usually truncated until it was useless.
 *
 * One rule: a segment shows something and can act on click, but it never
 * opens a modal. Errors also go here as a message, not as a dialog.
 */

import { icon, iconMarkup } from "./icons";
import { t } from "./i18n";

/** Words per minute for the reading time estimate. */
const READING_SPEED = 200;

/** Sync state of the active vault. */
export type SyncTone = "off" | "ready" | "busy" | "conflict" | "error";

/** Message tone. It is the same as the old header status tone. */
export type MessageTone = "idle" | "work" | "error";

/** Click handlers for the segments. A handler gets its button because a menu
 *  needs an anchor to attach to. */
export interface StatusBarActions {
  vault: (anchor: HTMLElement) => void;
  sync: () => void;
  /** Last command result: show or hide its output. */
  run: () => void;
  /** Chip: a conflict or a failure in another vault. */
  chip: (anchor: HTMLElement) => void;
  /** Note scale: reset to normal. */
  scale: () => void;
  /** Theme, language and settings in one menu. */
  more: (anchor: HTMLElement) => void;
}

export class StatusBar {
  private readonly vaultName: HTMLSpanElement;
  private readonly vaultLabel: HTMLSpanElement;
  private readonly vault: HTMLButtonElement;
  private readonly message: HTMLSpanElement;
  private readonly words: HTMLSpanElement;
  private readonly sync: HTMLButtonElement;
  private readonly syncText: HTMLSpanElement;
  private readonly run: HTMLButtonElement;
  private readonly runIcon: HTMLSpanElement;
  private readonly runText: HTMLSpanElement;
  private readonly chip: HTMLButtonElement;
  private readonly scale: HTMLButtonElement;

  constructor(root: HTMLElement, actions: StatusBarActions) {
    root.innerHTML = `
      <button class="status-item" id="status-vault" aria-haspopup="menu">
        <span class="vault-label" data-color="blue"></span>
        <span class="status-vault-name"></span>
        ${iconMarkup("caret")}
      </button>
      <button class="status-item" id="status-sync">
        <span class="status-dot"></span>
        <span class="status-sync-text"></span>
      </button>
      <span class="status-message" id="status-message" role="status" aria-live="polite"></span>
      <span class="spacer"></span>
      <button class="status-item" id="status-run" aria-expanded="false" hidden>
        <span class="status-run-icon"></span><span class="status-run-text"></span>
      </button>
      <button class="status-item chip" id="status-chip" aria-haspopup="menu" hidden></button>
      <span class="status-item static" id="status-words"></span>
      <button class="status-item" id="status-scale" hidden></button>
      <button class="status-item" id="status-more" aria-haspopup="menu">
        ${iconMarkup("gear")}
      </button>
    `;

    const pick = <T extends HTMLElement>(selector: string): T =>
      root.querySelector<T>(selector)!;

    this.vault = pick("#status-vault");
    this.vaultLabel = pick(".vault-label");
    this.vaultName = pick(".status-vault-name");
    this.message = pick("#status-message");
    this.words = pick("#status-words");
    this.sync = pick("#status-sync");
    this.syncText = pick(".status-sync-text");
    this.run = pick("#status-run");
    this.runIcon = pick(".status-run-icon");
    this.runText = pick(".status-run-text");
    this.chip = pick("#status-chip");
    this.scale = pick("#status-scale");
    const more = pick<HTMLButtonElement>("#status-more");

    this.vault.onclick = () => actions.vault(this.vault);
    this.sync.onclick = () => actions.sync();
    this.run.onclick = () => actions.run();
    this.chip.onclick = () => actions.chip(this.chip);
    this.scale.onclick = () => actions.scale();
    more.onclick = () => actions.more(more);

    // The gear has no visible label, so `aria-label` tells screen readers what
    // it opens. The sync label shows a value, not a purpose.
    more.title = t("statusbar.more");
    more.setAttribute("aria-label", t("statusbar.more"));
    this.scale.title = t("statusbar.scale.title");
    this.sync.setAttribute("aria-label", t("panel.sync.title"));
    this.setWords(0);
  }

  setVault(name: string | null, color = "blue"): void {
    this.vaultName.textContent = name ?? t("statusbar.vault.empty");
    this.vaultLabel.dataset.color = color;
    this.vault.title = name
      ? t("statusbar.vault.title", { name })
      : t("statusbar.vault.empty.title");
  }

  /**
   * Word count and reading time.
   *
   * The window counts from the editor text, not the core: the number changes
   * while the user types, and a Rust call on each key press costs too much.
   */
  setWords(words: number): void {
    const minutes = words === 0 ? 0 : Math.max(1, Math.round(words / READING_SPEED));
    this.words.textContent =
      words === 0
        ? ""
        : `${t("statusbar.words", { count: words })} · ${t("statusbar.minutes", {
            count: minutes,
          })}`;
    this.words.title = words === 0 ? "" : t("statusbar.words.title");
  }

  setSync(text: string, tone: SyncTone, title: string): void {
    this.syncText.textContent = text;
    this.sync.dataset.tone = tone;
    this.sync.title = title;
  }

  /**
   * Last command result.
   *
   * A segment, not a panel: usually it is enough to know that the command
   * passed. A click opens the output. `null` hides the segment (no commands yet).
   */
  setRun(name: string | null, tone: "ok" | "error", title: string): void {
    this.runIcon.replaceChildren(icon(tone === "ok" ? "check" : "close"));
    this.runText.textContent = name ?? "";
    this.run.hidden = name === null;
    this.run.dataset.tone = tone;
    this.run.title = title;
  }

  /** The segment is a toggle, so it must show whether the output is open. */
  setRunOpen(open: boolean): void {
    this.run.setAttribute("aria-expanded", String(open));
    this.run.dataset.open = String(open);
  }

  /**
   * Chip about another vault: a conflict or a sync failure.
   *
   * No modal here: the problem does not block work in the active vault, but
   * the user must know about it.
   */
  setChip(text: string | null, tone: "danger" | "warning" = "danger"): void {
    this.chip.textContent = text ?? "";
    this.chip.hidden = text === null;
    this.chip.dataset.tone = tone;
  }

  /** Message: save, note creation, core error. */
  setMessage(text: string, tone: MessageTone = "idle"): void {
    this.message.textContent = text;
    this.message.dataset.tone = tone;
  }

  /**
   * Scale of the open note. Visible only when it is not 100%: otherwise a
   * larger font looks the same as a large theme.
   */
  setScale(scale: number): void {
    this.scale.hidden = scale === 1;
    this.scale.textContent = `${Math.round(scale * 100)}%`;
  }
}

/**
 * Number of words in the text.
 *
 * A word is a run of letters and digits. Heading hashes, asterisks and link
 * brackets do not count: we count what a person reads, not the markup.
 */
export function countWords(text: string): number {
  return (text.match(/[\p{L}\p{N}][\p{L}\p{N}'’\-]*/gu) ?? []).length;
}
