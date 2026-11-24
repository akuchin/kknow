const values = new Map<string, string>();
const checks = new Map<string, boolean>();

export function draftValue(key: string, saved = ""): string {
  return values.get(key) ?? saved;
}

export function setDraft(key: string, value: string, saved = ""): void {
  if (value === saved) values.delete(key);
  else values.set(key, value);
}

export function hasDraft(key: string): boolean {
  return values.has(key) || checks.has(key);
}

export function clearDraft(key: string): void {
  values.delete(key);
  checks.delete(key);
}

export function clearDraftsUnder(prefix: string): void {
  for (const map of [values, checks]) {
    for (const each of [...map.keys()]) {
      if (each.startsWith(prefix)) map.delete(each);
    }
  }
}

export function draftChecked(key: string, saved: boolean): boolean {
  return checks.get(key) ?? saved;
}

export function setDraftChecked(key: string, checked: boolean, saved: boolean): void {
  if (checked === saved) checks.delete(key);
  else checks.set(key, checked);
}

type Field = HTMLInputElement | HTMLTextAreaElement;

export function bindDraft(field: Field, key: string, saved = ""): void {
  field.value = draftValue(key, saved);
  field.dataset.draft = key;
  field.addEventListener("input", () => setDraft(key, field.value, saved));
}

export function bindDraftCheck(box: HTMLInputElement, key: string, saved: boolean): void {
  box.checked = draftChecked(key, saved);
  box.dataset.draft = key;
  box.addEventListener("change", () => setDraftChecked(key, box.checked, saved));
}

export function keepFocus(root: ParentNode = document): () => void {
  const active = document.activeElement as Field | null;
  const key = active?.dataset?.draft;
  if (!key) return () => {};
  const text = typeof active.selectionStart === "number";
  const start = text ? active.selectionStart : null;
  const end = text ? active.selectionEnd : null;
  return () => {
    const field = root.querySelector<Field>(`[data-draft="${CSS.escape(key)}"]`);
    if (!field || field === active) return;
    field.focus();
    if (start !== null && end !== null) {
      try {
        field.setSelectionRange(start, end);
      } catch {
      }
    }
  };
}
