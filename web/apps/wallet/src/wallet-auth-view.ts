import type { NetworkStatus } from '@platform/api-client';
import type { EncryptedWalletVault } from '@platform/wallet-core';
import { attr, html, on } from 'workstar';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export function authScreen(state: {
  vault: ReadonlySignal<EncryptedWalletVault | null>;
  status: ReadonlySignal<NetworkStatus | null>;
  message: ReadonlySignal<string>;
  busy: ReadonlySignal<boolean>;
  createWallet(event: Event): Promise<void>;
  unlock(event: Event): Promise<void>;
  importVault(event: Event): Promise<void>;
}) {
  const submit = (event: Event) =>
    void (state.vault.value ? state.unlock(event) : state.createWallet(event));
  return html`<section class="auth-layout">
    <div class="hero">
      <p class="eyebrow">Payrail self-custody</p>
      <h1>Your money.<br /><em>Your signature.</em></h1>
      <p>Keys are encrypted on this device and never sent to Payrail.</p>
      <div class="trust-row">
        <span>600K PBKDF2 rounds</span><span>AES-256-GCM</span
        ><span>Client-side Ed25519</span>
      </div>
      <div class="trust-flow" aria-label="Payrail security model">
        <div>
          <b>01</b
          ><span
            ><strong>Created here</strong
            ><small>Your key begins on this device</small></span
          >
        </div>
        <i aria-hidden="true"></i>
        <div>
          <b>02</b
          ><span
            ><strong>Signed locally</strong
            ><small>Only the operation leaves</small></span
          >
        </div>
        <i aria-hidden="true"></i>
        <div>
          <b>03</b
          ><span
            ><strong>Finalized onchain</strong
            ><small>Independently verifiable</small></span
          >
        </div>
      </div>
    </div>
    <article class="auth-card">
      <p class="eyebrow">
        ${() => (state.vault.value ? 'Welcome back' : 'New wallet')}
      </p>
      <h2>${() => (state.vault.value ? 'Unlock wallet' : 'Create wallet')}</h2>
      <form data-testid="auth-form" ${on('submit', submit)}>
        <label
          >Password<input
            name="password"
            data-testid="password"
            type="password"
            minlength="10"
            maxlength="256"
            autocomplete="current-password"
            required
        /></label>
        ${() =>
          state.vault.value
            ? null
            : html`<label
                >Confirm password<input
                  name="confirmation"
                  data-testid="password-confirmation"
                  type="password"
                  minlength="10"
                  maxlength="256"
                  autocomplete="new-password"
                  required
              /></label>`}
        <button
          class="primary"
          data-testid="authorize-wallet"
          type="submit"
          ${attr('disabled', state.busy)}
        >
          ${() =>
            state.vault.value ? 'Unlock securely' : 'Create encrypted wallet'}
        </button>
      </form>
      <p class="status" role="status">${state.message}</p>
      <div class="restore-intro">
        <span></span><small>New device or existing wallet?</small><span></span>
      </div>
      <label class="restore-vault">
        <strong>Restore encrypted backup</strong>
        <small
          >Import your JSON vault, then unlock with the same password.</small
        >
        <input
          data-testid="import-wallet"
          type="file"
          accept="application/json,.json"
          ${attr('disabled', state.busy)}
          ${on('change', (event) => void state.importVault(event))}
        />
      </label>
    </article>
  </section>`;
}
