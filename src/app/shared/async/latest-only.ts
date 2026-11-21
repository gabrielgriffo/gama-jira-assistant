export class LatestOnly {
  private seq = 0;

  begin(): () => boolean {
    const mine = ++this.seq;
    return () => mine === this.seq;
  }
}
