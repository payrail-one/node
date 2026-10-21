import type { NetworkStatus } from '@platform/api-client';
import type { PayrailWallet } from '@platform/wallet-core';
import { attr, html, on } from 'workstar';
import { walletIcon, type WalletIcon } from './wallet-icons';
import type { LiveConnectionStatus } from './wallet-live';
import type { AutoLockMinutes, WalletPreferences } from './wallet-preferences';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export interface SettingsScreenState {
  readonly status: ReadonlySignal<NetworkStatus | null>;
  readonly wallet: ReadonlySignal<PayrailWallet | null>;
  readonly preferences: ReadonlySignal<WalletPreferences>;
  readonly liveStatus: ReadonlySignal<LiveConnectionStatus>;
  readonly lastLiveEvent: ReadonlySignal<number | null>;
  readonly notificationPermission: ReadonlySignal<
    NotificationPermission | 'unsupported'
  >;
  updatePreference<Key extends keyof WalletPreferences>(
    key: Key,
    value: WalletPreferences[Key],
  ): void;
  requestNotifications(): Promise<void>;
  reconnectLive(): void;
  backup(): void;
  lock(): Promise<void>;
}

export function settingsScreen(state: SettingsScreenState) {
  return html`<div class="settings-page">
    <section class="settings-heading">
      <div>
        <p class="eyebrow">Account control center</p>
        <h1>Private by default.<br /><em>Yours to configure.</em></h1>
        <p>
          Manage this device, live account updates and notification policy
          without exposing your signing key.
        </p>
      </div>
      <span class="settings-session"><i></i>Encrypted session</span>
    </section>
    <div class="settings-grid">
      ${identityPanel(state)} ${livePanel(state)} ${notificationPanel(state)}
      ${securityPanel(state)} ${recoveryPanel(state)}
    </div>
  </div>`;
}

function identityPanel(state: SettingsScreenState) {
  return html`<article class="settings-card identity-settings">
    <div class="settings-card-head">
      ${settingsIcon('identity')}
      <div>
        <p class="eyebrow">Profile</p>
        <h2>Wallet identity</h2>
      </div>
    </div>
    <label>Public account</label>
    <code>${() => state.wallet.value?.address ?? '—'}</code>
    <dl>
      <div>
        <dt>Network</dt>
        <dd>Payrail Devnet</dd>
      </div>
      <div>
        <dt>Signing</dt>
        <dd>On-device Ed25519</dd>
      </div>
      <div>
        <dt>Asset</dt>
        <dd>${() => state.status.value?.asset.symbol ?? '—'}</dd>
      </div>
    </dl>
    <button type="button" ${on('click', () => void copyAddress(state))}>
      Copy full address
    </button>
  </article>`;
}

function livePanel(state: SettingsScreenState) {
  return html`<article class="settings-card live-settings">
    <div class="settings-card-head">
      ${settingsIcon('live')}
      <div>
        <p class="eyebrow">Realtime</p>
        <h2>Live account stream</h2>
      </div>
      <span
        data-testid="live-status"
        class="connection-pill"
        ${attr('data-status', state.liveStatus)}
        >${() => liveLabel(state.liveStatus.value)}</span
      >
    </div>
    <p>
      Finality and checkout events arrive over a read-only, account-filtered
      WebSocket with heartbeat and automatic reconnect.
    </p>
    <dl>
      <div>
        <dt>Transport</dt>
        <dd>WSS · payrail.live.v1</dd>
      </div>
      <div>
        <dt>Heartbeat</dt>
        <dd>25 seconds</dd>
      </div>
      <div>
        <dt>Last event</dt>
        <dd>${() => lastEventLabel(state.lastLiveEvent.value)}</dd>
      </div>
    </dl>
    <button type="button" ${on('click', state.reconnectLive)}>
      Reconnect stream
    </button>
  </article>`;
}

function notificationPanel(state: SettingsScreenState) {
  return html`<article class="settings-card notification-settings">
    <div class="settings-card-head">
      ${settingsIcon('notification')}
      <div>
        <p class="eyebrow">Notifications</p>
        <h2>Signal, not noise</h2>
      </div>
    </div>
    <div class="permission-row">
      <span
        ><strong>Browser permission</strong
        ><small
          >${() => permissionLabel(state.notificationPermission.value)}</small
        ></span
      >
      <button
        class="compact-button"
        type="button"
        ${attr(
          'disabled',
          () =>
            state.notificationPermission.value === 'granted' ||
            state.notificationPermission.value === 'denied' ||
            state.notificationPermission.value === 'unsupported',
        )}
        ${on('click', () => void state.requestNotifications())}
      >
        ${() =>
          state.notificationPermission.value === 'granted'
            ? 'Enabled'
            : state.notificationPermission.value === 'denied'
              ? 'Blocked'
              : 'Enable'}
      </button>
    </div>
    <div class="preference-list">
      ${toggle(
        state,
        'browserNotifications',
        'Desktop notifications',
        'Show events outside this tab',
      )}
      ${toggle(
        state,
        'incomingPayments',
        'Incoming payments',
        'Alert when value reaches this account',
      )}
      ${toggle(
        state,
        'outgoingPayments',
        'Outgoing finality',
        'Confirm when a payment settles',
      )}
      ${toggle(
        state,
        'checkoutUpdates',
        'Payment requests',
        'Track request settlement live',
      )}
      ${toggle(
        state,
        'notificationSound',
        'Sound',
        'Play a subtle confirmation tone',
      )}
    </div>
  </article>`;
}

function securityPanel(state: SettingsScreenState) {
  return html`<article class="settings-card security-settings">
    <div class="settings-card-head">
      ${settingsIcon('security')}
      <div>
        <p class="eyebrow">Security</p>
        <h2>Device & session</h2>
      </div>
    </div>
    <label class="select-setting">
      <span
        ><strong>Automatic lock</strong
        ><small>Reset by trusted wallet actions</small></span
      >
      <select ${on('change', (event) => updateAutoLock(state, event))}>
        ${autoLockOption(state, 5)} ${autoLockOption(state, 10)}
        ${autoLockOption(state, 30)}
      </select>
    </label>
    <div class="security-proof">
      <div>
        <i>✓</i
        ><span
          ><strong>Encrypted vault</strong
          ><small>AES-256-GCM · 600K PBKDF2 rounds</small></span
        >
      </div>
      <div>
        <i>✓</i
        ><span
          ><strong>Reload protection</strong
          ><small>Non-extractable signing key · timed session</small></span
        >
      </div>
      <div>
        <i>✓</i
        ><span
          ><strong>No cloud recovery</strong
          ><small>Payrail never receives your private key</small></span
        >
      </div>
    </div>
    <div class="settings-actions">
      <button type="button" ${on('click', state.backup)}>
        Download backup
      </button>
      <button
        class="danger-button"
        type="button"
        ${on('click', () => void state.lock())}
      >
        Lock now
      </button>
    </div>
  </article>`;
}

function recoveryPanel(state: SettingsScreenState) {
  return html`<article class="settings-card recovery-settings">
    <div class="settings-card-head">
      ${settingsIcon('recovery')}
      <div>
        <p class="eyebrow">Recovery</p>
        <h2>Move to a new device</h2>
      </div>
    </div>
    <p>
      Your encrypted backup is the portable key to this account. It is safe to
      store offline, but it can only be opened with your wallet password.
    </p>
    <ol class="recovery-steps">
      <li>
        <b>1</b
        ><span
          ><strong>Download</strong
          ><small>Save the encrypted JSON backup</small></span
        >
      </li>
      <li>
        <b>2</b
        ><span
          ><strong>Transfer</strong
          ><small>Move it securely to the new device</small></span
        >
      </li>
      <li>
        <b>3</b
        ><span
          ><strong>Restore</strong
          ><small>Import it and enter the same password</small></span
        >
      </li>
    </ol>
    <div class="recovery-warning">
      <i>!</i
      ><span
        ><strong>No custodial reset</strong
        ><small
          >Without the backup or an unlocked device, Payrail cannot recover this
          self-custody wallet.</small
        ></span
      >
    </div>
    <button class="primary" type="button" ${on('click', state.backup)}>
      Download encrypted backup
    </button>
  </article>`;
}

function toggle<Key extends keyof WalletPreferences>(
  state: SettingsScreenState,
  key: Key,
  title: string,
  description: string,
) {
  return html`<label class="toggle-setting">
    <span><strong>${title}</strong><small>${description}</small></span>
    <input
      type="checkbox"
      ${attr('checked', () => Boolean(state.preferences.value[key]))}
      ${on('change', (event) =>
        state.updatePreference(
          key,
          (event.currentTarget as HTMLInputElement)
            .checked as WalletPreferences[Key],
        ),
      )}
    />
    <i aria-hidden="true"></i>
  </label>`;
}

function settingsIcon(icon: WalletIcon) {
  return html`<span class="settings-icon">${walletIcon(icon)}</span>`;
}

function autoLockOption(state: SettingsScreenState, minutes: AutoLockMinutes) {
  return html`<option
    ${attr('value', minutes)}
    ${attr(
      'selected',
      () => state.preferences.value.autoLockMinutes === minutes,
    )}
  >
    ${minutes} minutes
  </option>`;
}

function updateAutoLock(state: SettingsScreenState, event: Event) {
  const value = Number((event.currentTarget as HTMLSelectElement).value);
  if (value === 5 || value === 10 || value === 30) {
    state.updatePreference('autoLockMinutes', value);
  }
}

async function copyAddress(state: SettingsScreenState) {
  await navigator.clipboard.writeText(state.wallet.value?.address ?? '');
}

function liveLabel(status: LiveConnectionStatus) {
  if (status === 'live') return 'Live';
  if (status === 'connecting') return 'Connecting';
  if (status === 'reconnecting') return 'Reconnecting';
  return 'Offline';
}

function permissionLabel(permission: NotificationPermission | 'unsupported') {
  if (permission === 'unsupported') return 'Not supported by this browser';
  if (permission === 'granted') return 'Granted on this device';
  if (permission === 'denied') return 'Blocked in browser settings';
  return 'Permission not requested';
}

function lastEventLabel(value: number | null) {
  if (value === null) return 'Waiting for first event';
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(value);
}
