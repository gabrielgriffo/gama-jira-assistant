import type { UnlistenFn } from "@tauri-apps/api/event";

export class ListenerBag {
  private readonly unlisteners: UnlistenFn[] = [];
  private disposed = false;

  async add(listen: Promise<UnlistenFn>): Promise<void> {
    const unlisten = await listen;
    if (this.disposed) {
      unlisten();
      return;
    }
    this.unlisteners.push(unlisten);
  }

  get isDisposed(): boolean {
    return this.disposed;
  }

  dispose(): void {
    this.disposed = true;
    this.unlisteners.splice(0).forEach((unlisten) => unlisten());
  }
}
