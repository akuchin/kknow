/**
 * Unsaved values of form fields that the UI draws again.
 *
 * Many panels build their DOM from the state and replace it on each change:
 * the comment float after a reply or a sync, the vault page after a color or
 * a policy change. A field that lives only in the DOM loses its text then,
 * and the person loses what they typed in a neighbouring field. So a field
 * that can outlive one render keeps its value here, under a key that names
 * what the field edits (a thread, a proposal, the S3 endpoint of a vault).
 *
 * A draft lives until its form saves it. A value equal to the saved one is
 * not a draft: the next render shows the saved value, which may be newer.
 * Drafts stay in memory only, so a typed secret does not reach a disk.
 */

const values = new Map<string, string>();
const checks = new Map<string, boolean>();

/** The draft under `key`, or the saved value when there is no draft. */
export function draftValue(key: string, saved = ""): string {
  return values.get(key) ?? saved;
}

/** Keeps `value` as the draft, or drops it when it equals the saved value. */
export function setDraft(key: string, value: string, saved = ""): void {
  if (value === saved) values.delete(key);
  else values.set(key, value);
}

export function hasDraft(key: string): boolean {
  return values.has(key) || checks.has(key);
}

/** Drops the draft under exactly this key. */
export function clearDraft(key: string): void {
  values.delete(key);
  checks.delete(key);
}

/**
 * Drops the drafts of a whole form: all keys that start with `prefix`. A
 * form that saves several fields at once (the S3 form) clears them together.
 */
export function clearDraftsUnder(prefix: string): void {
  for (const map of [values, checks]) {
    for (const each of [...map.keys()]) {
      if (each.startsWith(prefix)) map.delete(each);
    }
  }
}

/** The state of a checkbox draft, or the saved one. */
export function draftChecked(key: string, saved: boolean): boolean {
  return checks.get(key) ?? saved;
}

export function setDraftChecked(key: string, checked: boolean, saved: boolean): void {
  if (checked === saved) checks.delete(key);
  else checks.set(key, checked);
}

type Field = HTMLInputElement | HTMLTextAreaElement;

/**
 * Binds a text field to the draft under `key`: the field shows the draft and
 * each keystroke updates it. `saved` is what the field shows without a draft.
 */
export function bindDraft(field: Field, key: string, saved = ""): void {
  field.value = draftValue(key, saved);
  field.dataset.draft = key;
  field.addEventListener("input", () => setDraft(key, field.value, saved));
}

/** Binds a checkbox to the draft under `key`, as `bindDraft` does for text. */
export function bindDraftCheck(box: HTMLInputElement, key: string, saved: boolean): void {
  box.checked = draftChecked(key, saved);
  box.dataset.draft = key;
  box.addEventListener("change", () => setDraftChecked(key, box.checked, saved));
}

/**
 * The focused draft field and its cursor, so a render can give them back.
 * Call it before the render replaces the DOM and call the result after.
 */
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
        // A number field has no selection.
      }
    }
  };
}
