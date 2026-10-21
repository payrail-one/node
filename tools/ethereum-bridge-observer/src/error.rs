use ethereum_bridge_core::{
    EvmBlockObservationQuorumError, EvmDepositError, EvmFinalityQuorumError, EvmObserverError,
    EvmObserverIngestionError,
};
use ethereum_bridge_observer_lmdb::EvmObserverStoreError;

pub type IngestionError =
    EvmObserverIngestionError<EvmBlockObservationQuorumError, EvmFinalityQuorumError>;

#[derive(Debug)]
pub enum ObserverServiceError {
    InvalidArguments,
    MissingConfiguration(&'static str),
    InvalidConfiguration(&'static str),
    RpcInitialization,
    ObservationPolicy(EvmBlockObservationQuorumError),
    FinalityPolicy(EvmFinalityQuorumError),
    Store(EvmObserverStoreError),
    Observation(EvmBlockObservationQuorumError),
    Finality(EvmFinalityQuorumError),
    Deposit(EvmDepositError),
    Observer(EvmObserverError),
    EvidenceUnavailable,
    SettlementConfiguration,
    SettlementKey,
    SettlementCursor,
    SettlementRequest,
    SettlementRejected(u16),
    SettlementCodec,
}

impl ObserverServiceError {
    pub fn from_ingestion(error: &IngestionError) -> Self {
        match error {
            IngestionError::ObservationUnavailable | IngestionError::BlockNotFinalized => {
                Self::EvidenceUnavailable
            }
            IngestionError::Observation(error) => Self::Observation(*error),
            IngestionError::Finality(error) => Self::Finality(*error),
            IngestionError::Deposit(error) => Self::Deposit(*error),
            IngestionError::Observer(error) => Self::Observer(*error),
            IngestionError::InvalidBindingScope => {
                Self::InvalidConfiguration("asset binding outside observer scope")
            }
            IngestionError::DuplicateOriginAsset => {
                Self::InvalidConfiguration("duplicate Ethereum origin asset")
            }
            IngestionError::DuplicatePayrailAsset => {
                Self::InvalidConfiguration("duplicate Payrail asset")
            }
            IngestionError::FinalizedBlockMismatch => {
                Self::InvalidConfiguration("observation and finality block mismatch")
            }
            IngestionError::UnknownAsset => Self::InvalidConfiguration("unbound deposit asset"),
        }
    }
}

impl std::fmt::Display for ObserverServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingConfiguration(name) => write!(formatter, "missing configuration: {name}"),
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid configuration: {reason}")
            }
            Self::InvalidArguments => {
                formatter.write_str("expected `scan-once`, `settle-deposits` or `run`")
            }
            Self::RpcInitialization => formatter.write_str("RPC client initialization failed"),
            Self::ObservationPolicy(error) => {
                write!(formatter, "invalid observation policy: {error:?}")
            }
            Self::FinalityPolicy(error) => write!(formatter, "invalid finality policy: {error:?}"),
            Self::Store(error) => write!(formatter, "observer store failed: {error}"),
            Self::Observation(error) => write!(formatter, "observation quorum failed: {error:?}"),
            Self::Finality(error) => write!(formatter, "finality quorum failed: {error:?}"),
            Self::Deposit(error) => write!(formatter, "deposit validation failed: {error:?}"),
            Self::Observer(error) => write!(formatter, "observer transition failed: {error:?}"),
            Self::EvidenceUnavailable => {
                formatter.write_str("quorum evidence is not available yet")
            }
            Self::SettlementConfiguration => {
                formatter.write_str("settlement configuration is invalid")
            }
            Self::SettlementKey => formatter.write_str("settlement authority key is invalid"),
            Self::SettlementCursor => formatter.write_str("settlement cursor is invalid"),
            Self::SettlementRequest => formatter.write_str("settlement request failed"),
            Self::SettlementRejected(status) => {
                write!(
                    formatter,
                    "settlement request was rejected with HTTP {status}"
                )
            }
            Self::SettlementCodec => formatter.write_str("settlement payload encoding failed"),
        }
    }
}

impl std::error::Error for ObserverServiceError {}

impl From<EvmObserverStoreError> for ObserverServiceError {
    fn from(error: EvmObserverStoreError) -> Self {
        Self::Store(error)
    }
}
