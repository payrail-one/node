const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)(?:\.([0-9]+))?$/;

export function parseAmount(value: string, decimals: number): bigint {
  assertDecimals(decimals);
  const normalized = value.trim();
  const match = DECIMAL_PATTERN.exec(normalized);
  if (!match) throw new Error('Enter a positive decimal amount.');
  const fraction = match[2] ?? '';
  if (fraction.length > decimals) {
    throw new Error(`This asset supports at most ${decimals} decimal places.`);
  }
  const scale = 10n ** BigInt(decimals);
  const paddedFraction = fraction.padEnd(decimals, '0');
  const atomic = BigInt(match[1]) * scale + BigInt(paddedFraction || '0');
  if (atomic <= 0n) throw new Error('Amount must be greater than zero.');
  return atomic;
}

export function formatAmount(
  atomicValue: string | bigint,
  decimals: number,
): string {
  assertDecimals(decimals);
  const atomic =
    typeof atomicValue === 'bigint' ? atomicValue : BigInt(atomicValue);
  if (atomic < 0n) throw new Error('A balance cannot be negative.');
  if (decimals === 0) return atomic.toString();
  const scale = 10n ** BigInt(decimals);
  const whole = atomic / scale;
  const fraction = (atomic % scale)
    .toString()
    .padStart(decimals, '0')
    .replace(/0+$/, '');
  return fraction.length === 0 ? whole.toString() : `${whole}.${fraction}`;
}

function assertDecimals(decimals: number): void {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 18) {
    throw new Error('Asset decimals must be an integer between 0 and 18.');
  }
}
