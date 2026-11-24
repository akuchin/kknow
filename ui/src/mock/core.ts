import { filesHandlers } from "./files";
import { gitHandlers } from "./git";
import type { Handlers, RawBodyOptions } from "./handlers";
import { locksHandlers } from "./locks";
import { notesHandlers } from "./notes";
import { proposalsHandlers } from "./proposals";
import { runsHandlers } from "./runs";
import { settingsHandlers } from "./settings";
import { syncHandlers } from "./sync";
import { trashHandlers } from "./trash";
import { coded } from "./store";
import { vaultsHandlers } from "./vaults";

export class Channel<T> {
  onmessage: (message: T) => void = () => {};
}

const handlers: Handlers = {
  ...vaultsHandlers,
  ...locksHandlers,
  ...notesHandlers,
  ...trashHandlers,
  ...proposalsHandlers,
  ...settingsHandlers,
  ...runsHandlers,
  ...syncHandlers,
  ...gitHandlers,
  ...filesHandlers,
};

function plannedFailure(command: string): string | null {
  try {
    const plan: { command: string; code: string } | null = JSON.parse(
      sessionStorage.getItem("kknow:mock:fail") ?? "null",
    );
    return plan?.command === command ? plan.code : null;
  } catch {
    return null;
  }
}

type Handler = (args: unknown, options?: RawBodyOptions) => unknown;

export async function invoke<T>(
  command: string,
  args: Record<string, unknown> | Uint8Array = {},
  options?: RawBodyOptions,
): Promise<T> {
  const handler = (handlers as Record<string, Handler | undefined>)[command];
  const failure = plannedFailure(command);
  if (failure) throw coded(failure, `the mock fails ${command} on purpose`);
  if (!handler) throw new Error(`the mock does not know the command ${command}`);
  return (await handler(args, options)) as T;
}
