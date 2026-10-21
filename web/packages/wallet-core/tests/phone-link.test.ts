import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { createEncryptedWallet, parsePhoneLinkChallenge } from '../src/index';

describe('Payrail phone-link authorization', () => {
  it('parses canonical bounded terms and signs the exact message', async () => {
    const now = Date.now();
    const message = challenge(now);
    const terms = parsePhoneLinkChallenge(message, now);
    assert.equal(terms.perEntryLimitMinor, 100n);
    assert.equal(terms.dailyLimitMinor, 500n);

    const created = await createEncryptedWallet(
      'paydev',
      'strong-demo-password',
    );
    const signatureHex = await created.wallet.signPhoneLinkChallenge(message);
    const key = await crypto.subtle.importKey(
      'raw',
      created.wallet.publicKey,
      'Ed25519',
      false,
      ['verify'],
    );
    assert.equal(
      await crypto.subtle.verify(
        'Ed25519',
        key,
        fromHex(signatureHex),
        new TextEncoder().encode(message),
      ),
      true,
    );
  });

  it('rejects expired, reordered and excessive mandates before signing', () => {
    const now = Date.now();
    assert.throws(() =>
      parsePhoneLinkChallenge(challenge(now - 20 * 60_000), now),
    );
    assert.throws(() =>
      parsePhoneLinkChallenge(
        challenge(now).replace('daily_limit_minor=500', 'daily_limit_minor=99'),
        now,
      ),
    );
    assert.throws(() =>
      parsePhoneLinkChallenge(
        challenge(now).replace('challenge=', 'unknown='),
        now,
      ),
    );
  });
});

function challenge(now: number): string {
  return [
    'payrail.phone-link.v1',
    'challenge=123e4567-e89b-42d3-a456-426614174000',
    `nonce=${'a'.repeat(43)}`,
    `expires_at=${now + 10 * 60_000}`,
    'per_entry_limit_minor=100',
    'daily_limit_minor=500',
    `mandate_expires_at=${now + 30 * 86_400_000}`,
  ].join('\n');
}

function fromHex(value: string): Uint8Array {
  const bytes = new Uint8Array(value.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}
