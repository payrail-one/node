import type { AccountState, NetworkStatus } from '@platform/api-client';
import type { PayrailWallet } from '@platform/wallet-core';
import { attr, html, on } from 'workstar';
import { formatBalance } from './wallet-view-utils';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export interface BalanceCardState {
  readonly status: ReadonlySignal<NetworkStatus | null>;
  readonly wallet: ReadonlySignal<PayrailWallet | null>;
  readonly account: ReadonlySignal<AccountState | null>;
  readonly balanceHidden: ReadonlySignal<boolean>;
  toggleBalancePrivacy(): void;
}

export function balanceCard(state: BalanceCardState) {
  return html`<article class="balance-card">
    <div class="card-orbit" aria-hidden="true"><i></i><i></i><i></i></div>
    <div class="card-head">
      <span>PAYRAIL</span>
      <div class="card-head-actions">
        <button
          type="button"
          ${attr('aria-pressed', () => String(state.balanceHidden.value))}
          ${attr('aria-label', () =>
            state.balanceHidden.value ? 'Show balance' : 'Hide balance',
          )}
          ${on('click', state.toggleBalancePrivacy)}
        >
          ${() => (state.balanceHidden.value ? 'Show' : 'Hide')}</button
        ><b><i></i>DEVNET</b>
      </div>
    </div>
    <small>Available · finalized</small>
    <strong data-testid="wallet-balance"
      >${() =>
        state.balanceHidden.value
          ? '••••••'
          : formatBalance(state.account.value, state.status.value)}</strong
    >
    <div class="card-address">
      <span>Public account</span>
      <code ${attr('title', () => state.wallet.value?.address ?? '')}
        >${() => state.wallet.value?.address ?? '—'}</code
      >
    </div>
    <div class="card-foot">
      <span>ASSET ${() => state.status.value?.asset.symbol ?? '—'}</span
      ><b>NETWORK FEE 0.00</b>
    </div>
  </article>`;
}
