# Account address format

This document specifies the canonical human-readable representation of a
ledger account. It does not assign a product name, production network prefix,
coin type or wallet derivation path.

## Encoding

An address is encoded as:

```text
bech32m(network_hrp, 0x00 || account_id[32])
```

The payload is exactly 33 bytes:

| Offset | Size | Meaning |
| --- | ---: | --- |
| 0 | 1 byte | account-address type discriminator; currently `0x00` |
| 1 | 32 bytes | raw ledger `AccountId` |

The network-specific human-readable prefix (HRP) is configured by the network.
It is not stored in consensus state. The codec maps that prefix to the full
32-byte `NetworkId`, and decoding with a codec for another network fails.

## Canonical rules

- The checksum is Bech32m, not legacy Bech32.
- The entire address is lowercase; uppercase and mixed-case inputs are rejected.
- The HRP is 2–16 lowercase ASCII alphanumeric characters and begins with a
  letter.
- The complete address is at most 90 characters.
- The decoded payload is exactly 33 bytes and its type discriminator is known.
- Wallets must choose the codec from trusted network configuration, never from
  untrusted address text alone.
- An address is a presentation encoding. Consensus, signatures and balances use
  the raw `AccountId` and `NetworkId`.

These constraints prevent a test-network address from being silently accepted
by a production-network wallet when the networks have distinct registered
prefixes. They do not replace transaction-level network binding, which is also
part of every signed operation.

## Compatibility vector

`main` below is a neutral test fixture, not a reserved production prefix.

| Field | Value |
| --- | --- |
| HRP | `main` |
| account-address type | `00` |
| account ID | 32 repetitions of byte `2a` |
| address | `main1qq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z58uvtja` |

The vector was generated independently from the Rust implementation using the
BIP-350 polymod constant and is asserted in the integration test suite.

## Wallet integration boundary

This format deliberately does not specify mnemonic generation, seed storage,
hardware-wallet transport or child-key derivation. Those decisions depend on
the final account cryptography, HSM/secure-element support and external-wallet
compatibility review. A production wallet profile must eventually publish:

- the registered production and test-network HRPs;
- the registered SLIP-0044 coin type;
- the key curve and hardened/non-hardened derivation rules;
- canonical derivation, address, signing and transaction test vectors;
- QR/deep-link media types and network mismatch behavior.
