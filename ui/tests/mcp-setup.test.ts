// MCP setup snippets (`src/mcp-setup.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { mcpSnippets, mcpUrl, shellQuote } from "../src/mcp-setup.ts";

const TOKEN = "0123456789abcdef";

test("the address tells which vaults the agent gets", () => {
  assert.equal(mcpUrl(7377, { all: true }), "http://127.0.0.1:7377/mcp");
  assert.equal(mcpUrl(7377, { vault: "заметки" }), "http://127.0.0.1:7377/mcp?vault=%D0%B7%D0%B0%D0%BC%D0%B5%D1%82%D0%BA%D0%B8");
  assert.equal(
    mcpUrl(8000, { labels: ["work", "ai"] }),
    "http://127.0.0.1:8000/mcp?label=work&label=ai",
  );
});

test("the Claude Code command adds an HTTP server with the token", () => {
  const { claudeCode } = mcpSnippets(7377, TOKEN, { labels: ["work"] });

  assert.equal(
    claudeCode,
    // `?` is a glob in zsh, so the address goes into quotes.
    "claude mcp add --transport http kknow-work 'http://127.0.0.1:7377/mcp?label=work' " +
      `--header 'Authorization: Bearer ${TOKEN}'`,
  );
});

test("an address with & is quoted for the shell", () => {
  const { claudeCode } = mcpSnippets(7377, TOKEN, { labels: ["work", "ai"] });

  assert.ok(claudeCode.includes(" kknow-work-ai 'http://127.0.0.1:7377/mcp?label=work&label=ai' "));
  assert.equal(shellQuote("it's"), `'it'\\''s'`);
});

test("the JSON block has the same address and header", () => {
  const { json } = mcpSnippets(7377, TOKEN, { vault: "notes" });

  assert.deepEqual(JSON.parse(json), {
    mcpServers: {
      kknow: {
        type: "http",
        url: "http://127.0.0.1:7377/mcp?vault=notes",
        headers: { Authorization: `Bearer ${TOKEN}` },
      },
    },
  });
});
