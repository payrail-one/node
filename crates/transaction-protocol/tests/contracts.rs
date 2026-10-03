use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, ContractCall, ContractDeploy,
    ContractId, IdempotencyKey, NetworkId, SignatureBytes, SignedOperation,
};
use transaction_protocol::SignedOperationCodec;

const SENDER: AccountId = AccountId::new([1; 32]);
const NETWORK: NetworkId = NetworkId::new([2; 32]);
const ASSET: AssetId = AssetId::new([3; 32]);

#[test]
fn deploy_and_call_envelopes_round_trip_canonically() {
    let operations = [
        AuthorizedOperation::ContractDeploy(ContractDeploy {
            network: NETWORK,
            idempotency_key: IdempotencyKey::new([4; 32]),
            asset: ASSET,
            owner: SENDER,
            salt: [5; 32],
            code: minimal_code(),
            fee: 7,
            nonce: 8,
            valid_until_height: 9,
        }),
        AuthorizedOperation::ContractCall(ContractCall {
            network: NETWORK,
            idempotency_key: IdempotencyKey::new([6; 32]),
            asset: ASSET,
            caller: SENDER,
            contract: ContractId::new([7; 32]),
            entrypoint: "run".to_owned(),
            args: vec![8; 48],
            attached_amount: 10,
            fee: 11,
            execution_limit: 12,
            nonce: 13,
            valid_until_height: 14,
        }),
    ];
    for operation in operations {
        let signed = SignedOperation {
            operation,
            sender_authorization: Authorization {
                signer: SENDER,
                signature: SignatureBytes::new([15; 64]),
            },
            fee_payer_authorization: None,
        };
        let encoded = SignedOperationCodec::encode(&signed).unwrap();
        assert_eq!(SignedOperationCodec::decode(&encoded).unwrap(), signed);
    }
}

#[test]
fn oversized_contract_payload_is_rejected() {
    let signed = SignedOperation {
        operation: AuthorizedOperation::ContractDeploy(ContractDeploy {
            network: NETWORK,
            idempotency_key: IdempotencyKey::new([4; 32]),
            asset: ASSET,
            owner: SENDER,
            salt: [5; 32],
            code: vec![0; ledger_core::MAX_CONTRACT_CODE_BYTES + 1],
            fee: 0,
            nonce: 0,
            valid_until_height: 1,
        }),
        sender_authorization: Authorization {
            signer: SENDER,
            signature: SignatureBytes::new([15; 64]),
        },
        fee_payer_authorization: None,
    };
    assert!(SignedOperationCodec::encode(&signed).is_err());
}

fn minimal_code() -> Vec<u8> {
    vec![b'P', b'R', b'C', b'1', 1, 3, b'r', b'u', b'n', 0, 1, 0]
}
