/**
 * Button icons.
 *
 * An own set, not a library: there are a few dozen, all line icons on a 24×24
 * grid, so a dependency is not worth it. The color comes from the text
 * (`currentColor`), so the theme colors them. CSS sets the size through
 * `.icon`, because it differs from place to place.
 */

/** The content of each `<svg>`: one or more paths. */
const PATHS = {
  // Notes folder.
  folder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
  // Add vault: a folder with a plus sign.
  folderPlus: [
    "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    "M12 10v6",
    "M9 13h6",
  ],
  // Plus: a new note from a tree row.
  plus: ["M12 5v14", "M5 12h14"],
  // Ellipsis: the menu of a tree row.
  dots: ["M6 12h.01", "M12 12h.01", "M18 12h.01"],
  // Right panel: preview.
  panel: ["M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M15 3v18"],
  // Header: sidebar, history and the three note modes.
  sidebar: ["M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M9 3v18"],
  back: ["m14.5 5-7 7 7 7"],
  split: ["M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", "M12 3v18"],
  book: ["M2 5h6a4 4 0 0 1 4 4v11a3 3 0 0 0-3-3H2z", "M22 5h-6a4 4 0 0 0-4 4v11a3 3 0 0 1 3-3h7z"],
  // Cloud: the vault that is synced.
  cloud: ["M17.5 19H7a4.5 4.5 0 0 1-.9-8.9 6 6 0 0 1 11.4 1.4A3.8 3.8 0 0 1 17.5 19Z"],
  // Reload: two circular arrows.
  refresh: [
    "M21 12a9 9 0 0 0-15.2-6.5L3 8",
    "M3 3v5h5",
    "M3 12a9 9 0 0 0 15.2 6.5L21 16",
    "M21 21v-5h-5",
  ],
  // Palette: theme choice.
  palette: [
    "M12 3a9 9 0 1 0 0 18 1.9 1.9 0 0 0 1.5-3.1 1.9 1.9 0 0 1 1.5-3.1h1.8A4.2 4.2 0 0 0 21 10.6 9 9 0 0 0 12 3Z",
    "M7.5 11.5h.01",
    "M10.5 7.5h.01",
    "M15.5 8.5h.01",
  ],
  // Globe: language choice.
  globe: [
    "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18",
    "M3.5 9h17",
    "M3.5 15h17",
    "M12 3c2.3 2.4 3.6 5.6 3.6 9S14.3 18.6 12 21c-2.3-2.4-3.6-5.6-3.6-9S9.7 5.4 12 3",
  ],
  // Tree folder: a rotated arrow marks an open folder, not a second icon.
  chevron: ["m9.5 5 7 7-7 7"],
  // A button with a menu: the vault switcher in the header.
  caret: ["m6 9.5 6 6 6-6"],
  // Settings: a gear.
  gear: [
    "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4",
    "M12 2.6v2.4M12 19v2.4M4.4 4.4l1.7 1.7M17.9 17.9l1.7 1.7M2.6 12h2.4M19 12h2.4M19.6 4.4l-1.7 1.7M6.1 17.9l-1.7 1.7",
  ],
  search: ["M11 4a7 7 0 1 0 0 14 7 7 0 1 0 0-14", "M16.5 16.5 21 21"],
  // Bookmark: a ribbon with a notch at the bottom.
  bookmark: ["M6 3h12v18l-6-4-6 4z"],
  // A heading in the note outline: the Markdown hash.
  heading: ["M5 9h15", "M4 15h15", "M10 4 8 20", "M16 4l-2 16"],
  // A task in palette results: a box with a check mark.
  task: ["M5 4h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z", "m8 12 3 3 5-6"],
  // A note in palette results: a page with two lines of text.
  note: [
    "M13 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9z",
    "M13 3v6h6",
    "M9 13h6",
    "M9 17h4",
  ],
  // New note.
  filePlus: ["M13 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9z", "M13 3v6h6", "M12 12v5", "M9.5 14.5h5"],
  // Close, hide.
  close: ["M18 6 6 18", "M6 6l12 12"],
  // Save.
  check: ["M20 6 9 17l-5-5"],
  // Remove vault: the same chain, crossed out.
  unlink: [
    "M10 13a5 5 0 0 0 7.1.4l2.4-2.4a5 5 0 0 0-7-7L11.2 5.6",
    "M14 11a5 5 0 0 0-7.1-.4l-2.4 2.4a5 5 0 0 0 7 7l1.3-1.4",
    "M3 3l18 18",
  ],
  // Show markup: `</>`.
  code: ["M16 18l6-6-6-6", "M8 6l-6 6 6 6"],
  // File operations in the tree menu.
  pencil: ["M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z", "m15 5 4 4"],
  folderMove: [
    "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    "M12 10v6",
    "m9 13 3 3 3-3",
  ],
  copy: [
    "M9 9h10a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2z",
    "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1",
  ],
  link: [
    "M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7",
    "M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7",
  ],
  // Backlink: an arrow that comes back into the page.
  backlink: ["M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4", "M10 17 5 12l5-5", "M5 12h11"],
  external: ["M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6", "M15 3h6v6", "M10 14 21 3"],
  // Run the last command: a triangle, as on a player.
  play: ["M7 4v16l13-8z"],
  // Stop a running command: a square, as on a player.
  stop: ["M6 6h12v12H6z"],
  // Collapse all: two chevrons that point at a line in the middle.
  collapse: ["M4 12h16", "m8 3 4 4 4-4", "m8 21 4-4 4 4"],
  // Command: a terminal window with a prompt.
  terminal: [
    "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    "m7 9 3 3-3 3",
    "M13 15h4",
  ],
  // Table toolbar: row and column, plus and minus.
  rowAdd: ["M3 4h18v6H3z", "M12 14v6", "M9 17h6"],
  rowRemove: ["M3 4h18v6H3z", "M9 17h6"],
  columnAdd: ["M4 3h6v18H4z", "M17 9v6", "M14 12h6"],
  columnRemove: ["M4 3h6v18H4z", "M14 12h6"],
  // Table column alignment: the lines line up on the left, center or right.
  alignLeft: ["M4 6h16", "M4 10h10", "M4 14h16", "M4 18h10"],
  alignCenter: ["M4 6h16", "M7 10h10", "M4 14h16", "M7 18h10"],
  alignRight: ["M4 6h16", "M10 10h10", "M4 14h16", "M10 18h10"],
  // Moving table rows and columns.
  rowUp: ["M12 19V5", "m6 11 6-6 6 6"],
  rowDown: ["M12 5v14", "m6 13 6 6 6-6"],
  columnLeft: ["M19 12H5", "m11 6-6 6 6 6"],
  columnRight: ["M5 12h14", "m13 6 6 6-6 6"],
  // Sorting table rows: an arrow and bars that grow or shrink.
  sortAsc: ["M7 20V4", "m3 8 4-4 4 4", "M14 6h3", "M14 12h5", "M14 18h7"],
  sortDesc: ["M7 4v16", "m3 16 4 4 4-4", "M14 6h7", "M14 12h5", "M14 18h3"],
  // Transpose: a square that flips over its diagonal.
  transpose: ["M4 4h16v16H4z", "M4 4l16 16", "M9 4v5", "M15 20v-5"],
  // Line numbers: a list with 1 and 2 in front of the lines.
  lineNumbers: [
    "M10 6h11",
    "M10 12h11",
    "M10 18h11",
    "M4 6h1v4",
    "M4 10h2",
    "M6 18H4c0-1 2-2 2-3s-1-1.5-2-1",
  ],
  // Agent note and its removal.
  comment: ["M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"],
  // Confluence page of a note: two waves, as in the product logo.
  confluence: ["M3 17c3.5-5.5 7.5-6.5 13-3l4 2.5", "M21 7c-3.5 5.5-7.5 6.5-13 3L4 7.5"],
  trash: ["M3 6h18", "M8 6V4h8v2", "M19 6l-1 14H6L5 6"],
} satisfies Record<string, string[]>;

export type IconName = keyof typeof PATHS;

/**
 * Icon as markup, for templates built as strings.
 *
 * The icon is decorative: the button has its own label, and a screen reader
 * must not read the same thing twice. Hence `aria-hidden`.
 */
export function iconMarkup(name: IconName): string {
  const paths = PATHS[name].map((d) => `<path d="${d}" />`).join("");
  return (
    `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"` +
    ` stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"` +
    ` aria-hidden="true">${paths}</svg>`
  );
}

/** Icon as an element, for code that builds the DOM by hand. */
export function icon(name: IconName): SVGSVGElement {
  const template = document.createElement("template");
  template.innerHTML = iconMarkup(name);
  return template.content.firstElementChild as SVGSVGElement;
}
