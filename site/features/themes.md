# Themes and language

## Built-in themes

kknow has two built-in themes: **Gruvbox dark** and **Gruvbox light**. Select a
theme in the gear menu in the status bar, or in **Settings → Appearance**.

## Your own theme

A theme is a JSON file with values for the style variables. It is not a
stylesheet.

```json
{
  "name": "Sepia",
  "base": "light",
  "variables": { "bg": "#f6efe0", "text": "#3f3527", "accent": "#9c5b28" }
}
```

`base` selects the built-in theme to start from: `light` or `dark`. kknow
applies your variables on top of it, so you set only the colors you want to
change.

Put the file in one of two places:

| Place | Who sees the theme |
|---|---|
| `<vault>/.kknow/themes/<name>.json` | only this vault |
| the shared `themes/` folder | all vaults |

On macOS, the shared folder is
`~/Library/Application Support/dev.kknow.desktop/themes/`.
**Settings → Appearance** has a button that opens it. A vault theme has a
**Make shared** button that copies it there.

To change a built-in theme, put a file with its name (`gruvbox.json` or
`gruvbox-light.json`) into the shared folder or into a vault.

## Safety

kknow accepts colors, font names and color functions. It rejects `url(…)` and
all other values that use the network. A theme can come from another person's
vault, and you have no reason to trust it.

A typo in a variable name does not break the theme. `kknow themes` reports it:

```bash
kknow --vault ~/notes themes
```

## Interface language

The interface has two languages: Russian (the default) and English. Change it
in the gear menu or in **Settings → General**. kknow does not follow the system
language: you select it, and kknow keeps it.

The labels are in the `locales/` folder of the repository, one i18next JSON
file per language. To add a language:

1. Copy `locales/ru.json` to `locales/<code>.json` and translate it.
2. Add the code to `LOCALES` in `src-tauri/src/menu.rs`.
3. Run `pnpm ui:test`. It checks that the file has all keys and placeholders.

Error messages from the core and the CLI output stay in Russian.

## Keyboard shortcuts

You can change any shortcut. See
[Keyboard shortcuts → Change a shortcut](/reference/shortcuts#change-a-shortcut).
