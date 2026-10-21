import type { WalletLiveEvent } from './wallet-live';
import type { WalletPreferences } from './wallet-preferences';

export function notifyLiveEvent(
  event: WalletLiveEvent,
  address: string,
  preferences: WalletPreferences,
): void {
  let title: string;
  let body: string;
  if (event.type === 'finalized') {
    const incoming = event.transaction.to === address;
    if (
      (incoming && !preferences.incomingPayments) ||
      (!incoming && !preferences.outgoingPayments)
    )
      return;
    title = incoming ? 'Payment received' : 'Payment finalized';
    body = `Finalized in block #${event.checkpoint.height}.`;
  } else if (event.type === 'checkoutUpdated') {
    if (!preferences.checkoutUpdates) return;
    title = 'Payment request updated';
    body = `Request ${event.checkout.status}.`;
  } else {
    return;
  }
  if (
    preferences.browserNotifications &&
    'Notification' in window &&
    Notification.permission === 'granted'
  ) {
    new Notification(title, { body, tag: 'payrail-wallet-event' });
  }
  if (preferences.notificationSound) playNotificationTone();
}

function playNotificationTone(): void {
  try {
    const context = new AudioContext();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.frequency.value = 660;
    gain.gain.setValueAtTime(0.04, context.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.0001, context.currentTime + 0.18);
    oscillator.connect(gain).connect(context.destination);
    oscillator.start();
    oscillator.stop(context.currentTime + 0.18);
    oscillator.addEventListener('ended', () => void context.close());
  } catch {
    // Notification sound is best-effort and never affects payment handling.
  }
}
