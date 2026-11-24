import { api, type McpStatus, type VaultView } from "../api";
import { t } from "../i18n";
import { iconMarkup } from "../icons";
import { mcpSnippets, mcpUrl, type McpScope } from "../mcp-setup";
import { showRunOutput } from "./commands";
import { state } from "./state";
import { fail, status } from "./status";
import { copyText } from "./util";

export let mcpFloat: HTMLElement;

let server: McpStatus | null = null;

let mode: "vault" | "all" | "labels" = "vault";
let chosen: string[] = [];

let vaults: VaultView[] = [];

export function initMcp(): void {
  mcpFloat = document.createElement("section");
  mcpFloat.id = "mcp-float";
  mcpFloat.className = "run-float mcp-float";
  mcpFloat.setAttribute("role", "dialog");
  mcpFloat.setAttribute("aria-labelledby", "mcp-title");
  mcpFloat.hidden = true;
  document.body.append(mcpFloat);
  void refresh();
}

async function refresh(): Promise<void> {
  try {
    take(await api.mcpStatus());
  } catch (error) {
    console.error(error);
  }
}

function take(next: McpStatus): void {
  server = next;
  status.setMcp(next);
}

export function showMcpSetup(show: boolean): void {
  mcpFloat.hidden = !show;
  status.setMcpOpen(show);
  if (!show) return;
  showRunOutput(false);
  vaults = state.vaults;
  render();
  void Promise.all([refresh(), readVaults()]).then(render);
}

async function readVaults(): Promise<void> {
  try {
    vaults = await api.listVaults();
  } catch {
  }
}

async function configure(enabled: boolean, port: number): Promise<void> {
  try {
    take(await api.setMcp(enabled, port));
  } catch (error) {
    fail(error);
  }
  render();
}

function render(): void {
  mcpFloat.innerHTML = `
    <h2>
      <span id="mcp-title">${t("panel.mcp.title")}</span>
      <button class="ghost bare mcp-hide" title="${t("panel.run.hide")}"
              aria-label="${t("panel.run.hide")}">
        ${iconMarkup("close")}
      </button>
    </h2>
  `;
  mcpFloat.querySelector<HTMLButtonElement>(".mcp-hide")!.onclick = () => showMcpSetup(false);
  if (!server) return;
  mcpFloat.append(controls(server));

  const intro = document.createElement("p");
  intro.className = "muted";
  intro.textContent = t("panel.mcp.intro");
  mcpFloat.append(intro);

  const vault = state.vault;
  const ordered = [...vaults].sort((a, b) => Number(b.id === vault?.id) - Number(a.id === vault?.id));
  const labels = [...new Set(ordered.flatMap((entry) => entry.labels))];
  chosen = chosen.filter((label) => labels.includes(label));
  if (mode === "labels" && chosen.length === 0) mode = "vault";
  if (mode === "vault" && !vault) mode = "all";
  mcpFloat.append(scopes(labels, vault !== null));

  const scope: McpScope =
    mode === "labels"
      ? { labels: chosen }
      : mode === "vault" && vault
        ? { vault: vault.id }
        : { all: true };
  const snippets = mcpSnippets(server.port, server.token, scope);
  mcpFloat.append(
    snippet(t("panel.mcp.claudeCode"), snippets.claudeCode),
    snippet(t("panel.mcp.other"), snippets.json),
  );
  const tools = document.createElement("p");
  tools.className = "muted";
  tools.textContent = t("panel.mcp.tools");
  mcpFloat.append(tools);
}

function controls(current: McpStatus): HTMLElement {
  const block = document.createElement("div");
  block.className = "mcp-controls";
  block.innerHTML = `
    <button class="mcp-switch" role="switch"><span class="mcp-switch-knob"></span></button>
    <label class="mcp-port"><span></span><input type="number" min="1" max="65535" /></label>
    <button class="ghost mcp-apply"></button>
    <p class="mcp-state"></p>
  `;

  const toggle = block.querySelector<HTMLButtonElement>(".mcp-switch")!;
  toggle.setAttribute("aria-checked", String(current.enabled));
  toggle.setAttribute("aria-label", t("panel.mcp.switch"));
  toggle.title = t("panel.mcp.switch");

  block.querySelector(".mcp-port span")!.textContent = t("panel.mcp.port");
  const port = block.querySelector<HTMLInputElement>(".mcp-port input")!;
  port.value = String(current.port);
  const apply = block.querySelector<HTMLButtonElement>(".mcp-apply")!;
  apply.textContent = t("panel.mcp.apply");
  apply.disabled = true;

  const wanted = (): number | null => {
    const value = Number(port.value);
    return Number.isInteger(value) && value >= 1 && value <= 65535 ? value : null;
  };
  port.oninput = () => {
    const value = wanted();
    apply.disabled = value === null || value === current.port;
  };
  const save = (): void => {
    const value = wanted();
    if (value !== null && value !== current.port) void configure(current.enabled, value);
  };
  apply.onclick = save;
  port.onkeydown = (event) => {
    if (event.key === "Enter") save();
  };
  toggle.onclick = () => void configure(!current.enabled, wanted() ?? current.port);

  const line = block.querySelector<HTMLElement>(".mcp-state")!;
  if (!current.enabled) {
    line.textContent = t("panel.mcp.off");
    line.dataset.tone = "off";
  } else if (current.running) {
    line.textContent = t("panel.mcp.listening", { url: mcpUrl(current.port, { all: true }) });
    line.dataset.tone = "ready";
  } else {
    line.textContent = t("panel.mcp.failed", {
      port: current.port,
      detail: current.error ?? "",
    });
    line.dataset.tone = "error";
  }
  return block;
}

function scopes(labels: string[], hasVault: boolean): HTMLElement {
  const block = document.createElement("div");
  block.className = "mcp-scope";
  block.innerHTML = `<h3></h3><div class="mcp-chips" role="group"></div><p class="muted"></p>`;
  block.querySelector("h3")!.textContent = t("panel.mcp.scope");

  const chips = block.querySelector(".mcp-chips")!;
  const chip = (text: string, on: boolean, pick: () => void): void => {
    const button = document.createElement("button");
    button.className = "mcp-chip";
    button.setAttribute("role", "checkbox");
    button.setAttribute("aria-checked", String(on));
    button.textContent = text;
    button.onclick = () => {
      pick();
      render();
    };
    chips.append(button);
  };
  if (hasVault) {
    chip(t("panel.mcp.scope.vault"), mode === "vault", () => {
      mode = "vault";
      chosen = [];
    });
  }
  chip(t("panel.mcp.scope.all"), mode === "all", () => {
    mode = "all";
    chosen = [];
  });
  for (const label of labels) {
    chip(`#${label}`, mode === "labels" && chosen.includes(label), () => {
      chosen = chosen.includes(label)
        ? chosen.filter((other) => other !== label)
        : labels.filter((other) => other === label || chosen.includes(other));
      mode = chosen.length > 0 ? "labels" : "vault";
    });
  }

  const note = block.querySelector("p")!;
  if (mode === "labels") {
    const names = vaults
      .filter((entry) => entry.labels.some((label) => chosen.includes(label)))
      .map((entry) => entry.name);
    note.textContent = t("panel.mcp.scope.labels", {
      count: chosen.length,
      labels: chosen.map((label) => `#${label}`).join(", "),
      names: names.join(", "),
    });
  } else if (labels.length === 0) {
    note.textContent = t("panel.mcp.noLabels");
  } else {
    note.hidden = true;
  }
  return block;
}

function snippet(title: string, text: string): HTMLElement {
  const block = document.createElement("div");
  block.className = "mcp-snippet";
  block.innerHTML = `
    <h3></h3>
    <div class="mcp-code">
      <pre class="run-output"></pre>
      <button class="ghost mcp-copy">${t("panel.mcp.copy")}</button>
    </div>
  `;
  block.querySelector("h3")!.textContent = title;
  block.querySelector("pre")!.textContent = text;
  block.querySelector<HTMLButtonElement>(".mcp-copy")!.onclick = () => void copyText(text);
  return block;
}
