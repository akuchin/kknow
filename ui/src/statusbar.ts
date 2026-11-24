import { icon, iconMarkup } from "./icons";
import { withShortcut } from "./actions";
import { t } from "./i18n";
import { SCALE_PERCENT } from "./note-scale";

const READING_SPEED = 200;

export type SyncTone = "off" | "ready" | "busy" | "conflict" | "error";

export type MessageTone = "idle" | "work" | "error";

export interface StatusBarActions {
  vault: (anchor: HTMLElement) => void;
  sync: () => void;
  run: () => void;
  chip: (anchor: HTMLElement) => void;
  update: () => void;
  scale: (steps: number) => void;
  lines: () => void;
  mcp: () => void;
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
  private readonly update: HTMLButtonElement;
  private readonly scale: HTMLDivElement;
  private readonly scaleSlider: HTMLInputElement;
  private readonly scaleSmaller: HTMLButtonElement;
  private readonly scaleLarger: HTMLButtonElement;
  private readonly scaleValue: HTMLButtonElement;
  private readonly lines: HTMLButtonElement;
  private readonly mcp: HTMLButtonElement;
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
      <button class="status-item" id="status-update" hidden></button>
      <button class="status-item" id="status-run" aria-expanded="false" hidden>
        <span class="status-run-icon"></span><span class="status-run-text"></span>
      </button>
      <button class="status-item chip" id="status-chip" aria-haspopup="menu" hidden></button>
      <span class="status-item static" id="status-words"></span>
      <div class="status-scale" id="status-scale" role="group" hidden>
        <button class="status-item" id="status-scale-smaller">−</button>
        <input type="range" id="status-scale-slider" min="${SCALE_PERCENT.min}"
               max="${SCALE_PERCENT.max}" step="${SCALE_PERCENT.step}" value="100"
               class="scale-slider" />
        <button class="status-item" id="status-scale-larger">+</button>
        <button class="status-item" id="status-scale-value">100%</button>
      </div>
      <button class="status-item" id="status-lines" aria-pressed="false">
        ${iconMarkup("lineNumbers")}
      </button>
      <button class="status-item" id="status-mcp" aria-expanded="false" data-tone="off">
        <span class="status-dot"></span><span>MCP</span>
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
    this.update = pick("#status-update");
    this.scale = pick("#status-scale");
    this.scaleSlider = pick("#status-scale-slider");
    this.scaleSmaller = pick("#status-scale-smaller");
    this.scaleLarger = pick("#status-scale-larger");
    this.scaleValue = pick("#status-scale-value");
    this.lines = pick("#status-lines");
    this.mcp = pick("#status-mcp");
    const more = pick<HTMLButtonElement>("#status-more");

    this.vault.onclick = () => actions.vault(this.vault);
    this.sync.onclick = () => actions.sync();
    this.run.onclick = () => actions.run();
    this.chip.onclick = () => actions.chip(this.chip);
    this.update.onclick = () => actions.update();
    this.scaleSmaller.onclick = () => actions.scale(-1);
    this.scaleLarger.onclick = () => actions.scale(1);
    this.scaleValue.onclick = () => actions.scale(0);
    this.lines.onclick = () => actions.lines();
    this.mcp.onclick = () => actions.mcp();
    this.scaleSlider.oninput = () => {
      const steps = Math.round(
        (Number(this.scaleSlider.value) - this.scalePercent) / SCALE_PERCENT.step,
      );
      if (steps !== 0) actions.scale(steps);
    };
    more.onclick = () => actions.more(more);

    more.title = t("statusbar.more");
    more.setAttribute("aria-label", t("statusbar.more"));
    this.scale.setAttribute("aria-label", t("statusbar.scale.title"));
    this.scaleSlider.title = withShortcut(t("statusbar.scale.title"), "scale.larger", "scale.smaller");
    this.scaleSlider.setAttribute("aria-label", t("statusbar.scale.title"));
    this.scaleSmaller.title = withShortcut(t("statusbar.scale.smaller"), "scale.smaller");
    this.scaleSmaller.setAttribute("aria-label", t("statusbar.scale.smaller"));
    this.scaleLarger.title = withShortcut(t("statusbar.scale.larger"), "scale.larger");
    this.scaleLarger.setAttribute("aria-label", t("statusbar.scale.larger"));
    this.scaleValue.title = withShortcut(t("statusbar.scale.reset"), "scale.reset");
    this.sync.setAttribute("aria-label", t("panel.sync.title"));
    this.setWords(0);
    this.setLineNumbers(false);
    this.setMcp(null);
  }

  setVault(name: string | null, color = "blue"): void {
    this.vaultName.textContent = name ?? t("statusbar.vault.empty");
    this.vaultLabel.dataset.color = color;
    this.vault.title = name
      ? t("statusbar.vault.title", { name })
      : t("statusbar.vault.empty.title");
  }

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

  setRun(name: string | null, tone: "ok" | "error" | "work", title: string): void {
    this.runIcon.replaceChildren(
      icon(tone === "ok" ? "check" : tone === "work" ? "play" : "close"),
    );
    this.runText.textContent = name ?? "";
    this.run.hidden = name === null;
    this.run.dataset.tone = tone;
    this.run.title = title;
  }

  setRunOpen(open: boolean): void {
    this.run.setAttribute("aria-expanded", String(open));
    this.run.dataset.open = String(open);
  }

  setChip(text: string | null, tone: "danger" | "warning" = "danger"): void {
    this.chip.textContent = text ?? "";
    this.chip.hidden = text === null;
    this.chip.dataset.tone = tone;
  }

  setUpdate(version: string | null): void {
    this.update.hidden = version === null;
    if (version === null) return;
    this.update.textContent = t("statusbar.update");
    this.update.title = t("statusbar.update.title", { version });
  }

  setMessage(text: string, tone: MessageTone = "idle"): void {
    this.message.textContent = text;
    this.message.dataset.tone = tone;
  }

  setScale(scale: number | null, normal = 1): void {
    this.scale.hidden = scale === null;
    if (scale === null) return;
    this.scalePercent = Math.round(scale * 100);
    this.scaleSlider.value = String(this.scalePercent);
    this.scaleValue.textContent = `${this.scalePercent}%`;
    this.scaleSmaller.disabled = this.scalePercent <= SCALE_PERCENT.min;
    this.scaleLarger.disabled = this.scalePercent >= SCALE_PERCENT.max;
    this.scaleValue.dataset.changed = String(this.scalePercent !== Math.round(normal * 100));
  }

  setLineNumbers(on: boolean): void {
    this.lines.setAttribute("aria-pressed", String(on));
    this.lines.dataset.on = String(on);
    const label = t(on ? "statusbar.lines.hide" : "statusbar.lines.show");
    this.lines.title = label;
    this.lines.setAttribute("aria-label", label);
  }

  setMcp(status: { enabled: boolean; running: boolean } | null): void {
    const tone = !status || !status.enabled ? "off" : status.running ? "ready" : "error";
    this.mcp.dataset.tone = tone;
    this.mcp.title = t(`statusbar.mcp.${tone}`);
  }

  setMcpOpen(open: boolean): void {
    this.mcp.setAttribute("aria-expanded", String(open));
    this.mcp.dataset.open = String(open);
  }
}

export function countWords(text: string): number {
  return (text.match(/[\p{L}\p{N}][\p{L}\p{N}'’\-]*/gu) ?? []).length;
}
