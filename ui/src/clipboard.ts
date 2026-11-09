/**
 * Puts text on the clipboard. Returns `false` if the webview did not let it.
 *
 * First try `navigator.clipboard`. If the webview blocks it (no permission or
 * an insecure context), fall back to the old `execCommand("copy")` with a
 * hidden field: it works with any user gesture, and a click is one.
 */
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Fall back below.
  }

  const field = document.createElement("textarea");
  field.value = text;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.append(field);
  field.select();
  const copied = document.execCommand("copy");
  field.remove();
  return copied;
}
