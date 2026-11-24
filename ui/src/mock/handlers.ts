import type { Commands } from "../bindings";

export interface RawBodyOptions {
  headers?: Record<string, string>;
}

export type Handlers = {
  [C in keyof Commands]?: (
    args: Commands[C]["args"],
    options?: RawBodyOptions,
  ) => Commands[C]["result"] | Promise<Commands[C]["result"]>;
};
