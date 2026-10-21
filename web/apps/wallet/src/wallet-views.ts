import type {
  AccountState,
  Checkout,
  ExplorerOverview,
  NetworkStatus,
  SubmissionResult,
} from '@platform/api-client';
import type { PayrailWallet } from '@platform/wallet-core';
import { attr, html, on, repeat } from 'workstar';
import { balanceCard } from './wallet-balance-card';
import {
  payrailCodeScreen,
  type PayrailCodeScreenState,
} from './wallet-code-view';
import { walletIcon } from './wallet-icons';
import { checkoutScreen, requestScreen } from './wallet-request-view';
import { sendScreen } from './wallet-send-view';
import { bridgeScreen } from './wallet-bridge-view';
import type { BridgeController } from './wallet-bridge-controller';
import {
  settingsScreen,
  type SettingsScreenState,
} from './wallet-settings-view';
import { finalityLabel, formatAtomic, short } from './wallet-view-utils';

export type WalletView =
  | 'overview'
  | 'send'
  | 'request'
  | 'bridge'
  | 'code'
  | 'settings'
  | 'checkout';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

interface PendingTransfer {
  readonly recipient: string;
  readonly amount: string;
}

interface WalletScreenState
  extends SettingsScreenState, PayrailCodeScreenState {
  readonly status: ReadonlySignal<NetworkStatus | null>;
  readonly wallet: ReadonlySignal<PayrailWallet | null>;
  readonly account: ReadonlySignal<AccountState | null>;
  readonly receipt: ReadonlySignal<SubmissionResult | null>;
  readonly pendingTransfer: ReadonlySignal<PendingTransfer | null>;
  readonly overview: ReadonlySignal<ExplorerOverview | null>;
  readonly checkout: ReadonlySignal<Checkout | null>;
  readonly checkoutReviewed: ReadonlySignal<boolean>;
  readonly view: ReadonlySignal<WalletView>;
  readonly message: ReadonlySignal<string>;
  readonly busy: ReadonlySignal<boolean>;
  readonly balanceHidden: ReadonlySignal<boolean>;
  readonly bridge: BridgeController;
  selectView(view: WalletView): void;
  refresh(): Promise<void>;
  fund(): Promise<void>;
  prepareSend(event: Event): Promise<void>;
  confirmSend(): Promise<void>;
  cancelSend(): void;
  createCheckout(event: Event): Promise<void>;
  copySms(): Promise<void>;
  reviewCheckout(): void;
  cancelCheckout(): void;
  confirmCheckout(): Promise<void>;
  lock(): Promise<void>;
  backup(): void;
  toggleBalancePrivacy(): void;
}

export function walletScreen(state: WalletScreenState) {
  return html`<section class="wallet-home">
    <div class="wallet-toolbar">
      <div class="wallet-tabs" role="navigation" aria-label="Wallet">
        ${tab(state, 'overview', 'Overview')}${tab(state, 'send', 'Send')}${tab(
          state,
          'request',
          'Request',
        )}${tab(state, 'bridge', 'Bridge')}${tab(
          state,
          'code',
          'Payrail Code',
        )}${tab(state, 'settings', 'Settings')}
      </div>
      <div class="session-actions">
        <button
          data-testid="backup-wallet"
          type="button"
          ${on('click', state.backup)}
        >
          Backup
        </button>
        <button
          data-testid="lock-wallet"
          type="button"
          ${on('click', () => void state.lock())}
        >
          Lock
        </button>
      </div>
    </div>
    ${() => screen(state)}
    <footer>
      <span>${state.message}</span
      ><span
        >Encrypted locally · auto-lock
        ${() => state.preferences.value.autoLockMinutes} min</span
      >
    </footer>
  </section>`;
}

function tab(state: WalletScreenState, view: WalletView, label: string) {
  return html`<button
    type="button"
    ${attr('data-testid', `${view}-tab`)}
    ${attr('data-active', () => String(state.view.value === view))}
    ${on('click', () => state.selectView(view))}
  >
    ${label}
  </button>`;
}

function screen(state: WalletScreenState) {
  switch (state.view.value) {
    case 'send':
      return sendScreen(state);
    case 'request':
      return requestScreen(state);
    case 'code':
      return payrailCodeScreen(state);
    case 'bridge':
      return bridgeScreen(state);
    case 'checkout':
      return checkoutScreen(state);
    case 'settings':
      return settingsScreen(state);
    default:
      return overviewScreen(state);
  }
}

function overviewScreen(state: WalletScreenState) {
  return html`<div class="wallet-dashboard">
    <section class="wallet-heading">
      <div>
        <p class="eyebrow">Payrail private wallet</p>
        <h1>Money in motion.</h1>
        <p class="heading-note">
          A self-custody account with bank-grade clarity and verifiable
          settlement.
        </p>
      </div>
      <span class="finality-badge"
        ><i></i>Finalized
        #${() => state.status.value?.finalizedHeight ?? '—'}</span
      >
    </section>
    <div class="wallet-grid">
      ${balanceCard(state)}
      <article class="controls account-panel">
        <div class="panel-title">
          <div>
            <p class="eyebrow">Account</p>
            <h2>Self-custody controls</h2>
          </div>
          <span class="risk-pill">Protected</span>
        </div>
        <div class="address-row">
          <div class="address-identity">
            <span>Public address</span>
            <code
              data-testid="wallet-address"
              ${attr('data-address', () => state.wallet.value?.address ?? '')}
              ${attr('title', () => state.wallet.value?.address ?? '')}
              >${() => state.wallet.value?.address ?? ''}</code
            >
          </div>
          <button
            type="button"
            ${on(
              'click',
              () =>
                void navigator.clipboard.writeText(
                  state.wallet.value?.address ?? '',
                ),
            )}
          >
            Copy full
          </button>
        </div>
        <dl class="account-facts">
          <div>
            <dt>Nonce</dt>
            <dd>${() => state.account.value?.nonce ?? '—'}</dd>
          </div>
          <div>
            <dt>Network</dt>
            <dd ${attr('title', () => state.status.value?.networkId ?? '')}>
              Payrail Devnet
            </dd>
          </div>
          <div>
            <dt>Signing</dt>
            <dd>On-device</dd>
          </div>
        </dl>
        <div class="button-row">
          <button
            class="primary"
            data-testid="fund-wallet"
            type="button"
            ${attr('disabled', state.busy)}
            ${on('click', () => void state.fund())}
          >
            Add test funds</button
          ><button
            class="secondary"
            type="button"
            ${attr('disabled', state.busy)}
            ${on('click', () => void state.refresh())}
          >
            Refresh state
          </button>
        </div>
      </article>
    </div>
    ${portfolioMetrics(state)}
    <section class="quick-actions">
      <button
        data-testid="payrail-code-action"
        type="button"
        ${on('click', () => state.selectView('code'))}
      >
        <i>6</i
        ><span
          ><strong>Payrail Code</strong
          ><small>Six digits · two minutes</small></span
        >
      </button>
      <button type="button" ${on('click', () => state.selectView('send'))}>
        <i>${walletIcon('send')}</i
        ><span
          ><strong>Send payment</strong><small>Sign and finalize</small></span
        >
      </button>
      <button type="button" ${on('click', () => state.selectView('request'))}>
        <i>${walletIcon('request')}</i
        ><span
          ><strong>Request money</strong><small>Web, QR and SMS</small></span
        >
      </button>
      <button type="button" ${on('click', () => state.selectView('bridge'))}>
        <i>Ξ</i
        ><span
          ><strong>Bridge tUSDT</strong><small>Sepolia → Payrail</small></span
        >
      </button>
      <a href="https://explorer.payrail.one"
        ><i>${walletIcon('explorer')}</i
        ><span
          ><strong>Open Explorer</strong
          ><small>Independent finality</small></span
        ></a
      >
    </section>
    <div class="dashboard-lower">
      ${activity(state)} ${networkAssurance(state)}
    </div>
  </div>`;
}

function portfolioMetrics(state: WalletScreenState) {
  return html`<section
    class="portfolio-metrics"
    aria-label="Recent account insights"
  >
    <div>
      <span>Recent inflow</span
      ><strong
        >${() =>
          state.balanceHidden.value
            ? '••••'
            : formatAtomic(
                recentFlow(state).incoming,
                state.status.value,
              )}</strong
      ><small>Indexed activity window</small>
    </div>
    <div>
      <span>Recent outflow</span
      ><strong
        >${() =>
          state.balanceHidden.value
            ? '••••'
            : formatAtomic(
                recentFlow(state).outgoing,
                state.status.value,
              )}</strong
      ><small>Indexed activity window</small>
    </div>
    <div>
      <span>Finalized payments</span
      ><strong
        >${() => recentFlow(state).count.toString().padStart(2, '0')}</strong
      ><small>For this account</small>
    </div>
    <div>
      <span>Settlement</span><strong>Deterministic</strong
      ><small>${() => finalityLabel(state.status.value)}</small>
    </div>
  </section>`;
}

function activity(state: WalletScreenState) {
  return html`<article class="activity-panel">
    <div class="panel-title">
      <div>
        <p class="eyebrow">Ledger activity</p>
        <h2>Recent payments</h2>
      </div>
      <a href="https://explorer.payrail.one">View explorer ↗</a>
    </div>
    <div class="activity-head">
      <span>Payment</span><span>Direction</span><span>Block</span
      ><span>Amount</span>
    </div>
    ${repeat(
      () => walletTransactions(state),
      (transaction) => transaction.id,
      (transaction) => {
        const incoming = () =>
          transaction.value.to === state.wallet.value?.address;
        return html`<a
          class="activity-row"
          ${attr(
            'href',
            () => `https://explorer.payrail.one/?tx=${transaction.value.id}`,
          )}
          ><span class="payment-id"
            ><i>${() => (incoming() ? '↓' : '↗')}</i
            ><code>${() => short(transaction.value.id)}</code></span
          ><span ${attr('data-direction', () => (incoming() ? 'in' : 'out'))}
            >${() => (incoming() ? 'Received' : 'Sent')}</span
          ><span>#${() => transaction.value.blockHeight}</span
          ><strong
            >${() =>
              `${incoming() ? '+' : '−'}${formatAtomic(transaction.value.amount, state.status.value)}`}</strong
          ></a
        >`;
      },
    )}
    ${() =>
      walletTransactions(state).length
        ? null
        : html`<div class="empty-state">
            <i>${walletIcon('send')}</i
            ><strong>Your first payment will appear here</strong
            ><span>Every row links to independent finality evidence.</span>
          </div>`}
  </article>`;
}

function networkAssurance(state: WalletScreenState) {
  return html`<aside class="assurance-panel">
    <div class="assurance-head">
      <span class="live-dot"><i></i>Network live</span
      ><small>Trust, made visible</small>
    </div>
    <h2>Settlement you can prove.</h2>
    <p>
      Payrail separates the private signing boundary from public, independently
      indexed finality.
    </p>
    <dl>
      <div>
        <dt>Latest finalized block</dt>
        <dd>#${() => state.status.value?.finalizedHeight ?? '—'}</dd>
      </div>
      <div>
        <dt>Consensus</dt>
        <dd>${() => finalityLabel(state.status.value)}</dd>
      </div>
      <div>
        <dt>Private key</dt>
        <dd>On this device</dd>
      </div>
    </dl>
    <a href="https://explorer.payrail.one"
      >Inspect the network <span>↗</span></a
    >
  </aside>`;
}

function walletTransactions(state: WalletScreenState) {
  const address = state.wallet.value?.address;
  return (state.overview.value?.transactions ?? [])
    .filter((item) => item.from === address || item.to === address)
    .slice(0, 6);
}

function recentFlow(state: WalletScreenState) {
  const address = state.wallet.value?.address;
  let incoming = 0n;
  let outgoing = 0n;
  const transactions = walletTransactions(state);
  for (const transaction of transactions) {
    const amount = BigInt(transaction.amount);
    if (transaction.to === address) incoming += amount;
    if (transaction.from === address) outgoing += amount;
  }
  return {
    incoming: incoming.toString(),
    outgoing: outgoing.toString(),
    count: transactions.length,
  };
}
