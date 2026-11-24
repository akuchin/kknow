import { invoke } from "@tauri-apps/api/core";

import type { Commands } from "./bindings";

export interface RawBodyOptions {
  headers: Record<string, string>;
}

type CallArguments<C extends keyof Commands> = Commands[C]["args"] extends Uint8Array
  ? [body: Uint8Array, options: RawBodyOptions]
  : keyof Commands[C]["args"] extends never
    ? []
    : Record<never, never> extends Commands[C]["args"]
      ? [args?: Commands[C]["args"]]
      : [args: Commands[C]["args"]];

export function call<C extends keyof Commands>(
  name: C,
  ...rest: CallArguments<C>
): Promise<Commands[C]["result"]> {
  const [args, options] = rest as [Record<string, unknown>?, RawBodyOptions?];
  return invoke(name, args, options);
}
