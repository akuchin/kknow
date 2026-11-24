export class SaveQueue {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private chain: Promise<void> = Promise.resolve();
  private readonly write: (text: string) => Promise<void>;
  private readonly delay: number;

  constructor(write: (text: string) => Promise<void>, delay: number) {
    this.write = write;
    this.delay = delay;
  }

  get pending(): boolean {
    return this.timer !== undefined;
  }

  schedule(text: string): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.save(text), this.delay);
  }

  save(text: string): Promise<void> {
    this.cancel();
    this.chain = this.chain.then(() => this.write(text));
    return this.chain;
  }

  flush(text: () => string): Promise<void> {
    return this.pending ? this.save(text()) : this.chain;
  }

  cancel(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
  }
}
