export interface ErrorView {
  code: string;
  params: Record<string, string>;
  message: string;
}

export type Translate = (key: string, params: Record<string, string>) => string | undefined;

export function isErrorView(value: unknown): value is ErrorView {
  if (typeof value !== "object" || value === null) return false;
  const view = value as Partial<ErrorView>;
  return typeof view.code === "string" && typeof view.message === "string";
}

export function errorText(error: unknown, translate: Translate): string {
  if (isErrorView(error)) {
    return translate(`error.${error.code}`, error.params ?? {}) ?? error.message;
  }
  if (error instanceof Error) return error.message;
  return String(error);
}
