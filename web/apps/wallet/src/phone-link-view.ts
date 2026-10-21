import { toHex } from '@platform/api-client';
import {
  parsePhoneLinkChallenge,
  type PayrailWallet,
  type PhoneLinkTerms,
} from '@platform/wallet-core';
import { attr, html, on } from 'workstar';

const ALLOWED_ORIGIN = 'https://lottery.payrail.one';

interface ReadonlySignal<Value> {
  readonly value: Value;
}

export interface PhoneLinkRequest {
  readonly challengeId: string;
  readonly token: string;
  readonly origin: string;
}

export interface PhoneLinkChallenge {
  readonly id: string;
  readonly message: string;
  readonly expiresAt: number;
  readonly perEntryLimitMinor: string;
  readonly dailyLimitMinor: string;
  readonly mandateExpiresAt: number;
  readonly terms: PhoneLinkTerms;
}

export function phoneLinkRequestFromHash(
  hash: string,
): PhoneLinkRequest | null {
  const parameters = new URLSearchParams(hash.replace(/^#/u, ''));
  if (parameters.get('mode') !== 'phone-link') return null;
  const challengeId = parameters.get('challenge') ?? '';
  const token = parameters.get('token') ?? '';
  const origin = parameters.get('origin') ?? '';
  if (
    origin !== ALLOWED_ORIGIN ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(
      challengeId,
    ) ||
    !/^[A-Za-z0-9_-]{43}$/u.test(token)
  ) {
    throw new Error('The phone-link request is malformed or untrusted.');
  }
  return { challengeId, token, origin };
}

export async function fetchPhoneLinkChallenge(
  request: PhoneLinkRequest,
): Promise<PhoneLinkChallenge> {
  const response = await fetch(
    `${request.origin}/api/wallet-links/challenges/${request.challengeId}`,
    { headers: { authorization: `Bearer ${request.token}` } },
  );
  const value = await responseJson(response);
  if (
    typeof value.id !== 'string' ||
    typeof value.message !== 'string' ||
    typeof value.expiresAt !== 'number' ||
    typeof value.perEntryLimitMinor !== 'string' ||
    typeof value.dailyLimitMinor !== 'string' ||
    typeof value.mandateExpiresAt !== 'number'
  ) {
    throw new Error('The phone-link response is invalid.');
  }
  const terms = parsePhoneLinkChallenge(value.message);
  if (
    value.id !== request.challengeId ||
    value.expiresAt !== terms.expiresAt ||
    value.perEntryLimitMinor !== terms.perEntryLimitMinor.toString() ||
    value.dailyLimitMinor !== terms.dailyLimitMinor.toString() ||
    value.mandateExpiresAt !== terms.mandateExpiresAt
  ) {
    throw new Error('The phone-link terms do not match the signed message.');
  }
  return { ...value, terms } as PhoneLinkChallenge;
}

export async function approvePhoneLink(
  request: PhoneLinkRequest,
  challenge: PhoneLinkChallenge,
  wallet: PayrailWallet,
): Promise<string> {
  parsePhoneLinkChallenge(challenge.message);
  const signatureHex = await wallet.signPhoneLinkChallenge(challenge.message);
  const response = await fetch(
    `${request.origin}/api/wallet-links/challenges/${request.challengeId}/complete`,
    {
      method: 'POST',
      headers: {
        authorization: `Bearer ${request.token}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        walletAddress: wallet.address,
        walletAccountId: wallet.accountId,
        publicKeyHex: toHex(wallet.publicKey),
        signatureHex,
      }),
    },
  );
  const value = await responseJson(response);
  if (typeof value.returnUrl !== 'string') {
    throw new Error('The phone-link completion response is invalid.');
  }
  const returnUrl = new URL(value.returnUrl);
  if (returnUrl.origin !== request.origin) {
    throw new Error('The phone-link return URL is untrusted.');
  }
  return returnUrl.toString();
}

export function phoneLinkScreen(state: {
  readonly request: PhoneLinkRequest;
  readonly challenge: ReadonlySignal<PhoneLinkChallenge | null>;
  readonly wallet: ReadonlySignal<PayrailWallet | null>;
  readonly busy: ReadonlySignal<boolean>;
  readonly message: ReadonlySignal<string>;
  approve(): Promise<void>;
  cancel(): void;
}) {
  return html`<section class="phone-link-layout">
    <article class="phone-link-card">
      <p class="eyebrow">Payrail verified identity</p>
      <h1>Link phone.<br /><em>Keep custody.</em></h1>
      <p class="phone-link-summary">
        Random One will recognize SMS purchases from this phone as this Payrail
        account. Your private key never leaves this wallet.
      </p>
      <dl>
        <div>
          <dt>Wallet</dt>
          <dd>${() => short(state.wallet.value?.address ?? '')}</dd>
        </div>
        <div>
          <dt>Per ticket</dt>
          <dd>${() => amount(state.challenge.value?.perEntryLimitMinor)} PR</dd>
        </div>
        <div>
          <dt>24-hour limit</dt>
          <dd>${() => amount(state.challenge.value?.dailyLimitMinor)} PR</dd>
        </div>
        <div>
          <dt>Authorization ends</dt>
          <dd>${() => expiry(state.challenge.value?.mandateExpiresAt)}</dd>
        </div>
      </dl>
      <div class="phone-link-warning">
        This demo authorization identifies the wallet and limits test-credit SMS
        purchases. It does not expose a key or permit arbitrary transfers.
      </div>
      <div class="phone-link-actions">
        <button type="button" class="secondary" ${on('click', state.cancel)}>
          Cancel
        </button>
        <button
          type="button"
          class="primary"
          ${attr('disabled', () => state.busy.value || !state.challenge.value)}
          ${on('click', () => void state.approve())}
        >
          Review and sign
        </button>
      </div>
      <p class="status" role="status">${state.message}</p>
    </article>
  </section>`;
}

async function responseJson(
  response: Response,
): Promise<Record<string, unknown>> {
  const value: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const message =
      value && typeof value === 'object' && 'error' in value
        ? String(value.error)
        : `Request failed (${response.status}).`;
    throw new Error(message);
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Service returned an invalid response.');
  }
  return value as Record<string, unknown>;
}

function amount(minor: string | undefined): string {
  if (!minor) return '—';
  const value = BigInt(minor);
  return `${value / 100n}.${(value % 100n).toString().padStart(2, '0')}`;
}

function expiry(value: number | undefined): string {
  return value ? new Date(value).toLocaleDateString() : '—';
}

function short(value: string): string {
  return value ? `${value.slice(0, 12)}…${value.slice(-8)}` : '—';
}
