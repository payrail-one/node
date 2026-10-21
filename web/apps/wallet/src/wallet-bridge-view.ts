import type { NetworkStatus } from '@platform/api-client';
import type { PayrailWallet } from '@platform/wallet-core';
import { attr, html, on } from 'workstar';
import {
  SEPOLIA_CHAIN_NAME,
  TEST_USDT_ADDRESS,
  TOKEN_VAULT_ADDRESS,
} from './ethereum-bridge';
import type { BridgeController } from './wallet-bridge-controller';

interface BridgeScreenState {
  readonly wallet: { readonly value: PayrailWallet | null };
  readonly status: { readonly value: NetworkStatus | null };
  readonly bridge: BridgeController;
}

export function bridgeScreen(state: BridgeScreenState) {
  return html`<div class="bridge-layout">
    <section class="wallet-heading bridge-heading">
      <div>
        <p class="eyebrow">Ethereum bridge · testnet</p>
        <h1>Bring tUSDT to Payrail.</h1>
        <p class="heading-note">
          Deposit test USDT from Sepolia. MetaMask signs on Ethereum; your
          Payrail key never leaves this browser.
        </p>
      </div>
      <span class="bridge-network"><i>Ξ</i>${SEPOLIA_CHAIN_NAME}</span>
    </section>

    <section class="bridge-route" aria-label="Bridge route">
      ${routeCard(
        'From',
        'Ethereum Sepolia',
        () => state.bridge.ethereumAddress.value,
        () =>
          formatAtomic(
            state.bridge.ethereumBalance.value,
            state.status.value?.bridgeAsset?.decimals ?? 6,
            'tUSDT',
          ),
      )}
      <div class="bridge-arrow" aria-hidden="true">→</div>
      ${routeCard(
        'To',
        'Payrail Devnet',
        () => state.wallet.value?.address ?? null,
        () =>
          formatAtomic(
            state.bridge.payrailBalance.value?.balance ?? '0',
            state.status.value?.bridgeAsset?.decimals ?? 6,
            state.status.value?.bridgeAsset?.symbol ?? 'TUSDT',
          ),
      )}
    </section>

    <div class="bridge-grid">
      <article class="controls bridge-action-panel">
        <div class="panel-title">
          <div>
            <p class="eyebrow">Deposit</p>
            <h2>Sepolia → Payrail</h2>
          </div>
          <span class="risk-pill">Inbound</span>
        </div>
        ${() =>
          state.bridge.ethereumAddress.value
            ? depositForm(state)
            : connectPanel(state)}
      </article>
      <aside class="bridge-status-panel">
        <p class="eyebrow">Settlement status</p>
        <ol>
          ${step(state, 'approving', 'Approve tUSDT')}
          ${step(state, 'depositing', 'Deposit to vault')}
          ${step(state, 'waiting', 'Wait for Ethereum finality')}
          ${step(state, 'complete', 'Credit Payrail balance')}
        </ol>
        ${() => transactionLink(state.bridge.transactionHash.value)}
        <dl>
          <div>
            <dt>Token</dt>
            <dd ${attr('title', TEST_USDT_ADDRESS)}>
              ${short(TEST_USDT_ADDRESS)}
            </dd>
          </div>
          <div>
            <dt>Vault</dt>
            <dd ${attr('title', TOKEN_VAULT_ADDRESS)}>
              ${short(TOKEN_VAULT_ADDRESS)}
            </dd>
          </div>
        </dl>
      </aside>
    </div>
    <p class="bridge-warning">
      tUSDT is an unbacked Sepolia test token, not Tether USD₮. This testnet
      screen supports deposits only; mainnet and withdrawals remain disabled.
    </p>
  </div>`;
}

function connectPanel(state: BridgeScreenState) {
  return html`<div class="bridge-connect">
    <div class="metamask-mark">Ξ</div>
    <h3>Connect MetaMask</h3>
    <p>
      The wallet will request the Sepolia network and two confirmations:
      approval, then deposit.
    </p>
    <button
      class="primary"
      data-testid="connect-ethereum"
      type="button"
      ${attr('disabled', state.bridge.busy)}
      ${on('click', () => void state.bridge.connect())}
    >
      ${() =>
        state.bridge.stage.value === 'connecting'
          ? 'Connecting…'
          : state.bridge.providerAvailable
            ? 'Connect MetaMask'
            : 'Install MetaMask'}
    </button>
  </div>`;
}

function depositForm(state: BridgeScreenState) {
  return html`<form class="bridge-form" ${on('submit', state.bridge.deposit)}>
    <label for="bridge-amount">Amount</label>
    <div class="bridge-amount-input">
      <input
        id="bridge-amount"
        name="bridge-amount"
        data-testid="bridge-amount"
        inputmode="decimal"
        autocomplete="off"
        placeholder="1.00"
        required
      />
      <strong>tUSDT</strong>
    </div>
    <p>
      Destination
      <code ${attr('title', () => state.wallet.value?.address ?? '')}
        >${() => short(state.wallet.value?.address ?? '')}</code
      >
    </p>
    <div class="button-row">
      <button
        class="primary"
        data-testid="submit-bridge-deposit"
        type="submit"
        ${attr('disabled', state.bridge.busy)}
      >
        ${() => actionLabel(state.bridge.stage.value)}
      </button>
      <button
        class="secondary"
        type="button"
        ${attr('disabled', state.bridge.busy)}
        ${on('click', () => void state.bridge.refresh())}
      >
        Refresh
      </button>
    </div>
  </form>`;
}

function routeCard(
  label: string,
  network: string,
  address: () => string | null,
  balance: () => string,
) {
  return html`<article>
    <span>${label}</span>
    <strong>${network}</strong>
    <code ${attr('title', () => address() ?? '')}
      >${() => (address() ? short(address() ?? '') : 'Not connected')}</code
    >
    <b>${balance}</b>
  </article>`;
}

function step(
  state: BridgeScreenState,
  stage: 'approving' | 'depositing' | 'waiting' | 'complete',
  label: string,
) {
  return html`<li
    ${attr('data-state', () => stepState(state.bridge.stage.value, stage))}
  >
    <i></i><span>${label}</span>
  </li>`;
}

function stepState(
  current: string,
  target: string,
): 'idle' | 'active' | 'done' {
  const order = [
    'idle',
    'connecting',
    'approving',
    'depositing',
    'waiting',
    'complete',
  ];
  const currentIndex = order.indexOf(current);
  const targetIndex = order.indexOf(target);
  if (current === target) return 'active';
  return currentIndex > targetIndex ? 'done' : 'idle';
}

function transactionLink(hash: string | null) {
  return hash
    ? html`<a
        class="bridge-transaction"
        ${attr('href', `https://sepolia.etherscan.io/tx/${hash}`)}
        target="_blank"
        rel="noreferrer"
        >View Ethereum transaction ↗</a
      >`
    : null;
}

function actionLabel(stage: string): string {
  if (stage === 'approving') return 'Approving…';
  if (stage === 'depositing') return 'Depositing…';
  if (stage === 'waiting') return 'Waiting for finality…';
  if (stage === 'complete') return 'Deposit more tUSDT';
  return 'Bridge to Payrail';
}

function formatAtomic(value: string, decimals: number, symbol: string): string {
  const amount = BigInt(value);
  const scale = 10n ** BigInt(decimals);
  const whole = amount / scale;
  const fraction = (amount % scale)
    .toString()
    .padStart(decimals, '0')
    .replace(/0+$/, '');
  return `${whole}${fraction ? `.${fraction}` : ''} ${symbol}`;
}

function short(value: string): string {
  return value.length > 18 ? `${value.slice(0, 10)}…${value.slice(-6)}` : value;
}
