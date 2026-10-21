import assert from 'node:assert/strict';
import test from 'node:test';
import {
  connectSepolia,
  depositTestUsdt,
  SEPOLIA_CHAIN_ID,
  TEST_USDT_ADDRESS,
  TOKEN_VAULT_ADDRESS,
  type DepositStage,
  type EthereumProvider,
} from '../src/ethereum-bridge';

const ACCOUNT = '0x1586eC91f476d14ae8D5Ac6e9B56785A2387571a';
const PAYRAIL_ACCOUNT =
  '17cb79fb2b4120f2b1ec65e4198d6e08b28e813feb01e4a400839b85e18080ce';

test('connects the injected wallet to Sepolia', async () => {
  const methods: string[] = [];
  const provider: EthereumProvider = {
    async request({ method }) {
      methods.push(method);
      if (method === 'eth_requestAccounts') return [ACCOUNT];
      if (method === 'wallet_switchEthereumChain') return null;
      if (method === 'eth_chainId') return SEPOLIA_CHAIN_ID;
      throw new Error(`unexpected ${method}`);
    },
  };

  assert.equal(await connectSepolia(provider), ACCOUNT);
  assert.deepEqual(methods, [
    'eth_requestAccounts',
    'wallet_switchEthereumChain',
    'eth_chainId',
  ]);
});

test('approves the exact integer amount before depositing to the vault', async () => {
  const transactions: Array<Record<string, string>> = [];
  const stages: DepositStage[] = [];
  const provider = transactionProvider(0n, transactions);

  const hash = await depositTestUsdt(
    provider,
    ACCOUNT,
    PAYRAIL_ACCOUNT,
    1_000_000n,
    (stage) => stages.push(stage),
  );

  assert.equal(hash, transactionHash(2));
  assert.deepEqual(stages, ['approving', 'depositing']);
  assert.equal(transactions.length, 2);
  assert.equal(transactions[0]?.to, TEST_USDT_ADDRESS);
  assert.equal(transactions[0]?.data.slice(0, 10), '0x095ea7b3');
  assert.equal(
    transactions[0]?.data.slice(10, 74),
    addressWord(TOKEN_VAULT_ADDRESS),
  );
  assert.equal(transactions[0]?.data.slice(74), uintWord(1_000_000n));
  assert.equal(transactions[1]?.to, TOKEN_VAULT_ADDRESS);
  assert.equal(transactions[1]?.data.slice(0, 10), '0x1de26e16');
  assert.equal(transactions[1]?.data.slice(10, 74), PAYRAIL_ACCOUNT);
  assert.equal(transactions[1]?.data.slice(74), uintWord(1_000_000n));
});

test('resets a legacy nonzero allowance before setting a smaller allowance', async () => {
  const transactions: Array<Record<string, string>> = [];
  const provider = transactionProvider(1n, transactions);

  await depositTestUsdt(
    provider,
    ACCOUNT,
    PAYRAIL_ACCOUNT,
    2n,
    () => undefined,
  );

  assert.equal(transactions.length, 3);
  assert.equal(transactions[0]?.data.slice(74), uintWord(0n));
  assert.equal(transactions[1]?.data.slice(74), uintWord(2n));
  assert.equal(transactions[2]?.data.slice(0, 10), '0x1de26e16');
});

test('rejects zero deposits before requesting a transaction', async () => {
  let called = false;
  const provider: EthereumProvider = {
    async request() {
      called = true;
      return null;
    },
  };

  await assert.rejects(
    depositTestUsdt(provider, ACCOUNT, PAYRAIL_ACCOUNT, 0n, () => undefined),
    /positive tUSDT amount/,
  );
  assert.equal(called, false);
});

function transactionProvider(
  allowance: bigint,
  transactions: Array<Record<string, string>>,
): EthereumProvider {
  return {
    async request({ method, params }) {
      if (method === 'eth_call') return `0x${allowance.toString(16)}`;
      if (method === 'eth_sendTransaction') {
        const transaction = (params?.[0] ?? {}) as Record<string, string>;
        transactions.push(transaction);
        return transactionHash(transactions.length);
      }
      if (method === 'eth_getTransactionReceipt') return { status: '0x1' };
      throw new Error(`unexpected ${method}`);
    },
  };
}

function transactionHash(index: number): string {
  return `0x${index.toString(16).padStart(64, '0')}`;
}

function addressWord(address: string): string {
  return address.slice(2).toLowerCase().padStart(64, '0');
}

function uintWord(value: bigint): string {
  return value.toString(16).padStart(64, '0');
}
