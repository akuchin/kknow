type Handler = (event: { payload: unknown }) => void;

const handlers = new Map<string, Set<Handler>>();

const tabs = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("kknow:mock:events");

function deliver(event: string, payload: unknown): void {
  for (const handler of handlers.get(event) ?? []) handler({ payload });
}

tabs?.addEventListener("message", ({ data }: MessageEvent<{ event: string; payload: unknown }>) =>
  deliver(data.event, data.payload),
);

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
