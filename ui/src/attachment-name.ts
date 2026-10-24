/**
 * Names and links for pasted images. The module has no imports on purpose:
 * the tests run it in plain Node (`ui/tests`).
 */

/** Clipboard types that the image loader can show, and their extensions. */
const EXTENSIONS: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
  "image/svg+xml": "svg",
  "image/avif": "avif",
  "image/bmp": "bmp",
  "image/x-icon": "ico",
  "image/vnd.microsoft.icon": "ico",
};

/**
 * `Pasted image 20261024153012.png`, as in Obsidian. Local time: the user
 * reads it. `null` for a type that the loader cannot show.
 */
export function pastedImageName(type: string, now: Date): string | null {
  const ext = EXTENSIONS[type.toLowerCase()];
  if (!ext) return null;
  const two = (value: number) => String(value).padStart(2, "0");
  const stamp =
    `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}` +
    `${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
  return `Pasted image ${stamp}.${ext}`;
}

/** The link for a path that the core returned. */
export function imageLink(path: string): string {
  return `![[${path}]]`;
}
