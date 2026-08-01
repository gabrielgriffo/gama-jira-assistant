import { Component } from "@angular/core";
import { CommonModule } from "@angular/common";
import { ToastService, ToastMessage } from "../../../core/feedback/toast.service";

@Component({
  selector: "app-toast",
  imports: [CommonModule],
  template: `
    <div class="toast-container">
      <div
        *ngFor="let toast of toasts$ | async"
        class="toast"
        [class.toast-success]="toast.type === 'success'"
        [class.toast-error]="toast.type === 'error'"
        [class.toast-info]="toast.type === 'info'"
      >
        {{ toast.message }}
      </div>
    </div>
  `,
  styles: [
    `
      .toast-container {
        position: fixed;
        right: 16px;
        bottom: calc(env(safe-area-inset-bottom, 0px) + 16px);
        z-index: 9999;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 8px;
        pointer-events: none;
      }

      .toast {
        max-width: 380px;
        padding: 12px 18px;
        border-radius: 16px;
        background: var(--primary);
        color: var(--on-primary);
        box-shadow: var(--shadow-pop);
        animation: rise 0.18s ease-out;
        pointer-events: auto;
      }

      .toast-error {
        background: var(--err-soft);
        color: var(--err);
        font-weight: 500;
      }

      @keyframes rise {
        from {
          opacity: 0;
          transform: translateY(6px);
        }
      }

      @media (prefers-reduced-motion: reduce) {
        .toast {
          animation: none;
        }
      }
    `,
  ],
})
export class ToastComponent {
  toasts$;

  constructor(private toastService: ToastService) {
    this.toasts$ = toastService.toasts$;
  }
}
