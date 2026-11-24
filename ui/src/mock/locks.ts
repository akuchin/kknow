import type { NoteView } from "../bindings";
import { checkPassword, coded, lockOf, unlockMock, vaultOf } from "./store";
import { view } from "./views";
import type { Handlers } from "./handlers";

function setNoteLock(args: { id: string; vault?: string | null }, locked: boolean): NoteView {
  const vault = vaultOf(args);
  const lock = lockOf(vault);
  if (!lock.password) throw coded("noLockPassword", "the vault has no password for locked notes");
  if (!lock.unlocked) throw coded("notesClosed", "locked notes are closed: enter the vault password");
  const note = vault.notes.find((each) => each.id === args.id);
  if (!note) throw coded("notFound", `note not found: ${args.id}`, { value: args.id });
  note.locked = locked;
  if (locked) note.bookmarks = [];
  return view(note);
}

export const locksHandlers = {
  lock_state: (args) => {
    const lock = lockOf(vaultOf(args));
    return { password: lock.password !== null, unlocked: lock.unlocked };
  },

  set_lock_password: (args) => {
    const lock = lockOf(vaultOf(args));
    if (lock.password) throw coded("lockPasswordExists", "the vault already has a password for locked notes");
    checkPassword(args.password);
    lock.password = args.password;
    lock.unlocked = true;
    return lock.code;
  },

  unlock_notes: (args) => {
    return unlockMock(vaultOf(args), args.secret);
  },

  change_lock_password: (args) => {
    const lock = lockOf(vaultOf(args));
    if (!lock.unlocked) throw coded("notesClosed", "locked notes are closed: enter the vault password");
    checkPassword(args.password);
    lock.password = args.password;
    return;
  },

  lock_all: (args) => {
    lockOf(vaultOf(args)).unlocked = false;
    return;
  },

  lock_note: (args) => setNoteLock(args, true),

  remove_lock: (args) => setNoteLock(args, false),
} satisfies Handlers;
