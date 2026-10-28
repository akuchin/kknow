/**
 * Setup snippets for the MCP server of the app: what a person pastes into an
 * agent.
 *
 * The app listens on `127.0.0.1:<port>/mcp` while it runs
 * (`src-tauri/src/mcp_http.rs`). The agent needs the address and the token,
 * nothing to install. The address tells which vaults the agent gets:
 *
 * - `?vault=<id>`: one vault. The ID, not the name: the name can change;
 * - `?label=a&label=b`: all vaults with one of the labels. A new vault with
 *   the label comes in without a new setup;
 * - nothing: all vaults of the list.
 */

/** Which vaults the agent gets. */
export type McpScope = { vault: string } | { labels: string[] } | { all: true };

/**
 * Server name in the agent configuration. Each scope gets its own name: a
 * person can add `kknow-work` and `kknow-study` next to each other.
 */
export function serverName(scope: McpScope): string {
  if ("labels" in scope) return ["kknow", ...scope.labels].join("-");
  if ("all" in scope) return "kknow-all";
  return "kknow";
}

/** The address of the server for the scope. */
export function mcpUrl(port: number, scope: McpScope): string {
  const query =
    "labels" in scope
      ? scope.labels.map((label) => `label=${encodeURIComponent(label)}`).join("&")
      : "vault" in scope
        ? `vault=${encodeURIComponent(scope.vault)}`
        : "";
  return `http://127.0.0.1:${port}/mcp${query ? `?${query}` : ""}`;
}

/** The two snippets: a Claude Code command and a JSON block for other clients. */
export interface McpSnippets {
  claudeCode: string;
  json: string;
}

/**
 * Quotes a word for a shell. A plain word stays as it is, so the common case
 * reads well; anything else goes into single quotes. bash, zsh and PowerShell
 * read single quotes the same way.
 */
export function shellQuote(word: string): string {
  if (/^[\w@%+=:,./-]+$/.test(word)) return word;
  return `'${word.replaceAll("'", `'\\''`)}'`;
}

export function mcpSnippets(port: number, token: string, scope: McpScope): McpSnippets {
  const name = serverName(scope);
  const url = mcpUrl(port, scope);
  const auth = `Bearer ${token}`;
  const claudeCode = [
    "claude", "mcp", "add", "--transport", "http", name, url,
    "--header", `Authorization: ${auth}`,
  ]
    .map(shellQuote)
    .join(" ");
  // The format of `.mcp.json` in Claude Code and of Cursor: `type`, `url` and
  // `headers`. By hand, not `JSON.stringify(…, 2)`: that puts the one header
  // on three lines, and the block gets much taller.
  const json = [
    "{",
    `  "mcpServers": {`,
    `    ${JSON.stringify(name)}: {`,
    `      "type": "http",`,
    `      "url": ${JSON.stringify(url)},`,
    `      "headers": { "Authorization": ${JSON.stringify(auth)} }`,
    "    }",
    "  }",
    "}",
  ].join("\n");
  return { claudeCode, json };
}
