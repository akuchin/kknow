/** Mock of the folder or file picker for mock mode. */
export async function open(options?: { filters?: unknown[] }): Promise<string | null> {
  // With filters the dialog asks for a file, without them for a folder.
  return options?.filters
    ? (window.prompt("Путь к markdown-файлу", "/Users/me/Desktop/черновик.md") ?? null)
    : (window.prompt("Путь к каталогу с заметками", "~/notes") ?? null);
}
