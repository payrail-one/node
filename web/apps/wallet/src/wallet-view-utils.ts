import type { AccountState, NetworkStatus } from '@platform/api-client';
import { formatAmount } from '@platform/money';
import { elementRef, html } from 'workstar';
import { encode } from 'uqr';

export function formatBalance(
  account: AccountState | null,
  status: NetworkStatus | null,
) {
  return account && status
    ? `${formatAmount(account.balance, status.asset.decimals)} ${status.asset.symbol}`
    : '0 TEST';
}

export function formatAtomic(value: string, status: NetworkStatus | null) {
  return status
    ? `${formatAmount(value, status.asset.decimals)} ${status.asset.symbol}`
    : value;
}

export function short(value: string) {
  return value.length > 24 ? `${value.slice(0, 11)}…${value.slice(-9)}` : value;
}

export function finalityLabel(status: NetworkStatus | null) {
  if (status?.finalityMode === 'four-validator-quorum') {
    return 'Quorum finality';
  }
  return status?.finalityMode === 'bft-devnet'
    ? 'BFT finality'
    : 'Devnet finality';
}

export function qrCanvas(value: string) {
  const qr = encode(value, { ecc: 'M', border: 4 });
  return html`<canvas
    width="640"
    height="640"
    aria-label="QR payment request"
    ${elementRef((element) => {
      if (!(element instanceof HTMLCanvasElement)) return;
      const context = element.getContext('2d');
      if (!context) return;
      const pixel = 640 / qr.size;
      context.fillStyle = '#fff';
      context.fillRect(0, 0, 640, 640);
      context.fillStyle = '#07100c';
      qr.data.forEach((row, y) =>
        row.forEach((filled, x) => {
          if (filled)
            context.fillRect(
              Math.floor(x * pixel),
              Math.floor(y * pixel),
              Math.ceil(pixel),
              Math.ceil(pixel),
            );
        }),
      );
    })}
  ></canvas>`;
}
