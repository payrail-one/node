import type {
  EncryptedWalletVault,
  WalletSession,
} from '@platform/wallet-core';

const DATABASE = 'payrail-wallet';
const STORE = 'vaults';
const ACTIVE_VAULT = 'primary';
const ACTIVE_SESSION = 'session';

export async function loadVault(): Promise<EncryptedWalletVault | null> {
  return transact('readonly', (store) => store.get(ACTIVE_VAULT));
}

export async function saveVault(vault: EncryptedWalletVault): Promise<void> {
  await transact('readwrite', (store) => store.put(vault, ACTIVE_VAULT));
}

export async function loadSession(): Promise<WalletSession | null> {
  return transact('readonly', (store) => store.get(ACTIVE_SESSION));
}

export async function saveSession(session: WalletSession): Promise<void> {
  await transact('readwrite', (store) => store.put(session, ACTIVE_SESSION));
}

export async function clearSession(): Promise<void> {
  await transact('readwrite', (store) => store.delete(ACTIVE_SESSION));
}

function transact<Result>(
  mode: IDBTransactionMode,
  operation: (store: IDBObjectStore) => IDBRequest<Result>,
): Promise<Result> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORE)) {
        database.createObjectStore(STORE);
      }
    };
    request.onerror = () => reject(new Error('Unable to open wallet storage.'));
    request.onsuccess = () => {
      const database = request.result;
      const transaction = database.transaction(STORE, mode);
      const result = operation(transaction.objectStore(STORE));
      result.onsuccess = () => resolve(result.result);
      result.onerror = () =>
        reject(new Error('Unable to access wallet storage.'));
      transaction.oncomplete = () => database.close();
      transaction.onerror = () => database.close();
    };
  });
}
