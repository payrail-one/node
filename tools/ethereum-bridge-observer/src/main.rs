#![forbid(unsafe_code)]

mod config;
mod error;
mod settlement;

use std::{env, process::ExitCode, thread};

use ethereum_bridge_core::{
    EvmBlockObservationQuorum, EvmFinalityQuorum, ingest_next_observer_block,
};
use ethereum_bridge_observer_lmdb::{EvmObserverCommitOutcome, LmdbEvmObserverStore};

use crate::{config::ObserverConfig, error::ObserverServiceError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    ScanOnce,
    SettleDeposits,
    Run,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanOutcome {
    Committed {
        block_number: u64,
        deposits: usize,
        existing: bool,
    },
    Pending,
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ethereum bridge observer: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), ObserverServiceError> {
    let command = command(env::args_os().skip(1))?;
    let config = ObserverConfig::from_environment()?;
    let store = LmdbEvmObserverStore::open(&config.store_path, config.scope, config.store_options)?;
    if command == Command::SettleDeposits {
        let settled = settlement::settle_deposits(&store)?;
        println!("settled_deposits={settled}");
        return Ok(());
    }
    let observation =
        EvmBlockObservationQuorum::new(config.providers.clone(), config.minimum_agreement)
            .map_err(ObserverServiceError::ObservationPolicy)?;
    let finality = EvmFinalityQuorum::new(config.providers.clone(), config.minimum_agreement)
        .map_err(ObserverServiceError::FinalityPolicy)?;

    loop {
        match scan(&config, &store, &observation, &finality)? {
            ScanOutcome::Committed {
                block_number,
                deposits,
                existing,
            } => println!(
                "block={block_number} deposits={deposits} outcome={}",
                if existing { "existing" } else { "committed" }
            ),
            ScanOutcome::Pending => {
                if command == Command::ScanOnce {
                    println!("outcome=pending");
                }
            }
        }
        if command == Command::ScanOnce {
            return Ok(());
        }
        thread::sleep(config.poll_interval);
    }
}

fn scan(
    config: &ObserverConfig,
    store: &LmdbEvmObserverStore,
    observation: &EvmBlockObservationQuorum<ethereum_bridge_rpc::RpcEvmFinalityProvider>,
    finality: &EvmFinalityQuorum<ethereum_bridge_rpc::RpcEvmFinalityProvider>,
) -> Result<ScanOutcome, ObserverServiceError> {
    let checkpoint = store.checkpoint()?;
    let prepared = match ingest_next_observer_block(
        config.scope,
        checkpoint,
        &config.bindings,
        observation,
        finality,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            let mapped = ObserverServiceError::from_ingestion(&error);
            return if matches!(mapped, ObserverServiceError::EvidenceUnavailable) {
                Ok(ScanOutcome::Pending)
            } else {
                Err(mapped)
            };
        }
    };
    let block_number = prepared.finalized().number;
    let deposits = prepared.deposits().len();
    let existing = store.commit(&prepared)? == EvmObserverCommitOutcome::ExistingSame;
    Ok(ScanOutcome::Committed {
        block_number,
        deposits,
        existing,
    })
}

fn command(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Command, ObserverServiceError> {
    let first = arguments
        .next()
        .ok_or(ObserverServiceError::InvalidArguments)?;
    if arguments.next().is_some() {
        return Err(ObserverServiceError::InvalidArguments);
    }
    match first.to_str() {
        Some("scan-once") => Ok(Command::ScanOnce),
        Some("settle-deposits") => Ok(Command::SettleDeposits),
        Some("run") => Ok(Command::Run),
        _ => Err(ObserverServiceError::InvalidArguments),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Command, command};

    #[test]
    fn command_is_explicit_and_rejects_extra_arguments() {
        assert_eq!(
            command([OsString::from("scan-once")].into_iter()).unwrap(),
            Command::ScanOnce
        );
        assert_eq!(
            command([OsString::from("settle-deposits")].into_iter()).unwrap(),
            Command::SettleDeposits
        );
        assert_eq!(
            command([OsString::from("run")].into_iter()).unwrap(),
            Command::Run
        );
        assert!(command([].into_iter()).is_err());
        assert!(command([OsString::from("run"), OsString::from("extra")].into_iter()).is_err());
    }
}
