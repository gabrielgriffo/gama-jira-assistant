import { Injectable, signal } from "@angular/core";

export interface ToastMessage {
  message: string;
  type: "success" | "error" | "info";
  duration?: number;
}

@Injectable({
  providedIn: "root",
})
export class ToastService {
  private readonly _toasts = signal<readonly ToastMessage[]>([]);
  readonly toasts = this._toasts.asReadonly();

  show(message: string, type: "success" | "error" | "info" = "info", duration = 3000) {
    const toast: ToastMessage = { message, type, duration };
    this._toasts.update((toasts) => [...toasts, toast]);
    setTimeout(() => this.remove(toast), duration);
  }

  success(message: string, duration = 3000) {
    this.show(message, "success", duration);
  }

  error(message: string, duration = 4000) {
    this.show(message, "error", duration);
  }

  private remove(toast: ToastMessage) {
    this._toasts.update((toasts) => toasts.filter((t) => t !== toast));
  }
}
