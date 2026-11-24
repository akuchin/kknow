import type {
  TrashEntryView,
} from "../api";
import { coded, session, vaults } from "./store";
import type { Handlers } from "./handlers";

export const trashHandlers = {
  list_trash: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    return asked.trash
      .map<TrashEntryView>((entry) => ({
        id: entry.id,
        path: entry.path,
        deleted: entry.deleted,
        notes: entry.notes.map((note) => `${note.id}.md`),
      }))
      .reverse();
  },

  restore_trash: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const index = asked.trash.findIndex((entry) => entry.id === args.entry);
    if (index < 0) throw coded("trashEntryNotFound", `trash entry not found: ${String(args.entry)}`, { value: String(args.entry) });
    const entry = asked.trash[index];
    const taken = entry.notes.find((note) => asked.notes.some((other) => other.id === note.id));
    if (taken) throw coded("exists", `already exists: ${taken.id}.md`, { value: `${taken.id}.md` });
    asked.trash.splice(index, 1);
    asked.notes.push(...entry.notes);
    asked.folders.push(...entry.folders.filter((folder) => !asked.folders.includes(folder)));
    return entry.notes.map((note) => note.id);
  },

  trash_note: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const entry = asked.trash.find((each) => each.id === args.entry);
    const note = entry?.notes.find((each) => `${each.id}.md` === args.note);
    if (!note) throw coded("trashEntryNotFound", `trash entry not found: ${String(args.note)}`, { value: String(args.note) });
    return note.html;
  },

  delete_trash_entry: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const index = asked.trash.findIndex((entry) => entry.id === args.entry);
    if (index < 0) throw coded("trashEntryNotFound", `trash entry not found: ${String(args.entry)}`, { value: String(args.entry) });
    asked.trash.splice(index, 1);
    return;
  },

  empty_trash: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const count = asked.trash.length;
    asked.trash = [];
    return count;
  },
} satisfies Handlers;
