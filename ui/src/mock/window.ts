export function getCurrentWindow() {
  return {
    onCloseRequested(_handler: () => Promise<void>): Promise<() => void> {
      return Promise.resolve(() => {});
    },
  };
}
