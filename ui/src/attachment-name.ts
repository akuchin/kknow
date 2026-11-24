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

export function pastedImageName(type: string, now: Date): string | null {
  const ext = EXTENSIONS[type.toLowerCase()];
  if (!ext) return null;
  const two = (value: number) => String(value).padStart(2, "0");
  const stamp =
    `${now.getFullYear()}${two(now.getMonth() + 1)}${two(now.getDate())}` +
    `${two(now.getHours())}${two(now.getMinutes())}${two(now.getSeconds())}`;
  return `Pasted image ${stamp}.${ext}`;
}

export function imageLink(path: string): string {
  return `![[${path}]]`;
}
