// Checks for the translation files (`locales/*.json`). The type system checks
// only the keys that the code uses; these tests check the files against each
// other. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const DIR = new URL("../../locales/", import.meta.url);
const REFERENCE = "ru";
const CATEGORIES = ["zero", "one", "two", "few", "many", "other"];

type Dict = Record<string, string>;

const locales: Record<string, Dict> = Object.fromEntries(
  readdirSync(DIR)
    .filter((file) => file.endsWith(".json"))
    .map((file) => [file.slice(0, -5), JSON.parse(readFileSync(new URL(file, DIR), "utf8"))]),
);

/** The key without its plural suffix, and the suffix. */
function split(key: string): [string, string | null] {
  const match = /^(.+)_([a-z]+)$/.exec(key);
  return match && CATEGORIES.includes(match[2]) ? [match[1], match[2]] : [key, null];
}

/** Base key → its plural categories (empty for a plain string). */
function shape(dict: Dict): Map<string, Set<string>> {
  const result = new Map<string, Set<string>>();
  for (const key of Object.keys(dict)) {
    const [base, category] = split(key);
    const forms = result.get(base) ?? new Set();
    if (category) forms.add(category);
    result.set(base, forms);
  }
  return result;
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1]).sort();
}

test("эталонный словарь на месте", () => {
  assert.ok(REFERENCE in locales);
  assert.ok(Object.keys(locales).length > 1);
});

for (const [name, dict] of Object.entries(locales)) {
  test(`${name}: только строки и имя языка`, () => {
    for (const [key, value] of Object.entries(dict)) {
      assert.equal(typeof value, "string", key);
    }
    assert.ok(dict["locale.name"], "нет locale.name");
  });

  test(`${name}: формы множественного числа из правил языка`, () => {
    const allowed = new Intl.PluralRules(name).resolvedOptions().pluralCategories;
    for (const [base, forms] of shape(dict)) {
      if (forms.size === 0) continue;
      assert.ok(forms.has("other"), `${base}: нет _other`);
      for (const form of forms) {
        assert.ok(allowed.includes(form as Intl.LDMLPluralRule), `${base}_${form}`);
      }
    }
  });

  if (name === REFERENCE) continue;

  test(`${name}: те же ключи и подстановки, что в ${REFERENCE}`, () => {
    const reference = shape(locales[REFERENCE]);
    const own = shape(dict);
    const missing = [...reference.keys()].filter((key) => !own.has(key));
    const extra = [...own.keys()].filter((key) => !reference.has(key));
    assert.deepEqual(missing, [], "ключей не хватает");
    assert.deepEqual(extra, [], "лишние ключи");

    for (const [base, forms] of reference) {
      assert.equal(own.get(base)!.size === 0, forms.size === 0, `${base}: строка или формы`);
      const want = placeholders(locales[REFERENCE][forms.size ? `${base}_other` : base]);
      const ownForms = own.get(base)!;
      for (const key of ownForms.size ? [...ownForms].map((f) => `${base}_${f}`) : [base]) {
        assert.deepEqual(placeholders(dict[key]), want, key);
      }
    }
  });
}
