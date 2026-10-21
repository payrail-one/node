import type { AccountState, Checkout } from '@platform/api-client';
import { formatAmount } from '@platform/money';
import { attr, html, on } from 'workstar';
import { walletIcon } from './wallet-icons';
import { qrCanvas, short } from './wallet-view-utils';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export interface RequestScreenState {
  readonly checkout: ReadonlySignal<Checkout | null>;
  readonly account: ReadonlySignal<AccountState | null>;
  readonly checkoutReviewed: ReadonlySignal<boolean>;
  readonly busy: ReadonlySignal<boolean>;
  createCheckout(event: Event): Promise<void>;
  copySms(): Promise<void>;
  fund(): Promise<void>;
  reviewCheckout(): void;
  cancelCheckout(): void;
  confirmCheckout(): Promise<void>;
}

export function requestScreen(state: RequestScreenState) {
  return html`<div class="request-layout">
      <section class="focused-copy">
        <p class="eyebrow">Omnichannel requests</p>
        <h1>One request.<br /><em>Every channel.</em></h1>
        <p>
          Create one immutable payment request for hosted web, QR and SMS. Each
          channel resolves to the same recipient and amount.
        </p>
        <div class="channel-preview" aria-label="Payment request channels">
          <div>
            <i>${walletIcon('link')}</i
            ><span
              ><strong>Hosted link</strong><small>Share anywhere</small></span
            >
          </div>
          <div>
            <i>${walletIcon('qr')}</i
            ><span
              ><strong>QR checkout</strong><small>Scan in person</small></span
            >
          </div>
          <div>
            <i>${walletIcon('sms')}</i
            ><span
              ><strong>Text message</strong
              ><small>Feature-phone ready</small></span
            >
          </div>
        </div>
      </section>
      <form
        class="request-builder"
        data-testid="request-form"
        ${on('submit', (event) => void state.createCheckout(event))}
      >
        <p class="eyebrow">Payment link</p>
        <h2>Create request</h2>
        <label
          >Amount<input
            name="checkout-amount"
            data-testid="request-amount"
            inputmode="decimal"
            placeholder="12.50"
            required /></label
        ><label
          >Order reference<input
            name="order-reference"
            data-testid="request-reference"
            maxlength="64"
            pattern="[A-Za-z0-9_]+"
            placeholder="invoice_1042"
            required /></label
        ><button
          class="primary"
          data-testid="create-checkout"
          type="submit"
          ${attr('disabled', state.busy)}
        >
          Create secure request
        </button>
      </form>
    </div>
    ${() =>
      state.checkout.value
        ? checkoutChannels(state.checkout.value, state.copySms)
        : null}`;
}

function checkoutChannels(checkout: Checkout, copySms: () => Promise<void>) {
  const url = `https://wallet.payrail.one${checkout.paymentPath}`;
  const sms = checkout.smsText.replace(
    '{origin}',
    'https://wallet.payrail.one',
  );
  return html`<section class="channel-stage" data-testid="checkout-created">
    <article class="qr-card">
      <div>
        <p class="eyebrow">Scan to pay</p>
        <h2>${checkout.merchantLabel}</h2>
      </div>
      <div class="qr-frame">${qrCanvas(url)}</div>
      <strong
        >${formatAmount(checkout.amount, checkout.asset.decimals)}
        ${checkout.asset.symbol}</strong
      ><a
        data-testid="checkout-link"
        ${attr('href', url)}
        target="_blank"
        rel="noopener"
        >Open secure payment</a
      >
    </article>
    <article class="channel-card">
      <p class="eyebrow">Hosted checkout</p>
      <h2>Share one verified link</h2>
      <code>${url}</code
      ><button
        type="button"
        ${on('click', () => void navigator.clipboard.writeText(url))}
      >
        Copy payment link
      </button>
    </article>
    <article class="channel-card sms-card">
      <p class="eyebrow">SMS delivery</p>
      <h2>Ready for feature phones</h2>
      <p data-testid="sms-preview">${sms}</p>
      <div class="sms-actions">
        <button
          data-testid="sms-compose"
          type="button"
          ${on('click', () => openSmsComposer(sms))}
        >
          Open SMS app</button
        ><button type="button" ${on('click', () => void copySms())}>
          Copy message
        </button>
      </div>
    </article>
  </section>`;
}

function openSmsComposer(message: string) {
  window.location.assign(`sms:?body=${encodeURIComponent(message)}`);
}

export function checkoutScreen(state: RequestScreenState) {
  const checkout = state.checkout.value;
  if (!checkout)
    return html`<p class="empty-state">Loading verified payment request…</p>`;
  const finalized = checkout.status === 'finalized';
  return html`<section class="checkout-page">
    <article class="payment-sheet" data-testid="hosted-checkout">
      <div class="sheet-head">
        <div>
          <p class="eyebrow">Secure Payrail checkout</p>
          <h2>${checkout.merchantLabel}</h2>
        </div>
        <span>${checkout.status}</span>
      </div>
      <div class="checkout-amount">
        <small>Total</small
        ><strong
          >${formatAmount(checkout.amount, checkout.asset.decimals)}
          ${checkout.asset.symbol}</strong
        >
      </div>
      <dl>
        <div>
          <dt>Network fee</dt>
          <dd>0 ${checkout.asset.symbol}</dd>
        </div>
        <div>
          <dt>Network</dt>
          <dd>Payrail Devnet</dd>
        </div>
        <div>
          <dt>Recipient</dt>
          <dd>${short(checkout.merchantAddress)}</dd>
        </div>
        <div>
          <dt>Request</dt>
          <dd>${short(checkout.id)}</dd>
        </div>
      </dl>
      ${finalized
        ? html`<div class="checkout-success" data-testid="checkout-finalized">
            <i>✓</i
            ><span
              ><strong>Payment finalized</strong
              ><small>${short(checkout.transaction?.id ?? '')}</small></span
            >
          </div>`
        : checkoutActions(state, checkout)}
    </article>
  </section>`;
}

function checkoutActions(state: RequestScreenState, checkout: Checkout) {
  if (BigInt(state.account.value?.balance ?? '0') < BigInt(checkout.amount)) {
    return html`<div class="checkout-action">
      <p>Add test funds before paying this request.</p>
      <button
        class="primary"
        type="button"
        ${attr('disabled', state.busy)}
        ${on('click', () => void state.fund())}
      >
        Add test funds
      </button>
    </div>`;
  }
  if (!state.checkoutReviewed.value) {
    return html`<button
      class="primary full-button"
      data-testid="review-checkout"
      type="button"
      ${on('click', state.reviewCheckout)}
    >
      Review payment
    </button>`;
  }
  return html`<div class="checkout-action">
    <p>
      You are signing this exact amount and recipient. The private key remains
      on this device.
    </p>
    <div class="button-row">
      <button
        class="secondary"
        type="button"
        ${on('click', state.cancelCheckout)}
      >
        Cancel</button
      ><button
        class="primary"
        data-testid="confirm-checkout"
        type="button"
        ${attr('disabled', state.busy)}
        ${on('click', () => void state.confirmCheckout())}
      >
        Confirm & pay
      </button>
    </div>
  </div>`;
}
