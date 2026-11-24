interface Labeled {
  id: string;
  labels: string[];
}

export function labelsOf(vaults: Labeled[]): string[] {
  return [...new Set(vaults.flatMap((vault) => vault.labels))];
}

export function filterVaults<T extends Labeled>(vaults: T[], picked: string[], keep: string | null): T[] {
  if (picked.length === 0) return vaults;
  return vaults.filter(
    (vault) => vault.id === keep || vault.labels.some((label) => picked.includes(label)),
  );
}

export function livePicks(picked: string[], vaults: Labeled[]): string[] {
  const labels = labelsOf(vaults);
  return picked.filter((label) => labels.includes(label));
}
