export type AutoLockMinutes = 5 | 10 | 30;

export interface WalletPreferences {
  readonly autoLockMinutes: AutoLockMinutes;
  readonly browserNotifications: boolean;
  readonly incomingPayments: boolean;
  readonly outgoingPayments: boolean;
  readonly checkoutUpdates: boolean;
  readonly notificationSound: boolean;
}

const STORAGE_KEY = 'payrail.wallet.preferences.v1';

export const DEFAULT_PREFERENCES: WalletPreferences = {
  autoLockMinutes: 10,
  browserNotifications: false,
  incomingPayments: true,
  outgoingPayments: true,
  checkoutUpdates: true,
  notificationSound: false,
};

export function loadPreferences(): WalletPreferences {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (!isRecord(parsed)) return DEFAULT_PREFERENCES;
    return {
      autoLockMinutes: isAutoLock(parsed.autoLockMinutes)
        ? parsed.autoLockMinutes
        : DEFAULT_PREFERENCES.autoLockMinutes,
      browserNotifications: parsed.browserNotifications === true,
      incomingPayments: parsed.incomingPayments !== false,
      outgoingPayments: parsed.outgoingPayments !== false,
      checkoutUpdates: parsed.checkoutUpdates !== false,
      notificationSound: parsed.notificationSound === true,
    };
  } catch {
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(preferences: WalletPreferences): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences));
}

function isAutoLock(value: unknown): value is AutoLockMinutes {
  return value === 5 || value === 10 || value === 30;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
