import { MockVault, cleanLabels, coded, session, vaultInfo, vaultView, vaults } from "./store";
import type { Handlers } from "./handlers";

export const vaultsHandlers = {
  current_vault: () => {
    return vaultInfo(session.activeVault);
  },

  reload_vault: () => {
    return vaultInfo(session.activeVault);
  },

  list_vaults: () => {
    return vaults.map(vaultView);
  },

  rename_vault: (args) => {
    const vault = vaults.find((entry) => entry.id === args.id);
    if (vault) vault.name = (args.name).trim() || vault.name;
    return vaults.map(vaultView);
  },

  set_vault_color: (args) => {
    const vault = vaults.find((entry) => entry.id === args.id);
    if (vault) vault.color = args.color;
    return vaults.map(vaultView);
  },

  set_vault_labels: (args) => {
    const vault = vaults.find((entry) => entry.id === args.id);
    if (vault) vault.labels = cleanLabels(args.labels);
    return vaults.map(vaultView);
  },

  vault_stats: (args) => {
    const vault = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const folders = new Set(
      vault.notes.flatMap((note) =>
        note.folder ? note.folder.split("/").map((_, index, all) => all.slice(0, index + 1).join("/")) : [],
      ),
    );
    return {
      notes: vault.notes.length,
      folders: folders.size,
      bytes: vault.notes.reduce((sum, note) => sum + note.content.length, 0),
    };
  },

  add_vault: (args) => {
    const path = args.path;
    const added: MockVault = {
      id: `vault-${vaults.length + 1}`,
      name: path.split("/").filter(Boolean).pop() ?? path,
      path,
      color: "green",
      labels: [],
      available: true,
      notes: [],
      folders: [],
      trash: [],
    };
    vaults.push(added);
    session.activeVault = added;
    return vaultInfo(added);
  },

  remove_vault: (args) => {
    const index = vaults.findIndex((vault) => vault.id === args.id);
    if (index >= 0) vaults.splice(index, 1);
    if (!vaults.includes(session.activeVault)) session.activeVault = vaults[0];
    return vaults.map(vaultView);
  },

  activate_vault: (args) => {
    const found = vaults.find((vault) => vault.id === args.id);
    if (!found) throw coded("unknownVault", `vault not found: ${String(args.id)}`, { value: String(args.id) });
    session.activeVault = found;
    return vaultInfo(found);
  },

  vault_by_hotkey: (args) => {
    const number = args.number;
    return (vaults[number - 1]?.id ?? null);
  },
} satisfies Handlers;
