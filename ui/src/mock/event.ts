/** Mock of Tauri events: in the browser nothing watches the files. */
export async function listen<T>(
  _event: string,
  _handler: (event: { payload: T }) => void,
): Promise<() => void> {
  return () => {};
}
