import { call } from "./command";

export const accepted = [
  call("load_note", { id: "Index" }),
  call("load_note", { id: "Index", vault: null }),
  call("list_vaults"),
  call("reload_vault"),
];

// @ts-expect-error unknown command name
export const unknownCommand = call("load_notes", { id: "Index" });

// @ts-expect-error missing required argument
export const missingArgument = call("load_note", {});

// @ts-expect-error argument of the wrong type
export const wrongType = call("load_note", { id: 1 });

// @ts-expect-error unknown argument key
export const unknownKey = call("load_note", { id: "Index", note: "Index" });
