import type {
  AccountState,
  NetworkStatus,
  SubmissionResult,
} from '@platform/api-client';
import { formatAmount } from '@platform/money';
import type { PayrailWallet } from '@platform/wallet-core';
import { attr, html, on } from 'workstar';
import { balanceCard, type BalanceCardState } from './wallet-balance-card';
import { formatAtomic, short } from './wallet-view-utils';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

interface PendingTransfer {
  readonly recipient: string;
  readonly amount: string;
}

interface SendScreenState extends BalanceCardState {
  readonly status: ReadonlySignal<NetworkStatus | null>;
  readonly wallet: ReadonlySignal<PayrailWallet | null>;
  readonly account: ReadonlySignal<AccountState | null>;
  readonly receipt: ReadonlySignal<SubmissionResult | null>;
  readonly pendingTransfer: ReadonlySignal<PendingTransfer | null>;
  readonly busy: ReadonlySignal<boolean>;
  prepareSend(event: Event): Promise<void>;
  confirmSend(): Promise<void>;
  cancelSend(): void;
}

export function sendScreen(state: SendScreenState) {
  return html`<div class="focused-layout send-layout">
    <section class="focused-copy">
      <p class="eyebrow">Signed transfer</p>
      <h1>Send with certainty.</h1>
      <p>
        The exact recipient, amount, network and nonce are signed locally.
        Payrail returns a deterministic final receipt.
      </p>
      ${balanceCard(state)}
    </section>
    <article class="send-panel">
      ${paymentSteps(state)}
      <p class="eyebrow">New payment</p>
      <h2>Send ${() => state.status.value?.asset.symbol ?? 'TEST'}</h2>
      <form
        data-testid="send-form"
        ${on('submit', (event) => void state.prepareSend(event))}
      >
        <label
          >Recipient<input
            name="recipient"
            data-testid="recipient"
            placeholder="paydev1…"
            autocomplete="off"
            spellcheck="false"
            required
            ${attr('disabled', state.busy)}
        /></label>
        <label
          >Amount<input
            name="amount"
            data-testid="amount"
            placeholder="0.00"
            inputmode="decimal"
            autocomplete="off"
            required
            ${attr('disabled', state.busy)}
        /></label>
        <div class="amount-presets" aria-label="Amount shortcuts">
          ${amountButton(state, '25%', 4, 'amount-quarter')}
          ${amountButton(state, '50%', 2, 'amount-half')}
          ${amountButton(state, 'Max', 1, 'amount-max')}
        </div>
        <div class="payment-summary">
          <span>Available</span
          ><strong
            >${() =>
              state.balanceHidden.value
                ? '••••'
                : formatAtomic(
                    state.account.value?.balance ?? '0',
                    state.status.value,
                  )}</strong
          >
          <span>Network fee</span
          ><strong
            >0.00 ${() => state.status.value?.asset.symbol ?? 'TEST'}</strong
          >
        </div>
        <button
          class="primary"
          data-testid="send-payment"
          type="submit"
          ${attr('disabled', () => state.busy.value || !state.account.value)}
        >
          Review payment
        </button>
      </form>
      ${() =>
        state.pendingTransfer.value
          ? transferReview(state, state.pendingTransfer.value)
          : null}
      ${() => (state.receipt.value ? receipt(state.receipt.value) : null)}
    </article>
  </div>`;
}

function paymentSteps(state: SendScreenState) {
  return html`<ol class="payment-steps" aria-label="Payment progress">
    <li data-active="true"><b>1</b><span>Details</span></li>
    <li
      ${attr('data-active', () =>
        String(Boolean(state.pendingTransfer.value || state.receipt.value)),
      )}
    >
      <b>2</b><span>Verify</span>
    </li>
    <li ${attr('data-active', () => String(Boolean(state.receipt.value)))}>
      <b>3</b><span>Finality</span>
    </li>
  </ol>`;
}

function amountButton(
  state: SendScreenState,
  label: string,
  divisor: number,
  testId: string,
) {
  return html`<button
    type="button"
    ${attr('data-testid', testId)}
    ${attr('disabled', () => state.busy.value || !state.account.value)}
    ${on('click', (event) => setAmount(event, state, divisor))}
  >
    ${label}
  </button>`;
}

function setAmount(event: Event, state: SendScreenState, divisor: number) {
  const button = event.currentTarget;
  if (!(button instanceof HTMLButtonElement)) return;
  const input = button.form?.elements.namedItem('amount');
  if (!(input instanceof HTMLInputElement) || !state.status.value) return;
  const balance = BigInt(state.account.value?.balance ?? '0');
  const amount = balance / BigInt(divisor);
  input.value = formatAmount(
    amount.toString(),
    state.status.value.asset.decimals,
  );
  input.focus();
}

function transferReview(state: SendScreenState, pending: PendingTransfer) {
  return html`<section class="transfer-review" data-testid="transfer-review">
    <div class="review-heading">
      <p class="eyebrow">Confirm signed payment</p>
      <span>Not signed yet</span>
    </div>
    <dl>
      <div class="recipient-review">
        <dt>Recipient</dt>
        <dd>${pending.recipient}</dd>
      </div>
      <div>
        <dt>Amount</dt>
        <dd>${formatAtomic(pending.amount, state.status.value)}</dd>
      </div>
      <div>
        <dt>Balance after</dt>
        <dd>${remainingBalance(state, pending.amount)}</dd>
      </div>
      <div>
        <dt>Network fee</dt>
        <dd>0 ${state.status.value?.asset.symbol ?? 'TEST'}</dd>
      </div>
    </dl>
    <p>The private key stays on this device. Only this operation is signed.</p>
    <div class="button-row">
      <button class="secondary" type="button" ${on('click', state.cancelSend)}>
        Edit details</button
      ><button
        class="primary"
        data-testid="confirm-transfer"
        type="button"
        ${attr('disabled', state.busy)}
        ${on('click', () => void state.confirmSend())}
      >
        Sign & finalize
      </button>
    </div>
  </section>`;
}

function remainingBalance(state: SendScreenState, amount: string) {
  const balance = BigInt(state.account.value?.balance ?? '0');
  const debit = BigInt(amount);
  const remaining = balance >= debit ? balance - debit : 0n;
  return formatAtomic(remaining.toString(), state.status.value);
}

function receipt(result: SubmissionResult) {
  return html`<div class="receipt" data-testid="receipt">
    <i>✓</i
    ><span
      ><strong>Payment finalized</strong
      ><small>Block #${result.checkpoint.height}</small></span
    ><code>${short(result.transaction.id)}</code
    ><a
      ${attr(
        'href',
        `https://explorer.payrail.one/?tx=${result.transaction.id}`,
      )}
      >View proof ↗</a
    >
  </div>`;
}
