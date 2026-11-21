import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";

export interface ToastMessage {
  message: string;
  type: "success" | "error" | "info";
  duration?: number;
}

@Injectable({
  providedIn: "root",
})
export class ToastService {
  private toasts: ToastMessage[] = [];
  public toasts$ = new BehaviorSubject<ToastMessage[]>([]);

  show(message: string, type: "success" | "error" | "info" = "info", duration = 3000) {
    const toast: ToastMessage = { message, type, duration };
    this.toasts.push(toast);
    this.toasts$.next([...this.toasts]);

    setTimeout(() => {
      this.remove(toast);
    }, duration);
  }

  success(message: string, duration = 3000) {
    this.show(message, "success", duration);
  }

  error(message: string, duration = 4000) {
    this.show(message, "error", duration);
  }

  private remove(toast: ToastMessage) {
    this.toasts = this.toasts.filter((t) => t !== toast);
    this.toasts$.next([...this.toasts]);
  }
}
