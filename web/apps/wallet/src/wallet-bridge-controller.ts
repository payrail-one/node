import type { AccountState, NetworkStatus } from '@platform/api-client';
import { parseAmount } from '@platform/money';
import type { PayrailWallet } from '@platform/wallet-core';
import { signal } from 'workstar';
import { payrailApi } from './api';
import {
  connectSepolia,
  depositTestUsdt,
  injectedEthereumProvider,
  tokenBalance,
  type DepositStage,
  type EthereumProvider,
} from './ethereum-bridge';

export type BridgeStage =
  | 'idle'
  | 'connecting'
  | DepositStage
  | 'waiting'
  | 'complete';

interface BridgeControllerOptions {
  readonly wallet: { readonly value: PayrailWallet | null };
  readonly status: { readonly value: NetworkStatus | null };
  setMessage(message: string): void;
}

export interface BridgeController {
  readonly ethereumAddress: { readonly value: string | null };
  readonly ethereumBalance: { readonly value: string };
  readonly payrailBalance: { readonly value: AccountState | null };
  readonly transactionHash: { readonly value: string | null };
  readonly stage: { readonly value: BridgeStage };
  readonly busy: { readonly value: boolean };
  readonly providerAvailable: boolean;
  connect(): Promise<void>;
  deposit(event: Event): Promise<void>;
  refresh(): Promise<void>;
}

export function createBridgeController(
  options: BridgeControllerOptions,
): BridgeController {
  const ethereumAddress = signal<string | null>(null);
  const ethereumBalance = signal('0');
  const payrailBalance = signal<AccountState | null>(null);
  const transactionHash = signal<string | null>(null);
  const stage = signal<BridgeStage>('idle');
  const busy = signal(false);
  let provider: EthereumProvider | null = null;

  const refresh = async () => {
    const wallet = options.wallet.value;
    if (!wallet) return;
    const [nextPayrail, nextEthereum] = await Promise.all([
      payrailApi.bridgeAccount(wallet.address),
      provider && ethereumAddress.value
        ? tokenBalance(provider, ethereumAddress.value)
        : Promise.resolve(null),
    ]);
    payrailBalance.value = nextPayrail;
    if (nextEthereum !== null) ethereumBalance.value = nextEthereum.toString();
  };

  const connect = async () => {
    busy.value = true;
    stage.value = 'connecting';
    try {
      provider = injectedEthereumProvider();
      if (!provider)
        throw new Error('Install MetaMask to bridge from Sepolia.');
      ethereumAddress.value = await connectSepolia(provider);
      await refresh();
      stage.value = 'idle';
      options.setMessage('MetaMask connected to Ethereum Sepolia.');
    } catch (error) {
      stage.value = 'idle';
      options.setMessage(errorMessage(error));
    } finally {
      busy.value = false;
    }
  };

  const deposit = async (event: Event) => {
    event.preventDefault();
    const wallet = options.wallet.value;
    const network = options.status.value;
    if (!wallet || !network) return;
    busy.value = true;
    transactionHash.value = null;
    try {
      if (!provider || !ethereumAddress.value) {
        throw new Error('Connect MetaMask before depositing.');
      }
      const form = new FormData(event.currentTarget as HTMLFormElement);
      const input = form.get('bridge-amount');
      if (typeof input !== 'string') throw new Error('Enter a tUSDT amount.');
      const decimals = network.bridgeAsset?.decimals ?? 6;
      const amount = parseAmount(input, decimals);
      const available = await tokenBalance(provider, ethereumAddress.value);
      if (amount > available)
        throw new Error('Amount exceeds the MetaMask tUSDT balance.');
      const initialBalance = BigInt(payrailBalance.value?.balance ?? '0');
      transactionHash.value = await depositTestUsdt(
        provider,
        ethereumAddress.value,
        wallet.accountId,
        amount,
        (next) => {
          stage.value = next;
          options.setMessage(
            next === 'approving'
              ? 'Confirm the tUSDT approval in MetaMask.'
              : 'Confirm the vault deposit in MetaMask.',
          );
        },
      );
      stage.value = 'waiting';
      options.setMessage(
        'Deposit mined. Waiting for finalized Ethereum consensus and Payrail settlement…',
      );
      const credited = await waitForCredit(
        wallet.address,
        initialBalance + amount,
      );
      payrailBalance.value = credited;
      ethereumBalance.value = (
        await tokenBalance(provider, ethereumAddress.value)
      ).toString();
      stage.value = 'complete';
      options.setMessage(
        `tUSDT finalized on Payrail at block #${credited.finalizedHeight}.`,
      );
    } catch (error) {
      if (stage.value !== 'waiting') stage.value = 'idle';
      options.setMessage(errorMessage(error));
    } finally {
      busy.value = false;
    }
  };

  return {
    ethereumAddress,
    ethereumBalance,
    payrailBalance,
    transactionHash,
    stage,
    busy,
    providerAvailable: injectedEthereumProvider() !== null,
    connect,
    deposit,
    refresh,
  };
}

async function waitForCredit(
  address: string,
  expectedBalance: bigint,
): Promise<AccountState> {
  for (let attempt = 0; attempt < 180; attempt += 1) {
    const account = await payrailApi.bridgeAccount(address);
    if (BigInt(account.balance) >= expectedBalance) return account;
    await new Promise((resolve) => window.setTimeout(resolve, 10_000));
  }
  throw new Error(
    'Deposit is still waiting for Ethereum finality. You can safely refresh later.',
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  ) {
    return error.message;
  }
  return 'Bridge request failed.';
}
