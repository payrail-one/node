import type { Checkout, SubmissionResult } from '@platform/api-client';
import { payrailLiveUrl } from './api';

export type LiveConnectionStatus =
  | 'connecting'
  | 'live'
  | 'reconnecting'
  | 'offline';

export type WalletLiveEvent =
  | ({ readonly type: 'finalized' } & SubmissionResult)
  | { readonly type: 'checkoutUpdated'; readonly checkout: Checkout }
  | { readonly type: 'resyncRequired' };

export interface WalletLiveConnection {
  close(): void;
  reconnect(): void;
}

export function connectWalletLive(
  address: string,
  onStatus: (status: LiveConnectionStatus) => void,
  onEvent: (event: WalletLiveEvent) => void,
): WalletLiveConnection {
  let socket: WebSocket | undefined;
  let reconnectTimer: number | undefined;
  let heartbeatTimer: number | undefined;
  let attempts = 0;
  let closed = false;

  const clearTimers = () => {
    if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer);
    if (heartbeatTimer !== undefined) window.clearInterval(heartbeatTimer);
    reconnectTimer = undefined;
    heartbeatTimer = undefined;
  };

  const scheduleReconnect = () => {
    if (closed || reconnectTimer !== undefined) return;
    onStatus(navigator.onLine ? 'reconnecting' : 'offline');
    const delay = Math.min(30_000, 1_000 * 2 ** Math.min(attempts, 5));
    attempts += 1;
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = undefined;
      open();
    }, delay);
  };

  const open = () => {
    if (closed || !navigator.onLine) {
      onStatus('offline');
      return;
    }
    onStatus(attempts === 0 ? 'connecting' : 'reconnecting');
    const next = new WebSocket(payrailLiveUrl(address), 'payrail.live.v1');
    socket = next;
    next.addEventListener('open', () => {
      if (socket !== next) return;
      attempts = 0;
      onStatus('live');
      heartbeatTimer = window.setInterval(() => {
        if (next.readyState === WebSocket.OPEN) next.send('ping');
      }, 25_000);
    });
    next.addEventListener('message', (message) => {
      const event = parseEvent(message.data);
      if (event) onEvent(event);
    });
    next.addEventListener('close', () => {
      if (socket !== next) return;
      socket = undefined;
      if (heartbeatTimer !== undefined) window.clearInterval(heartbeatTimer);
      heartbeatTimer = undefined;
      scheduleReconnect();
    });
    next.addEventListener('error', () => next.close());
  };

  const reconnect = () => {
    clearTimers();
    attempts = 0;
    const previous = socket;
    socket = undefined;
    previous?.close(1000, 'client reconnect');
    open();
  };

  const handleOnline = () => reconnect();
  const handleOffline = () => onStatus('offline');
  window.addEventListener('online', handleOnline);
  window.addEventListener('offline', handleOffline);
  open();

  return {
    reconnect,
    close() {
      closed = true;
      clearTimers();
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
      socket?.close(1000, 'wallet locked');
      socket = undefined;
      onStatus('offline');
    },
  };
}

function parseEvent(value: unknown): WalletLiveEvent | null {
  if (typeof value !== 'string') return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || typeof parsed.type !== 'string') return null;
  if (parsed.type === 'resyncRequired') return { type: 'resyncRequired' };
  if (
    parsed.type === 'finalized' &&
    isRecord(parsed.transaction) &&
    isRecord(parsed.checkpoint)
  ) {
    return parsed as WalletLiveEvent;
  }
  if (parsed.type === 'checkoutUpdated' && isRecord(parsed.checkout)) {
    return parsed as WalletLiveEvent;
  }
  return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
