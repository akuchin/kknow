/** Mock of the folder or file picker for mock mode. */
export async function open(options?: { filters?: unknown[] }): Promise<string | null> {
  // With filters the dialog asks for a file, without them for a folder.
  return options?.filters
    ? (window.prompt("Path to a markdown file", "/Users/me/Desktop/draft.md") ?? null)
    : (window.prompt("Path to a notes folder", "~/notes") ?? null);
}
