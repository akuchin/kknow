export type McpScope = { vault: string } | { labels: string[] } | { all: true };

export function serverName(scope: McpScope): string {
  if ("labels" in scope) return ["kknow", ...scope.labels].join("-");
  if ("all" in scope) return "kknow-all";
  return "kknow";
}

export function mcpUrl(port: number, scope: McpScope): string {
  const query =
    "labels" in scope
      ? scope.labels.map((label) => `label=${encodeURIComponent(label)}`).join("&")
      : "vault" in scope
        ? `vault=${encodeURIComponent(scope.vault)}`
        : "";
  return `http://127.0.0.1:${port}/mcp${query ? `?${query}` : ""}`;
}

export interface McpSnippets {
  claudeCode: string;
  json: string;
}

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
