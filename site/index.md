---
layout: home

hero:
  name: kknow
  text: Your notes stay plain Markdown
  tagline: A fast, local-first knowledge base. A Rust core, a light desktop window, a CLI and an MCP server for AI agents — all on the same folder of .md files.
  image:
    src: /logo.png
    alt: kknow
  actions:
    - theme: brand
      text: Quick start
      link: /guide/quick-start
    - theme: alt
      text: Features
      link: /features/
    - theme: alt
      text: GitHub
      link: https://github.com/akuchin/kknow

features:
  - icon: 📁
    title: Files are the source of truth
    details: A vault is an ordinary folder of .md files. Any editor, git or sync tool can use it. The index and the link graph are only a cache.
    link: /guide/what-is-kknow
  - icon: ✍️
    title: Live preview editor
    details: Headings, tables, images and Mermaid diagrams look finished while you type. Three modes per note — Edit, Side by side and Read.
    link: /features/editor
  - icon: 🔗
    title: Wiki links that survive a move
    details: "[[links]], embeds, tags and backlinks. A rename or a move updates the links in the other notes."
    link: /features/links
  - icon: 🔎
    title: Search in all vaults at once
    details: Full-text search on tantivy with a Russian stemmer. ⌘K finds a note in any vault in milliseconds.
    link: /features/search
  - icon: ☁️
    title: Sync without a server
    details: S3-compatible storage, iCloud Drive, Dropbox or any folder. A three-way plan keeps both versions on a conflict.
    link: /features/sync
  - icon: 🤖
    title: Built for AI agents
    details: The app serves your vaults over MCP. Leave a comment on a word, and an agent does the task.
    link: /features/agents
  - icon: ⚡
    title: Your own commands
    details: Attach a shell, fish or Python script to a note, a folder or a vault. Publish to Confluence with one key.
    link: /features/commands
  - icon: 🎨
    title: Themes and keys
    details: Gruvbox dark and light, your own JSON themes, and every shortcut can change.
    link: /features/themes
  - icon: 🖥️
    title: macOS, Windows, Linux
    details: Install with Homebrew or Scoop, or take a build from the Releases page. The app updates itself.
    link: /guide/quick-start
---
