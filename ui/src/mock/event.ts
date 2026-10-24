/**
 * Mock of Tauri events: in the browser nothing watches the files.
 *
 * Only the events that the mock commands send arrive. The core sends them to
 * every window with `app.emit`, so the mock sends them to every tab of the
 * page too: the settings window opens as another tab, and a theme picked there
 * must reach the main one.
 */

type Handler = (event: { payload: unknown }) => void;

const handlers = new Map<string, Set<Handler>>();

/** Tabs of the same origin; a missing API leaves events in this tab only. */
const tabs = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("kknow:mock:events");

function deliver(event: string, payload: unknown): void {
  for (const handler of handlers.get(event) ?? []) handler({ payload });
}

tabs?.addEventListener("message", ({ data }: MessageEvent<{ event: string; payload: unknown }>) =>
  deliver(data.event, data.payload),
);

/** Sends an event to this tab and to the others, as `app.emit` does. */
export function emit(event: string, payload: unknown): void {
  deliver(event, payload);
  tabs?.postMessage({ event, payload });
}

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const set = handlers.get(event) ?? new Set<Handler>();
  handlers.set(event, set);
  set.add(handler as Handler);
  return () => set.delete(handler as Handler);
}
