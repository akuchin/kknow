import { defineConfig } from "vitepress";

// GitHub Pages serves the site from https://akuchin.github.io/kknow/,
// so all links get the `/kknow/` prefix. `KKNOW_SITE_BASE` overrides it,
// for example `/` for a custom domain.
const base = process.env.KKNOW_SITE_BASE ?? "/kknow/";

export default defineConfig({
  base,
  lang: "en-US",
  title: "kknow",
  description:
    "A fast, local-first knowledge base for plain Markdown files. Rust core, Tauri window, CLI and MCP server.",
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ["link", { rel: "icon", type: "image/png", href: `${base}favicon.png` }],
    ["meta", { name: "theme-color", content: "#4a5cd6" }],
  ],
  themeConfig: {
    logo: "/logo.png",
    nav: [
      { text: "Quick start", link: "/guide/quick-start" },
      { text: "Features", link: "/features/" },
      { text: "Architecture", link: "/architecture" },
      { text: "Reference", link: "/reference/shortcuts" },
      {
        text: "Releases",
        link: "https://github.com/akuchin/kknow/releases",
      },
    ],
    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "What is kknow", link: "/guide/what-is-kknow" },
          { text: "Quick start", link: "/guide/quick-start" },
          { text: "Updates", link: "/guide/updates" },
        ],
      },
      {
        text: "Features",
        items: [
          { text: "Overview", link: "/features/" },
          { text: "Editor", link: "/features/editor" },
          { text: "Links and Markdown", link: "/features/links" },
          { text: "Vaults and the sidebar", link: "/features/vaults" },
          { text: "Search", link: "/features/search" },
          { text: "Sync", link: "/features/sync" },
          { text: "Agents (MCP)", link: "/features/agents" },
          { text: "Bookmarks, tasks, trash", link: "/features/organize" },
          { text: "Custom commands", link: "/features/commands" },
          { text: "Themes and language", link: "/features/themes" },
          { text: "Standalone files", link: "/features/standalone-files" },
        ],
      },
      {
        text: "Under the hood",
        items: [
          { text: "Architecture", link: "/architecture" },
          { text: "Development", link: "/development" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Keyboard shortcuts", link: "/reference/shortcuts" },
          { text: "CLI", link: "/reference/cli" },
          { text: "Files and folders", link: "/reference/files" },
        ],
      },
    ],
    socialLinks: [{ icon: "github", link: "https://github.com/akuchin/kknow" }],
    editLink: {
      pattern: "https://github.com/akuchin/kknow/edit/main/site/:path",
      text: "Edit this page on GitHub",
    },
    search: { provider: "local" },
    outline: { level: [2, 3] },
    footer: {
      message: "Released under the MIT License.",
      copyright: "© 2026 Artem Kuchyn",
    },
  },
  markdown: {
    // A `mermaid` code block becomes a diagram, drawn in the browser by
    // `theme/Mermaid.vue`.
    config(md) {
      const fence = md.renderer.rules.fence!;
      md.renderer.rules.fence = (tokens, index, options, env, self) => {
        const token = tokens[index];
        if (token.info.trim() === "mermaid") {
          return `<Mermaid code="${encodeURIComponent(token.content)}" />`;
        }
        return fence(tokens, index, options, env, self);
      };
    },
  },
  // Mermaid is one large chunk, and the site loads it only on pages with
  // diagrams.
  vite: { build: { chunkSizeWarningLimit: 4000 } },
});
