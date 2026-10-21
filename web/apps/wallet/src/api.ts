import {
  PlatformApiClient,
  type AccountState,
  type NetworkStatus,
  type SubmissionResult,
} from '@platform/api-client';

export type { AccountState, NetworkStatus, SubmissionResult };

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? '/api';

export const payrailApi = new PlatformApiClient(API_BASE_URL);

export function payrailLiveUrl(address: string): string {
  const url = new URL(`${API_BASE_URL.replace(/\/$/, '')}/live`, location.href);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('address', address);
  return url.toString();
}
