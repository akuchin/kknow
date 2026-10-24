/**
 * Window mock: the main window needs only the close hook from it.
 *
 * A browser tab cannot wait for a save before it closes, so the mock never
 * calls the handler.
 */

export function getCurrentWindow() {
  return {
    onCloseRequested(_handler: () => Promise<void>): Promise<() => void> {
      return Promise.resolve(() => {});
    },
  };
}
