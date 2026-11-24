export type Place = { file: true; path: string } | { file: false; vault: string; id: string };

export function samePlace(a: Place, b: Place): boolean {
  return a.file
    ? b.file && a.path === b.path
    : !b.file && a.vault === b.vault && a.id === b.id;
}

export class History {
  private readonly places: Place[] = [];
  private at = -1;
  private travelling = false;
  private readonly limit: number;

  constructor(limit = 100) {
    this.limit = limit;
  }

  get canBack(): boolean {
    return this.at > 0;
  }

  get canForward(): boolean {
    return this.at < this.places.length - 1;
  }

  remember(place: Place): boolean {
    if (this.travelling) return false;
    if (this.at >= 0 && samePlace(this.places[this.at], place)) return false;
    this.places.splice(this.at + 1);
    this.places.push(place);
    if (this.places.length > this.limit) this.places.shift();
    this.at = this.places.length - 1;
    return true;
  }

  async travel(
    step: -1 | 1,
    open: (place: Place) => Promise<void>,
    here: () => Place | null,
  ): Promise<void> {
    const target = this.at + step;
    if (target < 0 || target >= this.places.length) return;
    const place = this.places[target];

    this.travelling = true;
    try {
      await open(place);
    } finally {
      this.travelling = false;
    }

    const now = here();
    if (now && samePlace(now, place)) {
      this.at = target;
    } else {
      this.places.splice(target, 1);
      if (target < this.at) this.at -= 1;
    }
  }
}
