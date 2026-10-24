/**
 * Errors from the core.
 *
 * A command fails with `{ code, params, message }` (`src-tauri/src/errors.rs`).
 * The window translates `error.<code>` with the values from `params`. The
 * Russian `message` is the fallback for a code that the locale files do not
 * have: a new error in the core must not show as a raw key.
 *
 * This module has no imports, so the tests run it in Node without Vite.
 */

/** An error as a command returns it. */
export interface ErrorView {
  code: string;
  params: Record<string, string>;
  message: string;
}

/** The text for a key, or `undefined` if no locale has the key. */
export type Translate = (key: string, params: Record<string, string>) => string | undefined;

export function isErrorView(value: unknown): value is ErrorView {
  if (typeof value !== "object" || value === null) return false;
  const view = value as Partial<ErrorView>;
  return typeof view.code === "string" && typeof view.message === "string";
}

/** The text to show for anything that a `catch` can get. */
export function errorText(error: unknown, translate: Translate): string {
  if (isErrorView(error)) {
    return translate(`error.${error.code}`, error.params ?? {}) ?? error.message;
  }
  if (error instanceof Error) return error.message;
  return String(error);
}
