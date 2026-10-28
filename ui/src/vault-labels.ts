// The label filter of the sidebar as data: which labels exist and which
// vaults a filter leaves. The DOM part lives in `window/tree.ts`.

/** What the filter needs of a vault. */
interface Labeled {
  id: string;
  labels: string[];
}

/** All labels of the vaults, each once, in the order they first appear. */
export function labelsOf(vaults: Labeled[]): string[] {
  return [...new Set(vaults.flatMap((vault) => vault.labels))];
}

/**
 * The vaults with at least one of the picked labels. No picked label means no
 * filter. The active vault (`keep`) always stays: its note is open, and a
 * tree without it would hide where the note lives.
 */
export function filterVaults<T extends Labeled>(vaults: T[], picked: string[], keep: string | null): T[] {
  if (picked.length === 0) return vaults;
  return vaults.filter(
    (vault) => vault.id === keep || vault.labels.some((label) => picked.includes(label)),
  );
}

/** Drops picked labels that no vault has any more: a removed label must not hide everything. */
export function livePicks(picked: string[], vaults: Labeled[]): string[] {
  const labels = labelsOf(vaults);
  return picked.filter((label) => labels.includes(label));
}
