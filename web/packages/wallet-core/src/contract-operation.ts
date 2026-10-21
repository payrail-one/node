import { fromHex } from '@platform/api-client';

export const MAX_CONTRACT_CODE_BYTES = 16 * 1024;
export const MAX_CONTRACT_ARGS_BYTES = 4 * 1024;
export const MAX_CONTRACT_EXECUTION_UNITS = 100_000n;

export interface ContractDeployInput {
  readonly networkId: string;
  readonly assetId: string;
  readonly idempotencyKey: Uint8Array;
  readonly salt: Uint8Array;
  readonly code: Uint8Array;
  readonly fee: bigint;
  readonly nonce: bigint;
  readonly validUntilHeight: bigint;
}

export interface ContractCallInput {
  readonly networkId: string;
  readonly assetId: string;
  readonly idempotencyKey: Uint8Array;
  readonly contractId: string;
  readonly entrypoint: string;
  readonly args?: Uint8Array;
  readonly attachedAmount?: bigint;
  readonly fee: bigint;
  readonly executionLimit: bigint;
  readonly nonce: bigint;
  readonly validUntilHeight: bigint;
}

export function encodeContractDeploy(
  owner: Uint8Array,
  input: ContractDeployInput,
): Uint8Array<ArrayBuffer> {
  exactBytes(input.idempotencyKey, 32, 'Idempotency key');
  exactBytes(input.salt, 32, 'Contract salt');
  if (input.code.length === 0 || input.code.length > MAX_CONTRACT_CODE_BYTES) {
    throw new Error(
      `Contract code must contain 1–${MAX_CONTRACT_CODE_BYTES} bytes.`,
    );
  }
  return concat(
    Uint8Array.of(4),
    fromHex(input.networkId, 32),
    input.idempotencyKey,
    fromHex(input.assetId, 32),
    owner,
    input.salt,
    encodeUnsigned(BigInt(input.code.length), 4),
    input.code,
    encodeUnsigned(input.fee, 16),
    encodeUnsigned(input.nonce, 8),
    encodeUnsigned(input.validUntilHeight, 8),
  );
}

export function encodeContractCall(
  caller: Uint8Array,
  input: ContractCallInput,
): Uint8Array<ArrayBuffer> {
  exactBytes(input.idempotencyKey, 32, 'Idempotency key');
  const entrypoint = new TextEncoder().encode(input.entrypoint);
  if (
    entrypoint.length === 0 ||
    entrypoint.length > 32 ||
    !/^[a-z][a-z0-9_]{0,31}$/.test(input.entrypoint)
  ) {
    throw new Error('Contract entrypoint must be a canonical lowercase name.');
  }
  const args = Uint8Array.from(input.args ?? []);
  if (args.length > MAX_CONTRACT_ARGS_BYTES) {
    throw new Error(
      `Contract arguments exceed ${MAX_CONTRACT_ARGS_BYTES} bytes.`,
    );
  }
  if (
    input.executionLimit <= 0n ||
    input.executionLimit > MAX_CONTRACT_EXECUTION_UNITS
  ) {
    throw new Error(
      `Execution limit must be between 1 and ${MAX_CONTRACT_EXECUTION_UNITS}.`,
    );
  }
  return concat(
    Uint8Array.of(5),
    fromHex(input.networkId, 32),
    input.idempotencyKey,
    fromHex(input.assetId, 32),
    caller,
    fromHex(input.contractId, 32),
    Uint8Array.of(entrypoint.length),
    entrypoint,
    encodeUnsigned(BigInt(args.length), 4),
    args,
    encodeUnsigned(input.attachedAmount ?? 0n, 16),
    encodeUnsigned(input.fee, 16),
    encodeUnsigned(input.executionLimit, 8),
    encodeUnsigned(input.nonce, 8),
    encodeUnsigned(input.validUntilHeight, 8),
  );
}

function exactBytes(value: Uint8Array, length: number, field: string): void {
  if (value.length !== length)
    throw new Error(`${field} must be ${length} bytes.`);
}

function encodeUnsigned(
  value: bigint,
  length: number,
): Uint8Array<ArrayBuffer> {
  if (value < 0n) throw new Error('Unsigned values cannot be negative.');
  const output = new Uint8Array(length);
  let remaining = value;
  for (let index = length - 1; index >= 0; index -= 1) {
    output[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  if (remaining !== 0n)
    throw new Error(`Value does not fit in ${length} bytes.`);
  return output;
}

function concat(...parts: readonly Uint8Array[]): Uint8Array<ArrayBuffer> {
  const output = new Uint8Array(
    parts.reduce((total, part) => total + part.length, 0),
  );
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}
