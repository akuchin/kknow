export async function open(options?: { filters?: unknown[] }): Promise<string | null> {
  return options?.filters
    ? (window.prompt("Path to a markdown file", "/Users/me/Desktop/draft.md") ?? null)
    : (window.prompt("Path to a notes folder", "~/notes") ?? null);
}
