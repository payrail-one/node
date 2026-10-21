#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmDepositError {
    ZeroBridgeContract,
    ZeroTokenContract,
    RemovedLog,
    WrongBridgeContract,
    WrongTopicCount,
    WrongEventSignature,
    InvalidTokenTopic,
    WrongOriginAsset,
    InvalidDataLength,
    AmountOverflow,
    InvalidAmount,
    InvalidRecipient,
    MissingTransactionHash,
    InvalidTransactionHash,
    MissingBlockHash,
    InvalidBlockHash,
    MissingBlockNumber,
    MissingLogIndex,
    LogIndexOverflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvmDepositVerificationError<SourceError> {
    Deposit(EvmDepositError),
    Source(SourceError),
    BlockNotFinalized,
    FinalizedBlockNumberMismatch,
    NonCanonicalBlock,
    MissingFinalityProof,
}
