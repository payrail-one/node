export const SEPOLIA_CHAIN_ID = '0xaa36a7';
export const SEPOLIA_CHAIN_NAME = 'Ethereum Sepolia';
export const TEST_USDT_ADDRESS = '0x5e3a1bBB81d5C3c1391796E4e3c8c4E0420a8c4a';
export const TOKEN_VAULT_ADDRESS = '0x74E345a36BCf50B253bac512C322d1a8Abc6b298';

const APPROVE_SELECTOR = '095ea7b3';
const ALLOWANCE_SELECTOR = 'dd62ed3e';
const BALANCE_OF_SELECTOR = '70a08231';
const DEPOSIT_SELECTOR = '1de26e16';
const UINT256_MAX = (1n << 256n) - 1n;

export interface EthereumProvider {
  request(input: {
    readonly method: string;
    readonly params?: readonly unknown[];
  }): Promise<unknown>;
}

export type DepositStage = 'approving' | 'depositing';

export function injectedEthereumProvider(): EthereumProvider | null {
  const candidate = (window as Window & { ethereum?: unknown }).ethereum;
  return isProvider(candidate) ? candidate : null;
}

export async function connectSepolia(
  provider: EthereumProvider,
): Promise<string> {
  const accounts = await provider.request({ method: 'eth_requestAccounts' });
  await provider.request({
    method: 'wallet_switchEthereumChain',
    params: [{ chainId: SEPOLIA_CHAIN_ID }],
  });
  const chainId = await provider.request({ method: 'eth_chainId' });
  if (chainId !== SEPOLIA_CHAIN_ID) {
    throw new Error('MetaMask must be connected to Ethereum Sepolia.');
  }
  const address = firstAddress(accounts);
  if (!address) throw new Error('MetaMask did not return an Ethereum account.');
  return address;
}

export async function tokenBalance(
  provider: EthereumProvider,
  account: string,
): Promise<bigint> {
  return callUint(
    provider,
    TEST_USDT_ADDRESS,
    BALANCE_OF_SELECTOR + addressWord(account),
  );
}

export async function depositTestUsdt(
  provider: EthereumProvider,
  ethereumAccount: string,
  payrailAccountId: string,
  amount: bigint,
  onStage: (stage: DepositStage) => void,
): Promise<string> {
  validateAmount(amount);
  const recipient = bytes32Word(payrailAccountId);
  const allowance = await callUint(
    provider,
    TEST_USDT_ADDRESS,
    ALLOWANCE_SELECTOR +
      addressWord(ethereumAccount) +
      addressWord(TOKEN_VAULT_ADDRESS),
  );
  if (allowance < amount) {
    onStage('approving');
    if (allowance !== 0n) {
      await sendAndWait(
        provider,
        ethereumAccount,
        TEST_USDT_ADDRESS,
        approveData(0n),
      );
    }
    await sendAndWait(
      provider,
      ethereumAccount,
      TEST_USDT_ADDRESS,
      approveData(amount),
    );
  }
  onStage('depositing');
  return sendAndWait(
    provider,
    ethereumAccount,
    TOKEN_VAULT_ADDRESS,
    `0x${DEPOSIT_SELECTOR}${recipient}${uintWord(amount)}`,
  );
}

function approveData(amount: bigint): string {
  return `0x${APPROVE_SELECTOR}${addressWord(TOKEN_VAULT_ADDRESS)}${uintWord(amount)}`;
}

async function callUint(
  provider: EthereumProvider,
  to: string,
  data: string,
): Promise<bigint> {
  const result = await provider.request({
    method: 'eth_call',
    params: [{ to, data: `0x${data}` }, 'latest'],
  });
  if (typeof result !== 'string' || !/^0x[0-9a-fA-F]{1,64}$/.test(result)) {
    throw new Error('Ethereum returned an invalid contract value.');
  }
  return BigInt(result);
}

async function sendAndWait(
  provider: EthereumProvider,
  from: string,
  to: string,
  data: string,
): Promise<string> {
  const result = await provider.request({
    method: 'eth_sendTransaction',
    params: [{ from, to, data }],
  });
  if (typeof result !== 'string' || !/^0x[0-9a-fA-F]{64}$/.test(result)) {
    throw new Error('MetaMask returned an invalid transaction hash.');
  }
  for (let attempt = 0; attempt < 120; attempt += 1) {
    const receipt = await provider.request({
      method: 'eth_getTransactionReceipt',
      params: [result],
    });
    if (receipt !== null) {
      if (!isRecord(receipt) || receipt.status !== '0x1') {
        throw new Error('The Ethereum transaction reverted.');
      }
      return result;
    }
    await delay(1_500);
  }
  throw new Error(
    'Ethereum confirmation timed out. Check the transaction in Etherscan.',
  );
}

function validateAmount(amount: bigint): void {
  if (amount <= 0n || amount > UINT256_MAX) {
    throw new Error('Enter a positive tUSDT amount.');
  }
}

function addressWord(address: string): string {
  if (!/^0x[0-9a-fA-F]{40}$/.test(address)) {
    throw new Error('Ethereum returned an invalid account address.');
  }
  return address.slice(2).toLowerCase().padStart(64, '0');
}

function bytes32Word(value: string): string {
  if (!/^[0-9a-f]{64}$/.test(value)) {
    throw new Error('Payrail account ID is not canonical.');
  }
  return value;
}

function uintWord(value: bigint): string {
  if (value < 0n || value > UINT256_MAX) {
    throw new Error('Ethereum amount is outside uint256.');
  }
  return value.toString(16).padStart(64, '0');
}

function firstAddress(value: unknown): string | null {
  if (!Array.isArray(value) || typeof value[0] !== 'string') return null;
  return /^0x[0-9a-fA-F]{40}$/.test(value[0]) ? value[0] : null;
}

function isProvider(value: unknown): value is EthereumProvider {
  return isRecord(value) && typeof value.request === 'function';
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}
