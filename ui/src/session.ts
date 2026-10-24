/**
 * Autosave of the open note: a delayed write and a queue of writes.
 *
 * Before, `main.ts` kept the timer and the promise chain as two module
 * variables, and any handler could change them. Now only this class does.
 *
 * - An edit arms the timer (`schedule`). The write comes `delay` ms after the
 *   last key press.
 * - Writes run strictly one at a time. Each one needs the previous result as
 *   `base`; else the second write would treat the first one as an external
 *   edit.
 * - `flush` writes a pending edit at once. The window calls it before it
 *   leaves the note, loses focus or closes.
 * - `cancel` drops a pending edit: the note was deleted, and a write would
 *   create the file again.
 *
 * The write itself (`write`) stays in `main.ts`: it knows the open note, the
 * editor and the conflict copy. This module has no imports, so the tests run
 * it in Node.
 */
export class SaveQueue {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private chain: Promise<void> = Promise.resolve();
  private readonly write: (text: string) => Promise<void>;
  private readonly delay: number;

  constructor(write: (text: string) => Promise<void>, delay: number) {
    this.write = write;
    this.delay = delay;
  }

  /** An edit is not on disk yet. */
  get pending(): boolean {
    return this.timer !== undefined;
  }

  /** An edit: write `text` after the delay, unless another edit comes first. */
  schedule(text: string): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.save(text), this.delay);
  }

  /** Writes `text` after the writes before it. */
  save(text: string): Promise<void> {
    // The write starts, so no edit is pending. Before this rule the timer
    // stayed armed, and the window always saw the note as unsaved: it ignored
    // external edits, and the next save erased them.
    this.cancel();
    this.chain = this.chain.then(() => this.write(text));
    return this.chain;
  }

  /**
   * Writes a pending edit at once. Without one, waits for the write in
   * progress instead of leaving in the middle of it.
   */
  flush(text: () => string): Promise<void> {
    return this.pending ? this.save(text()) : this.chain;
  }

  /** Drops a pending edit. A write in progress still ends. */
  cancel(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
  }
}
