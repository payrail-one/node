import {
  fromHex,
  toHex,
  type Checkout,
  type ExplorerOverview,
  type IssuedApprovalCode,
} from '@platform/api-client';
import { parseAmount } from '@platform/money';
import {
  accountIdFromAddress,
  createEncryptedWallet,
  parseEncryptedWalletVault,
  restoreWalletSession,
  unlockEncryptedWallet,
  type EncryptedWalletVault,
  type PayrailWallet,
} from '@platform/wallet-core';
import { payrailBrand } from '@platform/ui-kit';
import { html, mount, signal } from 'workstar';
import {
  payrailApi,
  type AccountState,
  type NetworkStatus,
  type SubmissionResult,
} from './api';
import {
  clearSession,
  loadSession,
  loadVault,
  saveSession,
  saveVault,
} from './vault-store';
import { authScreen } from './wallet-auth-view';
import { createBridgeController } from './wallet-bridge-controller';
import { walletScreen, type WalletView } from './wallet-views';
import { connectWalletLive, type WalletLiveConnection } from './wallet-live';
import { loadPreferences, savePreferences } from './wallet-preferences';
import { notifyLiveEvent } from './wallet-notifications';
import { privacyScreen } from './privacy-view';
import {
  approvePhoneLink as completePhoneLink,
  fetchPhoneLinkChallenge,
  phoneLinkRequestFromHash,
  phoneLinkScreen,
  type PhoneLinkChallenge,
  type PhoneLinkRequest,
} from './phone-link-view';
import '@platform/ui-kit/styles.css';
import './style.css';
import './wallet-dashboard.css';
import './wallet-premium.css';
import './wallet-responsive.css';
import './wallet-security.css';
import './wallet-payments.css';
import './wallet-settings.css';
import './wallet-polish.css';
import './wallet-code.css';
import './privacy.css';
import './phone-link.css';
import './wallet-bridge.css';

interface PendingTransfer {
  readonly recipient: string;
  readonly amount: string;
}

function createApp() {
  const status = signal<NetworkStatus | null>(null);
  const vault = signal<EncryptedWalletVault | null>(null);
  const wallet = signal<PayrailWallet | null>(null);
  const account = signal<AccountState | null>(null);
  const receipt = signal<SubmissionResult | null>(null);
  const pendingTransfer = signal<PendingTransfer | null>(null);
  const overview = signal<ExplorerOverview | null>(null);
  const checkout = signal<Checkout | null>(null);
  const checkoutReviewed = signal(false);
  const approvalCode = signal<IssuedApprovalCode | null>(null);
  const codeSeconds = signal(0);
  const routeCheckoutId = checkoutIdFromPath(location.pathname);
  const view = signal<WalletView>(routeCheckoutId ? 'checkout' : 'overview');
  const message = signal('Connecting to Payrail testnet…');
  const phoneLinkChallenge = signal<PhoneLinkChallenge | null>(null);
  let phoneLinkRequest: PhoneLinkRequest | null = null;
  try {
    phoneLinkRequest = phoneLinkRequestFromHash(location.hash);
  } catch (error) {
    message.value = errorMessage(error);
  }
  const busy = signal(false);
  const balanceHidden = signal(false);
  const preferences = signal(loadPreferences());
  const liveStatus = signal<'connecting' | 'live' | 'reconnecting' | 'offline'>(
    'offline',
  );
  const lastLiveEvent = signal<number | null>(null);
  const notificationPermission = signal<NotificationPermission | 'unsupported'>(
    'Notification' in window ? Notification.permission : 'unsupported',
  );
  const bridge = createBridgeController({
    wallet,
    status,
    setMessage: (next) => (message.value = next),
  });
  let lockTimer: number | undefined;
  let liveConnection: WalletLiveConnection | undefined;
  let codePollTimer: number | undefined;
  let codeCountdownTimer: number | undefined;

  const clearCodeTimers = () => {
    if (codePollTimer !== undefined) window.clearTimeout(codePollTimer);
    if (codeCountdownTimer !== undefined)
      window.clearInterval(codeCountdownTimer);
    codePollTimer = undefined;
    codeCountdownTimer = undefined;
  };

  const cancelPayrailCode = () => {
    clearCodeTimers();
    approvalCode.value = null;
    codeSeconds.value = 0;
    message.value = 'Payrail Code cancelled. No payment was authorized.';
  };

  async function pollPayrailCode(sessionToken: string) {
    if (approvalCode.value?.sessionToken !== sessionToken) return;
    try {
      const challenge = await payrailApi.approvalCodeChallenge(sessionToken);
      if (approvalCode.value?.sessionToken !== sessionToken) return;
      if (challenge.status !== 'waiting' && challenge.checkout) {
        clearCodeTimers();
        approvalCode.value = null;
        codeSeconds.value = 0;
        checkout.value = challenge.checkout;
        checkoutReviewed.value = false;
        view.value = 'checkout';
        resetLockTimer();
        message.value =
          'Payrail Code linked. Review the merchant and amount before signing.';
        return;
      }
      codePollTimer = window.setTimeout(
        () => void pollPayrailCode(sessionToken),
        1_500,
      );
    } catch (error) {
      if (approvalCode.value?.sessionToken !== sessionToken) return;
      clearCodeTimers();
      approvalCode.value = null;
      codeSeconds.value = 0;
      message.value = errorMessage(error);
    }
  }

  async function createPayrailCode() {
    if (!wallet.value || !status.value) return;
    await run(async () => {
      clearCodeTimers();
      const request = await wallet.value?.createApprovalCodeIssue({
        networkId: status.value?.networkId ?? '',
        deviceId: payrailCodeDeviceId(),
      });
      if (!request) throw new Error('Wallet was locked before code issuance.');
      const issued = await payrailApi.issueApprovalCode(request);
      approvalCode.value = issued;
      const updateCountdown = () => {
        const remaining = Math.max(
          0,
          Math.ceil((Number(issued.expiresAtMs) - Date.now()) / 1_000),
        );
        codeSeconds.value = remaining;
        if (
          remaining === 0 &&
          approvalCode.value?.sessionToken === issued.sessionToken
        ) {
          clearCodeTimers();
          approvalCode.value = null;
          message.value = 'Payrail Code expired. Generate a new code.';
        }
      };
      updateCountdown();
      codeCountdownTimer = window.setInterval(updateCountdown, 1_000);
      resetLockTimer();
      message.value = 'Payrail Code ready. Enter it at checkout.';
      void pollPayrailCode(issued.sessionToken);
    });
  }

  const resetLockTimer = () => {
    if (lockTimer !== undefined) window.clearTimeout(lockTimer);
    if (wallet.value) {
      const autoLockMs = preferences.value.autoLockMinutes * 60 * 1000;
      lockTimer = window.setTimeout(lock, autoLockMs);
      void saveSession(
        wallet.value.createSession(Date.now() + autoLockMs),
      ).catch(() => undefined);
    }
  };

  const refresh = async () => {
    const [network, networkOverview] = await Promise.all([
      payrailApi.status(),
      payrailApi.overview(),
    ]);
    status.value = network;
    overview.value = networkOverview;
    if (wallet.value) {
      account.value = await payrailApi.account(wallet.value.address);
    }
  };

  const startLive = (activeWallet: PayrailWallet) => {
    liveConnection?.close();
    liveConnection = connectWalletLive(
      activeWallet.address,
      (next) => (liveStatus.value = next),
      (event) => {
        lastLiveEvent.value = Date.now();
        if (event.type === 'checkoutUpdated') checkout.value = event.checkout;
        void refresh().catch((error) => (message.value = errorMessage(error)));
        notifyLiveEvent(event, activeWallet.address, preferences.value);
      },
    );
  };

  const run = async (operation: () => Promise<void>) => {
    busy.value = true;
    try {
      await operation();
    } catch (error) {
      message.value = errorMessage(error);
    } finally {
      busy.value = false;
    }
  };

  const createWallet = async (event: Event) => {
    event.preventDefault();
    if (!status.value) {
      message.value = 'Still connecting to Payrail. Try again in a moment.';
      return;
    }
    const data = new FormData(event.currentTarget as HTMLFormElement);
    const password = required(data, 'password');
    if (password !== required(data, 'confirmation')) {
      message.value = 'Passwords do not match.';
      return;
    }
    await run(async () => {
      const created = await createEncryptedWallet(
        status.value?.addressPrefix ?? '',
        password,
      );
      await saveVault(created.vault);
      vault.value = created.vault;
      wallet.value = created.wallet;
      await refresh();
      resetLockTimer();
      startLive(created.wallet);
      message.value = 'Wallet created and encrypted on this device.';
    });
  };

  const unlock = async (event: Event) => {
    event.preventDefault();
    if (!status.value || !vault.value) {
      message.value = 'Still connecting to Payrail. Try again in a moment.';
      return;
    }
    const password = required(
      new FormData(event.currentTarget as HTMLFormElement),
      'password',
    );
    await run(async () => {
      wallet.value = await unlockEncryptedWallet(
        status.value?.addressPrefix ?? '',
        password,
        vault.value as EncryptedWalletVault,
      );
      await refresh();
      resetLockTimer();
      startLive(wallet.value);
      message.value = 'Wallet unlocked locally.';
    });
  };

  async function lock() {
    const activeWallet = wallet.value;
    if (lockTimer !== undefined) window.clearTimeout(lockTimer);
    lockTimer = undefined;
    liveConnection?.close();
    liveConnection = undefined;
    clearCodeTimers();
    approvalCode.value = null;
    codeSeconds.value = 0;
    try {
      await clearSession();
    } catch (error) {
      message.value = `Unable to securely lock this session. ${errorMessage(error)}`;
      resetLockTimer();
      if (activeWallet) startLive(activeWallet);
      return;
    }
    wallet.value = null;
    account.value = null;
    receipt.value = null;
    pendingTransfer.value = null;
    message.value =
      'Wallet locked. The encrypted vault remains on this device.';
  }

  const fund = async () => {
    if (!wallet.value) return;
    await run(async () => {
      receipt.value = await payrailApi.faucet(wallet.value?.address ?? '');
      await refresh();
      resetLockTimer();
      message.value = 'Test funds finalized on Payrail.';
    });
  };

  const prepareSend = async (event: Event) => {
    event.preventDefault();
    if (!wallet.value || !account.value || !status.value) return;
    const data = new FormData(event.currentTarget as HTMLFormElement);
    await run(async () => {
      const recipient = required(data, 'recipient');
      const amount = parseAmount(
        required(data, 'amount'),
        status.value?.asset.decimals ?? 0,
      );
      if (amount > BigInt(account.value?.balance ?? '0')) {
        throw new Error('Amount exceeds the finalized balance.');
      }
      accountIdFromAddress(recipient, status.value?.addressPrefix ?? '');
      pendingTransfer.value = { recipient, amount: amount.toString() };
      receipt.value = null;
      message.value = 'Review the exact recipient and amount before signing.';
    });
  };

  const confirmSend = async () => {
    if (
      !wallet.value ||
      !account.value ||
      !status.value ||
      !pendingTransfer.value
    )
      return;
    const pending = pendingTransfer.value;
    await run(async () => {
      const amount = BigInt(pending.amount);
      if (amount > BigInt(account.value?.balance ?? '0')) {
        throw new Error('Amount exceeds the finalized balance.');
      }
      const signed = await wallet.value?.signTransfer({
        networkId: status.value?.networkId ?? '',
        assetId: status.value?.asset.id ?? '',
        idempotencyKey: crypto.getRandomValues(new Uint8Array(32)),
        recipientAccountId: accountIdFromAddress(
          pending.recipient,
          status.value?.addressPrefix ?? '',
        ),
        amount,
        fee: 0n,
        nonce: BigInt(account.value?.nonce ?? '0'),
        validUntilHeight: BigInt(status.value?.finalizedHeight ?? '0') + 20n,
      });
      if (!signed) throw new Error('Wallet was locked before signing.');
      receipt.value = await payrailApi.submit(signed);
      await refresh();
      resetLockTimer();
      message.value = `Finalized in block #${receipt.value.checkpoint.height}.`;
      pendingTransfer.value = null;
    });
  };

  const cancelSend = () => {
    pendingTransfer.value = null;
    message.value = 'Payment cancelled before signing.';
  };

  const createCheckout = async (event: Event) => {
    event.preventDefault();
    if (!wallet.value || !status.value) return;
    const data = new FormData(event.currentTarget as HTMLFormElement);
    await run(async () => {
      const amount = parseAmount(
        required(data, 'checkout-amount'),
        status.value?.asset.decimals ?? 0,
      );
      checkout.value = await payrailApi.createCheckout({
        merchantAddress: wallet.value?.address ?? '',
        amount: amount.toString(),
        orderReference: required(data, 'order-reference'),
      });
      resetLockTimer();
      message.value = 'One payment request is ready for web, QR and SMS.';
    });
  };

  const copySms = async () => {
    if (!checkout.value) return;
    const text = checkout.value.smsText.replace(
      '{origin}',
      'https://wallet.payrail.one',
    );
    try {
      await navigator.clipboard.writeText(text);
      message.value = 'SMS payment request copied.';
    } catch {
      message.value = text;
    }
  };

  const reviewCheckout = () => {
    checkoutReviewed.value = true;
    message.value = 'Review the exact recipient and amount before signing.';
  };

  const cancelCheckout = () => {
    checkoutReviewed.value = false;
    message.value = 'Payment cancelled before signing.';
  };

  const confirmCheckout = async () => {
    if (!wallet.value || !account.value || !status.value || !checkout.value)
      return;
    const current = checkout.value;
    await run(async () => {
      const envelope = await wallet.value?.signTransfer({
        networkId: status.value?.networkId ?? '',
        assetId: current.asset.id,
        idempotencyKey: fromHex(current.id, 32),
        recipientAccountId: accountIdFromAddress(
          current.merchantAddress,
          status.value?.addressPrefix ?? '',
        ),
        amount: BigInt(current.amount),
        fee: BigInt(current.fee),
        nonce: BigInt(account.value?.nonce ?? '0'),
        validUntilHeight: BigInt(current.validUntilHeight),
      });
      if (!envelope) throw new Error('Wallet was locked before signing.');
      checkout.value = await payrailApi.submitCheckout(current.id, envelope);
      checkoutReviewed.value = false;
      await refresh();
      resetLockTimer();
      message.value = `Checkout finalized in block #${checkout.value.transaction?.blockHeight ?? '—'}.`;
    });
  };

  const backup = () => {
    if (!vault.value) return;
    const blob = new Blob([JSON.stringify(vault.value)], {
      type: 'application/json',
    });
    const link = document.createElement('a');
    link.href = URL.createObjectURL(blob);
    link.download = `payrail-wallet-${vault.value.address.slice(0, 14)}.json`;
    link.click();
    URL.revokeObjectURL(link.href);
    message.value = 'Encrypted wallet backup downloaded.';
  };

  const importVault = async (event: Event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !status.value) return;
    await run(async () => {
      if (file.size > 32_768) throw new Error('Wallet backup is too large.');
      const parsed = parseEncryptedWalletVault(JSON.parse(await file.text()));
      accountIdFromAddress(parsed.address, status.value?.addressPrefix ?? '');
      await saveVault(parsed);
      vault.value = parsed;
      wallet.value = null;
      account.value = null;
      receipt.value = null;
      pendingTransfer.value = null;
      message.value = 'Encrypted wallet backup imported. Unlock it locally.';
    });
  };

  const approvePhoneLink = async () => {
    if (!phoneLinkRequest || !phoneLinkChallenge.value || !wallet.value) return;
    await run(async () => {
      const returnUrl = await completePhoneLink(
        phoneLinkRequest as PhoneLinkRequest,
        phoneLinkChallenge.value as PhoneLinkChallenge,
        wallet.value as PayrailWallet,
      );
      resetLockTimer();
      window.location.assign(returnUrl);
    });
  };

  const cancelPhoneLink = () => {
    if (phoneLinkRequest) {
      window.location.assign(`${phoneLinkRequest.origin}/#account`);
    }
  };

  if (phoneLinkRequest) {
    void fetchPhoneLinkChallenge(phoneLinkRequest)
      .then((challenge) => {
        phoneLinkChallenge.value = challenge;
        message.value = 'Review the limits before signing this phone link.';
      })
      .catch((error) => (message.value = errorMessage(error)));
  }

  void Promise.all([
    loadVault(),
    loadSession(),
    payrailApi.status(),
    payrailApi.overview(),
    routeCheckoutId ? payrailApi.checkout(routeCheckoutId) : null,
  ])
    .then(
      async ([
        storedVault,
        storedSession,
        network,
        networkOverview,
        loadedCheckout,
      ]) => {
        vault.value = storedVault;
        status.value = network;
        overview.value = networkOverview;
        checkout.value = loadedCheckout;
        if (storedVault && storedSession) {
          try {
            const restored = await restoreWalletSession(
              network.addressPrefix,
              storedSession,
            );
            if (restored.address !== storedVault.address) {
              throw new Error('Wallet session belongs to another vault.');
            }
            wallet.value = restored;
            account.value = await payrailApi.account(restored.address);
            resetLockTimer();
            startLive(restored);
            message.value = 'Secure session restored on this device.';
            return;
          } catch {
            await clearSession();
          }
        }
        message.value = storedVault
          ? 'Payrail is live. Unlock your wallet.'
          : 'Payrail is live. Create your encrypted wallet.';
      },
    )
    .catch((error) => (message.value = errorMessage(error)));

  return html`<div class="shell">
    <header class="pr-public-header">
      ${payrailBrand()}
      <nav>
        <a href="/privacy">Privacy</a>
        <a href="https://explorer.payrail.one">Explorer</a>
        <span class="network"><i></i>Payrail testnet</span>
      </nav>
    </header>
    <main>
      ${() =>
        wallet.value
          ? phoneLinkRequest
            ? phoneLinkScreen({
                request: phoneLinkRequest,
                challenge: phoneLinkChallenge,
                wallet,
                busy,
                message,
                approve: approvePhoneLink,
                cancel: cancelPhoneLink,
              })
            : walletScreen({
                status,
                wallet,
                account,
                receipt,
                pendingTransfer,
                overview,
                checkout,
                checkoutReviewed,
                approvalCode,
                codeSeconds,
                view,
                message,
                busy,
                balanceHidden,
                preferences,
                liveStatus,
                lastLiveEvent,
                notificationPermission,
                toggleBalancePrivacy: () =>
                  (balanceHidden.value = !balanceHidden.value),
                updatePreference: (key, value) => {
                  preferences.value = { ...preferences.value, [key]: value };
                  savePreferences(preferences.value);
                  if (key === 'autoLockMinutes') resetLockTimer();
                },
                requestNotifications: async () => {
                  if (!('Notification' in window)) return;
                  notificationPermission.value =
                    await Notification.requestPermission();
                  if (notificationPermission.value === 'granted') {
                    preferences.value = {
                      ...preferences.value,
                      browserNotifications: true,
                    };
                    savePreferences(preferences.value);
                  }
                },
                reconnectLive: () => liveConnection?.reconnect(),
                selectView: (next) => {
                  view.value = next;
                  if (next === 'bridge') {
                    void bridge
                      .refresh()
                      .catch((error) => (message.value = errorMessage(error)));
                  }
                },
                bridge,
                refresh: () => run(refresh),
                fund,
                prepareSend,
                confirmSend,
                cancelSend,
                createCheckout,
                copySms,
                reviewCheckout,
                cancelCheckout,
                confirmCheckout,
                createPayrailCode,
                cancelPayrailCode,
                lock,
                backup,
              })
          : authScreen({
              vault,
              status,
              message,
              busy,
              createWallet,
              unlock,
              importVault,
            })}
    </main>
  </div>`;
}

const PAYRAIL_CODE_DEVICE_KEY = 'payrail.code.device.v1';

function payrailCodeDeviceId(): Uint8Array {
  const stored = localStorage.getItem(PAYRAIL_CODE_DEVICE_KEY);
  if (stored) {
    try {
      return fromHex(stored, 32);
    } catch {
      localStorage.removeItem(PAYRAIL_CODE_DEVICE_KEY);
    }
  }
  const created = crypto.getRandomValues(new Uint8Array(32));
  localStorage.setItem(PAYRAIL_CODE_DEVICE_KEY, toHex(created));
  return created;
}

function checkoutIdFromPath(pathname: string): string | null {
  const match = /^\/pay\/([0-9a-f]{64})$/.exec(pathname);
  return match?.[1] ?? null;
}

function required(data: FormData, key: string): string {
  const value = data.get(key);
  if (typeof value !== 'string' || value === '') {
    throw new Error(`${key} is required.`);
  }
  return value;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Something went wrong.';
}

const host = document.querySelector('#app');
if (!host) throw new Error('Missing application mount point.');
mount(host, location.pathname === '/privacy' ? privacyScreen() : createApp());
