use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement, ContractCall,
    ContractDeploy, IdempotencyKey, Ledger, LedgerError, NetworkId, OperationKind, contract_id,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const REGISTRY: AccountId = AccountId::new([2; 32]);
const ISSUER: AccountId = AccountId::new([3; 32]);
const TREASURY: AccountId = AccountId::new([4; 32]);
const ALICE: AccountId = AccountId::new([5; 32]);
const BOB: AccountId = AccountId::new([6; 32]);
const ASSET: AssetId = AssetId::new([7; 32]);

#[test]
fn contract_is_deployed_called_and_persisted_in_canonical_snapshot() {
    let mut ledger = funded_ledger();
    let deploy = deployment(escrow_code());
    let id = contract_id(&deploy);
    let receipt = ledger.deploy_contract(ALICE, deploy).unwrap();
    assert_eq!(receipt.kind, OperationKind::ContractDeploy);
    assert_eq!(ledger.contract(id).unwrap().owner, ALICE);

    let deposit = call(id, "deposit", Vec::new(), 300, 1);
    let receipt = ledger.call_contract(ALICE, deposit).unwrap();
    assert_eq!(receipt.kind, OperationKind::ContractCall);
    assert_eq!(ledger.balance(ASSET, id.account()), 300);

    let mut release_args = BOB.as_bytes().to_vec();
    release_args.extend_from_slice(&200_u128.to_be_bytes());
    ledger
        .call_contract(ALICE, call(id, "release", release_args, 0, 2))
        .unwrap();
    assert_eq!(ledger.balance(ASSET, id.account()), 100);
    assert_eq!(ledger.balance(ASSET, BOB), 200);
    assert_eq!(ledger.contract_state(id, b"released"), Some(1));

    let restored = Ledger::from_snapshot(NETWORK, ledger.snapshot()).unwrap();
    assert!(restored.contract(id).is_some());
    assert_eq!(restored.contract_state(id, b"released"), Some(1));
    assert!(restored.audit_asset(ASSET).unwrap().supply_matches_balances);
}

#[test]
fn rejected_contract_call_is_fully_atomic() {
    let mut ledger = funded_ledger();
    let deploy = deployment(escrow_code());
    let id = contract_id(&deploy);
    ledger.deploy_contract(ALICE, deploy).unwrap();
    ledger
        .call_contract(ALICE, call(id, "deposit", Vec::new(), 300, 1))
        .unwrap();
    let mut args = BOB.as_bytes().to_vec();
    args.extend_from_slice(&200_u128.to_be_bytes());
    ledger
        .call_contract(ALICE, call(id, "release", args.clone(), 0, 2))
        .unwrap();
    let before = ledger.snapshot();

    assert_eq!(
        ledger.call_contract(ALICE, call(id, "release", args, 0, 3)),
        Err(LedgerError::ContractRejected(1))
    );
    assert_eq!(ledger.snapshot(), before);
}

#[test]
fn execution_limit_exhaustion_does_not_charge_or_consume_nonce() {
    let mut ledger = funded_ledger();
    let deploy = deployment(escrow_code());
    let id = contract_id(&deploy);
    ledger.deploy_contract(ALICE, deploy).unwrap();
    let before = ledger.snapshot();
    let mut limited = call(id, "deposit", Vec::new(), 50, 1);
    limited.execution_limit = 1;
    assert_eq!(
        ledger.call_contract(ALICE, limited),
        Err(LedgerError::ContractExecutionLimit)
    );
    assert_eq!(ledger.snapshot(), before);
}

fn funded_ledger() -> Ledger {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: ASSET,
                symbol: "TEST".to_owned(),
                decimals: 6,
                class: AssetClass::NetworkNative,
                status: AssetStatus::Active,
                issuer: ISSUER,
                backing_authority: ISSUER,
                freeze_authority: ISSUER,
                treasury: TREASURY,
                max_supply: None,
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, ASSET, ALICE, 10_000).unwrap();
    ledger
}

fn deployment(code: Vec<u8>) -> ContractDeploy {
    ContractDeploy {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new([8; 32]),
        asset: ASSET,
        owner: ALICE,
        salt: [9; 32],
        code,
        fee: 10,
        nonce: 0,
        valid_until_height: 100,
    }
}

fn call(
    contract: ledger_core::ContractId,
    entrypoint: &str,
    args: Vec<u8>,
    attached_amount: u128,
    nonce: u64,
) -> ContractCall {
    ContractCall {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new(
            [10_u8.wrapping_add(u8::try_from(nonce).unwrap()); 32],
        ),
        asset: ASSET,
        caller: ALICE,
        contract,
        entrypoint: entrypoint.to_owned(),
        args,
        attached_amount,
        fee: 1,
        execution_limit: 100,
        nonce,
        valid_until_height: 100,
    }
}

fn escrow_code() -> Vec<u8> {
    let deposit = vec![
        0x0c, 9, b'D', b'e', b'p', b'o', b's', b'i', b't', b'e', b'd', 0x00,
    ];
    let mut release = Vec::new();
    release.extend([0x03, 8]);
    release.extend(b"released");
    release.push(0x01);
    release.extend(0_u128.to_be_bytes());
    release.extend([0x07, 0x0a, 0, 1]);
    release.extend([0x02, 0, 32, 0x10, 0, 0]);
    release.push(0x01);
    release.extend(1_u128.to_be_bytes());
    release.extend([0x0b, 8]);
    release.extend(b"released");
    release.extend([0x0c, 8]);
    release.extend(b"Released");
    release.push(0x00);
    program(&[("deposit", deposit), ("release", release)])
}

fn program(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut code = b"PRC1".to_vec();
    code.push(u8::try_from(entries.len()).unwrap());
    for (name, body) in entries {
        code.push(u8::try_from(name.len()).unwrap());
        code.extend(name.as_bytes());
        code.extend(u16::try_from(body.len()).unwrap().to_be_bytes());
        code.extend(body);
    }
    code
}
