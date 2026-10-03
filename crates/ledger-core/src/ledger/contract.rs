use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::{
    AccountId, AccountStatus, AssetId, AuthorizedOperation, Balance, ContractCall, ContractDeploy,
    ContractId, ContractRecord, Event, LedgerError, OperationKind, OperationOutcome,
    OperationReceipt,
    contract::{execute, validate_code},
    contract_id,
};

use super::{Ledger, checked_add, has_backing_deficit};

const MAX_CONTRACT_STATE_ENTRIES: usize = 256;

impl Ledger {
    /// Deploys validated deterministic Payrail bytecode into canonical state.
    ///
    /// # Errors
    ///
    /// Rejects invalid code, duplicate identities, authorization, nonce, fee,
    /// asset policy or balance failures without mutating state.
    pub fn deploy_contract(
        &mut self,
        origin: AccountId,
        deploy: ContractDeploy,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_contract_deploy(origin, &deploy)?;
        let id = contract_id(&deploy);
        let operation_id = AuthorizedOperation::ContractDeploy(deploy.clone()).operation_id()?;
        let treasury = self.asset(deploy.asset)?.definition.treasury;
        let balances = self.fee_balances(deploy.asset, deploy.owner, treasury, deploy.fee)?;
        let receipt = self.create_receipt(
            operation_id,
            deploy.owner,
            deploy.idempotency_key,
            deploy.nonce,
            OperationKind::ContractDeploy,
            OperationOutcome::Applied,
        );
        let next_nonce = deploy
            .nonce
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let next_index = receipt
            .operation_index
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let code_hash: [u8; 32] = Sha256::digest(&deploy.code).into();

        for (account, balance) in balances {
            self.set_balance(deploy.asset, account, balance);
        }
        self.contracts.insert(
            id,
            ContractRecord {
                id,
                owner: deploy.owner,
                code: deploy.code,
            },
        );
        self.nonces.insert(deploy.owner, next_nonce);
        self.next_operation_index = next_index;
        self.events.push(Event::ContractDeployed {
            operation_id,
            contract: id,
            owner: deploy.owner,
            code_hash,
        });
        Ok(receipt)
    }

    /// Executes a deployed contract atomically against canonical state.
    ///
    /// # Errors
    ///
    /// Rejects unknown contracts, invalid arguments, exhausted execution fuel,
    /// failed contract requirements or monetary/state invariants. No mutation
    /// is committed unless the complete execution plan is valid.
    pub fn call_contract(
        &mut self,
        origin: AccountId,
        call: ContractCall,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_contract_call(origin, &call)?;
        let contract = self
            .contracts
            .get(&call.contract)
            .ok_or(LedgerError::ContractNotFound)?;
        let state = self.contract_values(call.contract);
        let execution = execute(
            &contract.code,
            &call.entrypoint,
            &call.args,
            call.attached_amount,
            call.caller,
            &state,
            call.execution_limit,
        )?;
        let projected_state_entries = state
            .keys()
            .chain(execution.writes.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if projected_state_entries > MAX_CONTRACT_STATE_ENTRIES {
            return Err(LedgerError::ContractStateLimit);
        }
        let treasury = self.asset(call.asset)?.definition.treasury;
        let balances = self.contract_call_balances(
            call.asset,
            call.caller,
            call.contract.account(),
            treasury,
            call.attached_amount,
            call.fee,
            &execution.transfers,
        )?;
        let operation_id = AuthorizedOperation::ContractCall(call.clone()).operation_id()?;
        let receipt = self.create_receipt(
            operation_id,
            call.caller,
            call.idempotency_key,
            call.nonce,
            OperationKind::ContractCall,
            OperationOutcome::Applied,
        );
        let next_nonce = call
            .nonce
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let next_index = receipt
            .operation_index
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;

        for (account, balance) in balances {
            self.set_balance(call.asset, account, balance);
        }
        for (key, value) in execution.writes {
            if value == 0 {
                self.contract_state.remove(&(call.contract, key));
            } else {
                self.contract_state.insert((call.contract, key), value);
            }
        }
        self.nonces.insert(call.caller, next_nonce);
        self.next_operation_index = next_index;
        self.events.push(Event::ContractCalled {
            operation_id,
            contract: call.contract,
            caller: call.caller,
            entrypoint: call.entrypoint,
            attached_amount: call.attached_amount,
            execution_units: execution.units,
        });
        for topic in execution.topics {
            self.events.push(Event::ContractEmitted {
                operation_id,
                contract: call.contract,
                topic,
            });
        }
        Ok(receipt)
    }

    #[must_use]
    pub fn contract(&self, id: ContractId) -> Option<&ContractRecord> {
        self.contracts.get(&id)
    }

    #[must_use]
    pub fn contract_state(&self, id: ContractId, key: &[u8]) -> Option<u128> {
        self.contract_state.get(&(id, key.to_vec())).copied()
    }

    #[must_use]
    pub fn contract_state_entries(&self, id: ContractId) -> Vec<crate::ContractStateEntry> {
        self.contract_state
            .range((id, Vec::new())..)
            .take_while(|((candidate, _), _)| *candidate == id)
            .map(|((contract, key), value)| crate::ContractStateEntry {
                contract: *contract,
                key: key.clone(),
                value: *value,
            })
            .collect()
    }

    fn validate_contract_deploy(
        &self,
        origin: AccountId,
        deploy: &ContractDeploy,
    ) -> Result<(), LedgerError> {
        if deploy.network != self.network || origin != deploy.owner {
            return Err(if deploy.network == self.network {
                LedgerError::Unauthorized
            } else {
                LedgerError::WrongNetwork
            });
        }
        validate_code(&deploy.code)?;
        if self.contracts.contains_key(&contract_id(deploy)) {
            return Err(LedgerError::ContractAlreadyExists);
        }
        self.validate_contract_asset(deploy.asset, deploy.owner, deploy.fee)?;
        self.validate_nonce(deploy.owner, deploy.nonce)
    }

    fn validate_contract_call(
        &self,
        origin: AccountId,
        call: &ContractCall,
    ) -> Result<(), LedgerError> {
        if call.network != self.network || origin != call.caller {
            return Err(if call.network == self.network {
                LedgerError::Unauthorized
            } else {
                LedgerError::WrongNetwork
            });
        }
        if call.entrypoint.is_empty()
            || call.entrypoint.len() > 32
            || call.args.len() > crate::MAX_CONTRACT_ARGS_BYTES
        {
            return Err(LedgerError::InvalidContractArguments);
        }
        self.validate_contract_asset(
            call.asset,
            call.caller,
            checked_add(call.attached_amount, call.fee)?,
        )?;
        self.validate_nonce(call.caller, call.nonce)
    }

    fn validate_contract_asset(
        &self,
        asset: AssetId,
        sender: AccountId,
        required: Balance,
    ) -> Result<(), LedgerError> {
        let asset_state = self.asset(asset)?;
        if !asset_state.definition.status.allows_transfer() {
            return Err(LedgerError::AssetNotTransferable);
        }
        if has_backing_deficit(asset_state) {
            return Err(LedgerError::BackingDeficit);
        }
        validate_sender(self.account_status(asset, sender))?;
        if self.balance(asset, sender) < required {
            return Err(LedgerError::InsufficientBalance);
        }
        Ok(())
    }

    fn fee_balances(
        &self,
        asset: AssetId,
        owner: AccountId,
        treasury: AccountId,
        fee: Balance,
    ) -> Result<BTreeMap<AccountId, Balance>, LedgerError> {
        let mut balances = BTreeMap::from([
            (owner, self.balance(asset, owner)),
            (treasury, self.balance(asset, treasury)),
        ]);
        debit(&mut balances, owner, fee)?;
        credit(&mut balances, treasury, fee)?;
        Ok(balances)
    }

    #[allow(clippy::too_many_arguments)]
    fn contract_call_balances(
        &self,
        asset: AssetId,
        caller: AccountId,
        contract: AccountId,
        treasury: AccountId,
        attached: Balance,
        fee: Balance,
        transfers: &[(AccountId, Balance)],
    ) -> Result<BTreeMap<AccountId, Balance>, LedgerError> {
        let mut balances = BTreeMap::new();
        for account in [caller, contract, treasury] {
            balances.insert(account, self.balance(asset, account));
        }
        debit(&mut balances, caller, checked_add(attached, fee)?)?;
        credit(&mut balances, contract, attached)?;
        credit(&mut balances, treasury, fee)?;
        for (recipient, amount) in transfers {
            if *amount == 0 || *recipient == contract {
                return Err(LedgerError::InvalidAmount);
            }
            validate_recipient(self.account_status(asset, *recipient))?;
            balances
                .entry(*recipient)
                .or_insert_with(|| self.balance(asset, *recipient));
            debit(&mut balances, contract, *amount)?;
            credit(&mut balances, *recipient, *amount)?;
        }
        Ok(balances)
    }

    fn contract_values(&self, contract: ContractId) -> BTreeMap<Vec<u8>, u128> {
        self.contract_state
            .range((contract, Vec::new())..)
            .take_while(|((candidate, _), _)| *candidate == contract)
            .map(|((_, key), value)| (key.clone(), *value))
            .collect()
    }
}

fn validate_sender(status: AccountStatus) -> Result<(), LedgerError> {
    if status.allows_send() {
        Ok(())
    } else if status == AccountStatus::Frozen {
        Err(LedgerError::AccountFrozen)
    } else {
        Err(LedgerError::AccountCannotSend)
    }
}

fn validate_recipient(status: AccountStatus) -> Result<(), LedgerError> {
    if status.allows_receive() {
        Ok(())
    } else if status == AccountStatus::Frozen {
        Err(LedgerError::AccountFrozen)
    } else {
        Err(LedgerError::AccountCannotReceive)
    }
}

fn debit(
    balances: &mut BTreeMap<AccountId, Balance>,
    account: AccountId,
    amount: Balance,
) -> Result<(), LedgerError> {
    let current = balances.get(&account).copied().unwrap_or(0);
    balances.insert(
        account,
        current
            .checked_sub(amount)
            .ok_or(LedgerError::InsufficientBalance)?,
    );
    Ok(())
}

fn credit(
    balances: &mut BTreeMap<AccountId, Balance>,
    account: AccountId,
    amount: Balance,
) -> Result<(), LedgerError> {
    let current = balances.get(&account).copied().unwrap_or(0);
    balances.insert(account, checked_add(current, amount)?);
    Ok(())
}
