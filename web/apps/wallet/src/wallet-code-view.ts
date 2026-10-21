import type { IssuedApprovalCode } from '@platform/api-client';
import { attr, html, on } from 'workstar';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export interface PayrailCodeScreenState {
  readonly approvalCode: ReadonlySignal<IssuedApprovalCode | null>;
  readonly codeSeconds: ReadonlySignal<number>;
  readonly busy: ReadonlySignal<boolean>;
  createPayrailCode(): Promise<void>;
  cancelPayrailCode(): void;
}

export function payrailCodeScreen(state: PayrailCodeScreenState) {
  return html`<div class="focused-layout code-layout">
    <section class="code-intro">
      <p class="eyebrow">Payrail Code · DEVNET</p>
      <h1>Six digits.<br />Your approval.</h1>
      <p>
        Generate a short-lived code, enter it at checkout, then return here to
        review the exact merchant and amount before signing.
      </p>
      <ul>
        <li>The code expires after two minutes</li>
        <li>It can link exactly one checkout</li>
        <li>It cannot move money without your signature</li>
      </ul>
    </section>
    <section class="payrail-code-card">
      ${() =>
        state.approvalCode.value
          ? activeCode(state)
          : html`<div class="code-empty">
              <span class="code-mark">P</span>
              <h2>Ready when you are.</h2>
              <p>Create a one-time code inside this unlocked wallet.</p>
              <button
                class="primary"
                data-testid="create-payrail-code"
                type="button"
                ${attr('disabled', state.busy)}
                ${on('click', () => void state.createPayrailCode())}
              >
                Generate Payrail Code
              </button>
            </div>`}
    </section>
  </div>`;
}

function activeCode(state: PayrailCodeScreenState) {
  return html`<div class="code-active" data-testid="payrail-code-active">
    <div class="code-status"><i></i>Waiting for a checkout</div>
    <div class="code-digits" data-testid="payrail-code">
      ${() =>
        [...(state.approvalCode.value?.code ?? '')].map(
          (digit) => html`<b>${digit}</b>`,
        )}
    </div>
    <div class="code-expiry">
      <span>Expires in</span
      ><strong>${() => formatCountdown(state.codeSeconds.value)}</strong>
    </div>
    <p>
      Enter these digits only in a Payrail checkout. The payment details will
      appear here before anything is signed.
    </p>
    <div class="button-row">
      <button
        type="button"
        ${on(
          'click',
          () =>
            void navigator.clipboard.writeText(
              state.approvalCode.value?.code ?? '',
            ),
        )}
      >
        Copy code</button
      ><button
        class="secondary"
        type="button"
        ${on('click', state.cancelPayrailCode)}
      >
        Cancel
      </button>
    </div>
  </div>`;
}

function formatCountdown(seconds: number): string {
  const bounded = Math.max(0, seconds);
  return `${Math.floor(bounded / 60)}:${String(bounded % 60).padStart(2, '0')}`;
}
