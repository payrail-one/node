export interface NetworkAsset {
  readonly id: string;
  readonly symbol: string;
  readonly decimals: number;
}

export interface NetworkStatus {
  readonly networkId: string;
  readonly addressPrefix: string;
  readonly finalizedHeight: string;
  readonly finalityMode:
    | 'single-node-devnet'
    | 'bft-devnet'
    | 'four-validator-quorum';
  readonly validatorCount: number;
  readonly onlineValidators: number;
  readonly quorumWeight: number;
  readonly asset: NetworkAsset;
  readonly bridgeAsset?: NetworkAsset;
}

export interface AccountState {
  readonly address: string;
  readonly accountId: string;
  readonly nonce: string;
  readonly balance: string;
  readonly finalizedHeight: string;
}

export interface FinalizedTransaction {
  readonly id: string;
  readonly blockHeight: string;
  readonly operationIndex: string;
  readonly from: string;
  readonly to: string;
  readonly amount: string;
  readonly fee: string;
  readonly outcome: 'applied' | 'expired';
  readonly kind:
    | 'transfer'
    | 'sponsoredTransfer'
    | 'batchTransfer'
    | 'contractDeploy'
    | 'contractCall';
}

export interface FinalizedBlock {
  readonly height: string;
  readonly hash: string;
  readonly stateRoot: string;
  readonly transactionCount: number;
}

export interface ExplorerOverview {
  readonly status: NetworkStatus;
  readonly blocks: readonly FinalizedBlock[];
  readonly transactions: readonly FinalizedTransaction[];
}

export interface SubmissionResult {
  readonly transaction: FinalizedTransaction;
  readonly checkpoint: FinalizedBlock;
  readonly contract: ContractExecution | null;
}

export interface ContractExecution {
  readonly contractId: string;
  readonly entrypoint: string | null;
  readonly executionUnits: string | null;
  readonly codeHash: string | null;
  readonly events: readonly string[];
}

export interface ContractStateEntry {
  readonly key: string;
  readonly value: string;
}

export interface Contract {
  readonly id: string;
  readonly owner: string;
  readonly code: string;
  readonly codeHash: string;
  readonly balance: string;
  readonly state: readonly ContractStateEntry[];
  readonly finalizedHeight: string;
}

export interface Checkout {
  readonly id: string;
  readonly merchantLabel: string;
  readonly merchantAddress: string;
  readonly amount: string;
  readonly fee: string;
  readonly asset: NetworkAsset;
  readonly status: 'open' | 'processing' | 'finalized' | 'expired';
  readonly expiresAtMs: string;
  readonly validUntilHeight: string;
  readonly paymentPath: string;
  readonly smsText: string;
  readonly transaction: FinalizedTransaction | null;
}

export interface CreateCheckoutRequest {
  readonly merchantAddress: string;
  readonly amount: string;
  readonly orderReference: string;
}

export interface IssueApprovalCodeRequest {
  readonly accountAddress: string;
  readonly deviceId: string;
  readonly issuedAtMs: string;
  readonly nonce: string;
  readonly signature: string;
}

export interface IssuedApprovalCode {
  readonly code: string;
  readonly sessionToken: string;
  readonly expiresAtMs: string;
}

export interface ApprovalCodeChallenge {
  readonly status: 'waiting' | 'claimed' | 'finalized';
  readonly checkout: Checkout | null;
}

export class PlatformApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = 'PlatformApiError';
  }
}

export class PlatformApiClient {
  readonly #baseUrl: string;
  readonly #fetcher: typeof fetch;

  constructor(
    baseUrl: string,
    fetcher: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {
    this.#baseUrl = baseUrl.replace(/\/$/, '');
    this.#fetcher = fetcher;
  }

  status(): Promise<NetworkStatus> {
    return this.#request('/status');
  }

  account(address: string): Promise<AccountState> {
    return this.#request(`/accounts/${encodeURIComponent(address)}`);
  }

  bridgeAccount(address: string): Promise<AccountState> {
    return this.#request(`/bridge/accounts/${encodeURIComponent(address)}`);
  }

  contract(id: string): Promise<Contract> {
    return this.#request(`/contracts/${encodeURIComponent(id)}`);
  }

  overview(): Promise<ExplorerOverview> {
    return this.#request('/explorer');
  }

  faucet(address: string): Promise<SubmissionResult> {
    return this.#request('/faucet', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ address }),
    });
  }

  submit(envelope: Uint8Array): Promise<SubmissionResult> {
    return this.#request('/transactions', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ envelope: toHex(envelope) }),
    });
  }

  createCheckout(request: CreateCheckoutRequest): Promise<Checkout> {
    return this.#request('/checkouts', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
    });
  }

  checkout(id: string): Promise<Checkout> {
    return this.#request(`/checkouts/${encodeURIComponent(id)}`);
  }

  issueApprovalCode(
    request: IssueApprovalCodeRequest,
  ): Promise<IssuedApprovalCode> {
    return this.#request('/approval-codes', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
    });
  }

  approvalCodeChallenge(sessionToken: string): Promise<ApprovalCodeChallenge> {
    return this.#request(
      `/approval-codes/sessions/${encodeURIComponent(sessionToken)}`,
    );
  }

  submitCheckout(id: string, envelope: Uint8Array): Promise<Checkout> {
    return this.#request(`/checkouts/${encodeURIComponent(id)}/transactions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ envelope: toHex(envelope) }),
    });
  }

  async #request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.#fetcher(`${this.#baseUrl}${path}`, init);
    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as {
        error?: unknown;
      } | null;
      const message =
        typeof body?.error === 'string' ? body.error : response.statusText;
      throw new PlatformApiError(message || 'Request failed.', response.status);
    }
    return (await response.json()) as T;
  }
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join(
    '',
  );
}

export function fromHex(value: string, expectedBytes?: number): Uint8Array {
  if (!/^[0-9a-f]+$/.test(value) || value.length % 2 !== 0) {
    throw new Error('Expected canonical lowercase hexadecimal data.');
  }
  const bytes = Uint8Array.from(value.match(/.{2}/g) ?? [], (byte) =>
    Number.parseInt(byte, 16),
  );
  if (expectedBytes !== undefined && bytes.length !== expectedBytes) {
    throw new Error(`Expected ${expectedBytes} bytes.`);
  }
  return bytes;
}
