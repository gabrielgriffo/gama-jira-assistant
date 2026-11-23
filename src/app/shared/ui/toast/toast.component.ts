import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { ToastService } from "../../../core/feedback/toast.service";

@Component({
  selector: "app-toast",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="toast-container">
      @for (toast of toasts(); track toast) {
        <div
          class="toast"
          [class.toast-success]="toast.type === 'success'"
          [class.toast-error]="toast.type === 'error'"
          [class.toast-info]="toast.type === 'info'"
          animate.leave="toast-leave"
        >
          {{ toast.message }}
        </div>
      }
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
        padding: 8px 12px;
        border-radius: 8px;
        background: var(--inverse);
        color: var(--on-inverse);
        box-shadow: var(--shadow-pop);
        transition:
          opacity 180ms var(--ease-out),
          transform 180ms var(--ease-out);
        pointer-events: auto;
        font-size: 13px;
      }

      @starting-style {
        .toast {
          opacity: 0;
          transform: translateY(6px);
        }
      }

      .toast-error {
        background: var(--err-soft);
        color: var(--err);
        font-weight: 500;
      }

      .toast.toast-leave {
        opacity: 0;
        transform: translateY(6px);
        transition-duration: 150ms;
        pointer-events: none;
      }

      @media (prefers-reduced-motion: reduce) {
        .toast {
          transition-property: opacity;
        }
        .toast.toast-leave {
          transform: none;
        }
      }
    `,
  ],
})
export class ToastComponent {
  readonly toasts = inject(ToastService).toasts;
}
