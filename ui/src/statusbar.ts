/**
 * Status bar along the bottom edge of the window.
 *
 * The header shows what is open; the status bar shows state. The left side is
 * about the vault: which one is active, when it last synced (sync sits next to
 * the name because it belongs to the vault, not the window), and the message.
 * The right side is about the note and the window: the last command result,
 * problems in other vaults, words, the font size of the note, the line
 * numbers toggle, and one button for theme, language and settings.
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
import { SCALE_PERCENT } from "./note-scale";

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
  /** Note font size: steps up or down; 0 resets to normal. */
  scale: (steps: number) => void;
  /** Line numbers in the editor gutter: on or off. */
  lines: () => void;
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
  private readonly scale: HTMLDivElement;
  private readonly scaleSlider: HTMLInputElement;
  private readonly scaleSmaller: HTMLButtonElement;
  private readonly scaleLarger: HTMLButtonElement;
  private readonly scaleValue: HTMLButtonElement;
  private readonly lines: HTMLButtonElement;
  /** Current scale in percents: the slider moves by steps from it. */
  private scalePercent = 100;

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
      <div class="status-scale" id="status-scale" role="group" hidden>
        <button class="status-item" id="status-scale-smaller">−</button>
        <input type="range" id="status-scale-slider" min="${SCALE_PERCENT.min}"
               max="${SCALE_PERCENT.max}" step="${SCALE_PERCENT.step}" value="100" />
        <button class="status-item" id="status-scale-larger">+</button>
        <button class="status-item" id="status-scale-value">100%</button>
      </div>
      <button class="status-item" id="status-lines" aria-pressed="false">
        ${iconMarkup("lineNumbers")}
      </button>
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
    this.scaleSlider = pick("#status-scale-slider");
    this.scaleSmaller = pick("#status-scale-smaller");
    this.scaleLarger = pick("#status-scale-larger");
    this.scaleValue = pick("#status-scale-value");
    this.lines = pick("#status-lines");
    const more = pick<HTMLButtonElement>("#status-more");

    this.vault.onclick = () => actions.vault(this.vault);
    this.sync.onclick = () => actions.sync();
    this.run.onclick = () => actions.run();
    this.chip.onclick = () => actions.chip(this.chip);
    this.scaleSmaller.onclick = () => actions.scale(-1);
    this.scaleLarger.onclick = () => actions.scale(1);
    this.scaleValue.onclick = () => actions.scale(0);
    this.lines.onclick = () => actions.lines();
    // The slider sends the difference in steps: 0 would mean "reset".
    this.scaleSlider.oninput = () => {
      const steps = Math.round(
        (Number(this.scaleSlider.value) - this.scalePercent) / SCALE_PERCENT.step,
      );
      if (steps !== 0) actions.scale(steps);
    };
    more.onclick = () => actions.more(more);

    // The gear has no visible label, so `aria-label` tells screen readers what
    // it opens. The sync label shows a value, not a purpose.
    more.title = t("statusbar.more");
    more.setAttribute("aria-label", t("statusbar.more"));
    this.scale.setAttribute("aria-label", t("statusbar.scale.title"));
    this.scaleSlider.title = t("statusbar.scale.title");
    this.scaleSlider.setAttribute("aria-label", t("statusbar.scale.title"));
    this.scaleSmaller.title = t("statusbar.scale.smaller");
    this.scaleSmaller.setAttribute("aria-label", t("statusbar.scale.smaller"));
    this.scaleLarger.title = t("statusbar.scale.larger");
    this.scaleLarger.setAttribute("aria-label", t("statusbar.scale.larger"));
    this.scaleValue.title = t("statusbar.scale.reset");
    this.sync.setAttribute("aria-label", t("panel.sync.title"));
    this.setWords(0);
    this.setLineNumbers(false);
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
   * Font size of the open note: − / slider / + / value. The value resets to
   * 100%. `null` hides the control: nothing is open, so there is nothing to
   * scale.
   *
   * The control is always visible with a note open, not only away from 100%.
   * Before, the size was in the note menu and the status bar showed only a
   * changed value. A control that appears only after the first change cannot
   * be found.
   */
  setScale(scale: number | null): void {
    this.scale.hidden = scale === null;
    if (scale === null) return;
    this.scalePercent = Math.round(scale * 100);
    this.scaleSlider.value = String(this.scalePercent);
    this.scaleValue.textContent = `${this.scalePercent}%`;
    this.scaleSmaller.disabled = this.scalePercent <= SCALE_PERCENT.min;
    this.scaleLarger.disabled = this.scalePercent >= SCALE_PERCENT.max;
    this.scaleValue.dataset.changed = String(this.scalePercent !== 100);
  }

  /**
   * Line numbers toggle. It is the same setting as in the settings window, so
   * the window calls this from the settings, not from the click.
   */
  setLineNumbers(on: boolean): void {
    this.lines.setAttribute("aria-pressed", String(on));
    this.lines.dataset.on = String(on);
    const label = t(on ? "statusbar.lines.hide" : "statusbar.lines.show");
    this.lines.title = label;
    this.lines.setAttribute("aria-label", label);
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
