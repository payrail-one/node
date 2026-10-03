use std::collections::BTreeMap;

use crate::{AccountId, Balance, LedgerError};

use super::{MAX_CONTRACT_CODE_BYTES, MAX_CONTRACT_EXECUTION_UNITS};

const MAGIC: &[u8; 4] = b"PRC1";
const MAX_ENTRYPOINTS: usize = 16;
const MAX_ENTRYPOINT_BYTES: usize = 32;
const MAX_INSTRUCTIONS: usize = 1_024;
const MAX_STACK_ITEMS: usize = 64;
const MAX_STATE_KEY_BYTES: usize = 32;
const MAX_WRITES: usize = 32;
const MAX_EVENTS: usize = 16;

type EntrypointSlices<'a> = Vec<(&'a [u8], &'a [u8])>;

pub(crate) struct ContractExecution {
    pub writes: BTreeMap<Vec<u8>, u128>,
    pub transfers: Vec<(AccountId, Balance)>,
    pub topics: Vec<Vec<u8>>,
    pub units: u64,
}

pub(crate) fn validate_code(code: &[u8]) -> Result<(), LedgerError> {
    if code.len() > MAX_CONTRACT_CODE_BYTES || code.len() < MAGIC.len() + 1 {
        return Err(LedgerError::InvalidContractCode);
    }
    let entries = entries(code)?;
    if entries.is_empty() || entries.len() > MAX_ENTRYPOINTS {
        return Err(LedgerError::InvalidContractCode);
    }
    for (_, body) in entries {
        validate_body(body)?;
    }
    Ok(())
}

pub(crate) fn execute(
    code: &[u8],
    entrypoint: &str,
    args: &[u8],
    attached_amount: Balance,
    caller: AccountId,
    state: &BTreeMap<Vec<u8>, u128>,
    execution_limit: u64,
) -> Result<ContractExecution, LedgerError> {
    if execution_limit == 0 || execution_limit > MAX_CONTRACT_EXECUTION_UNITS {
        return Err(LedgerError::InvalidExecutionLimit);
    }
    let body = entries(code)?
        .into_iter()
        .find_map(|(name, body)| (name == entrypoint.as_bytes()).then_some(body))
        .ok_or(LedgerError::ContractEntrypointNotFound)?;
    let machine = Machine {
        body,
        args,
        attached_amount,
        caller,
        state,
        stack: Vec::new(),
        writes: BTreeMap::new(),
        transfers: Vec::new(),
        topics: Vec::new(),
        cursor: 0,
        units: 0,
        execution_limit,
    };
    machine.run()
}

struct Machine<'a> {
    body: &'a [u8],
    args: &'a [u8],
    attached_amount: Balance,
    caller: AccountId,
    state: &'a BTreeMap<Vec<u8>, u128>,
    stack: Vec<u128>,
    writes: BTreeMap<Vec<u8>, u128>,
    transfers: Vec<(AccountId, Balance)>,
    topics: Vec<Vec<u8>>,
    cursor: usize,
    units: u64,
    execution_limit: u64,
}

impl Machine<'_> {
    fn run(mut self) -> Result<ContractExecution, LedgerError> {
        loop {
            self.charge()?;
            match self.byte()? {
                0x00 => {
                    if self.cursor != self.body.len() {
                        return Err(LedgerError::InvalidContractCode);
                    }
                    return Ok(ContractExecution {
                        writes: self.writes,
                        transfers: self.transfers,
                        topics: self.topics,
                        units: self.units,
                    });
                }
                0x01 => {
                    let value = u128::from_be_bytes(array(self.take(16)?)?);
                    self.push(value)?;
                }
                0x02 => {
                    let offset = usize::from(self.u16()?);
                    let bytes = self
                        .args
                        .get(offset..offset.saturating_add(16))
                        .ok_or(LedgerError::InvalidContractArguments)?;
                    self.push(u128::from_be_bytes(array(bytes)?))?;
                }
                0x03 => {
                    let key = self.key()?;
                    let value = self
                        .writes
                        .get(&key)
                        .or_else(|| self.state.get(&key))
                        .copied()
                        .unwrap_or(0);
                    self.push(value)?;
                }
                0x04 => self.push(self.attached_amount)?,
                0x05 => self.binary(u128::checked_add)?,
                0x06 => self.binary(u128::checked_sub)?,
                0x07 => self.compare(|left, right| left == right)?,
                0x08 => self.compare(|left, right| left <= right)?,
                0x09 => self.compare(|left, right| left >= right)?,
                0x0a => {
                    let reason = self.u16()?;
                    if self.pop()? == 0 {
                        return Err(LedgerError::ContractRejected(reason));
                    }
                }
                0x0b => {
                    let key = self.key()?;
                    let value = self.pop()?;
                    if !self.writes.contains_key(&key) && self.writes.len() >= MAX_WRITES {
                        return Err(LedgerError::ContractStateLimit);
                    }
                    self.writes.insert(key, value);
                }
                0x0c => {
                    let topic = self.key()?;
                    if self.topics.len() >= MAX_EVENTS {
                        return Err(LedgerError::ContractEventLimit);
                    }
                    self.topics.push(topic);
                }
                0x0d => {
                    let value = *self
                        .stack
                        .last()
                        .ok_or(LedgerError::ContractStackUnderflow)?;
                    self.push(value)?;
                }
                0x0e => {
                    self.pop()?;
                }
                0x0f => {
                    let amount = self.pop()?;
                    self.transfers.push((self.caller, amount));
                }
                0x10 => {
                    let offset = usize::from(self.u16()?);
                    let bytes = self
                        .args
                        .get(offset..offset.saturating_add(32))
                        .ok_or(LedgerError::InvalidContractArguments)?;
                    let account = AccountId::new(array(bytes)?);
                    let amount = self.pop()?;
                    self.transfers.push((account, amount));
                }
                _ => return Err(LedgerError::InvalidContractCode),
            }
        }
    }

    fn charge(&mut self) -> Result<(), LedgerError> {
        self.units = self
            .units
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        if self.units > self.execution_limit {
            return Err(LedgerError::ContractExecutionLimit);
        }
        Ok(())
    }

    fn push(&mut self, value: u128) -> Result<(), LedgerError> {
        if self.stack.len() >= MAX_STACK_ITEMS {
            return Err(LedgerError::ContractStackOverflow);
        }
        self.stack.push(value);
        Ok(())
    }

    fn pop(&mut self) -> Result<u128, LedgerError> {
        self.stack.pop().ok_or(LedgerError::ContractStackUnderflow)
    }

    fn binary(&mut self, operation: fn(u128, u128) -> Option<u128>) -> Result<(), LedgerError> {
        let right = self.pop()?;
        let left = self.pop()?;
        self.push(operation(left, right).ok_or(LedgerError::ArithmeticOverflow)?)
    }

    fn compare(&mut self, predicate: fn(u128, u128) -> bool) -> Result<(), LedgerError> {
        let right = self.pop()?;
        let left = self.pop()?;
        self.push(u128::from(predicate(left, right)))
    }

    fn byte(&mut self) -> Result<u8, LedgerError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, LedgerError> {
        Ok(u16::from_be_bytes(array(self.take(2)?)?))
    }

    fn key(&mut self) -> Result<Vec<u8>, LedgerError> {
        let length = usize::from(self.byte()?);
        if length == 0 || length > MAX_STATE_KEY_BYTES {
            return Err(LedgerError::InvalidContractCode);
        }
        Ok(self.take(length)?.to_vec())
    }

    fn take(&mut self, length: usize) -> Result<&[u8], LedgerError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(LedgerError::InvalidContractCode)?;
        let bytes = self
            .body
            .get(self.cursor..end)
            .ok_or(LedgerError::InvalidContractCode)?;
        self.cursor = end;
        Ok(bytes)
    }
}

fn entries(code: &[u8]) -> Result<EntrypointSlices<'_>, LedgerError> {
    if code.get(..4) != Some(MAGIC) {
        return Err(LedgerError::InvalidContractCode);
    }
    let count = usize::from(*code.get(4).ok_or(LedgerError::InvalidContractCode)?);
    let mut cursor = 5;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let name_length = usize::from(*code.get(cursor).ok_or(LedgerError::InvalidContractCode)?);
        cursor += 1;
        if name_length == 0 || name_length > MAX_ENTRYPOINT_BYTES {
            return Err(LedgerError::InvalidContractCode);
        }
        let name = slice(code, &mut cursor, name_length)?;
        if !valid_name(name) || result.iter().any(|(existing, _)| *existing == name) {
            return Err(LedgerError::InvalidContractCode);
        }
        let body_length = usize::from(u16::from_be_bytes(array(slice(code, &mut cursor, 2)?)?));
        let body = slice(code, &mut cursor, body_length)?;
        result.push((name, body));
    }
    if cursor != code.len() {
        return Err(LedgerError::InvalidContractCode);
    }
    Ok(result)
}

fn validate_body(body: &[u8]) -> Result<(), LedgerError> {
    let mut cursor = 0;
    let mut instructions = 0;
    while cursor < body.len() {
        instructions += 1;
        if instructions > MAX_INSTRUCTIONS {
            return Err(LedgerError::InvalidContractCode);
        }
        let opcode = body[cursor];
        cursor += 1;
        let operand = match opcode {
            0x00 => {
                if cursor != body.len() {
                    return Err(LedgerError::InvalidContractCode);
                }
                return Ok(());
            }
            0x01 => 16,
            0x02 | 0x0a | 0x10 => 2,
            0x03 | 0x0b | 0x0c => {
                let length =
                    usize::from(*body.get(cursor).ok_or(LedgerError::InvalidContractCode)?);
                if length == 0 || length > MAX_STATE_KEY_BYTES {
                    return Err(LedgerError::InvalidContractCode);
                }
                1 + length
            }
            0x04..=0x09 | 0x0d..=0x0f => 0,
            _ => return Err(LedgerError::InvalidContractCode),
        };
        cursor = cursor
            .checked_add(operand)
            .filter(|end| *end <= body.len())
            .ok_or(LedgerError::InvalidContractCode)?;
    }
    Err(LedgerError::InvalidContractCode)
}

fn valid_name(value: &[u8]) -> bool {
    value.first().is_some_and(u8::is_ascii_lowercase)
        && value
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

fn slice<'a>(input: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], LedgerError> {
    let end = cursor
        .checked_add(length)
        .ok_or(LedgerError::InvalidContractCode)?;
    let value = input
        .get(*cursor..end)
        .ok_or(LedgerError::InvalidContractCode)?;
    *cursor = end;
    Ok(value)
}

fn array<const N: usize>(input: &[u8]) -> Result<[u8; N], LedgerError> {
    input
        .try_into()
        .map_err(|_| LedgerError::InvalidContractCode)
}
