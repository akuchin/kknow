export type Params = Record<string, string | number>;

export type Dict = Record<string, string>;

export function lookup(
  dicts: readonly Dict[],
  plurals: Intl.PluralRules,
  key: string,
  params: Params,
): string | undefined {
  for (const dict of dicts) {
    const text = lookupIn(dict, plurals, key, params);
    if (text !== undefined) return text;
  }
  return undefined;
}

function lookupIn(
  dict: Dict,
  plurals: Intl.PluralRules,
  key: string,
  params: Params,
): string | undefined {
  if (key in dict) return dict[key];
  const category = plurals.select(Number(params.count ?? 0));
  return dict[`${key}_${category}`] ?? dict[`${key}_other`];
}

export function fill(text: string, params: Params): string {
  return text.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}
