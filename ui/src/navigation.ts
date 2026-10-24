/**
 * Navigation history: back and forward, as in a browser.
 *
 * A place is a note in its vault or a standalone file. Going back can lead to
 * another vault; the caller makes it active in `open`. While the history
 * walks (`travel`), opens are not recorded: else "back" would add a new step
 * and never move.
 *
 * This module has no imports, so the tests run it in Node.
 */
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

  /** A note was opened. Returns whether the history changed. */
  remember(place: Place): boolean {
    if (this.travelling) return false;
    if (this.at >= 0 && samePlace(this.places[this.at], place)) return false;
    // A new step after "back" drops the forward entries, as in a browser.
    this.places.splice(this.at + 1);
    this.places.push(place);
    if (this.places.length > this.limit) this.places.shift();
    this.at = this.places.length - 1;
    return true;
  }

  /**
   * One step back (-1) or forward (1). `open` shows the place, and `here`
   * tells what is open after it.
   *
   * The place can be gone: the note was deleted or renamed. Then the history
   * removes it instead of getting stuck on it.
   */
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
