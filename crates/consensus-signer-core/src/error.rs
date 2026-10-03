#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsensusSignerError {
    WrongNetwork,
    ConflictingVote,
    JournalFailure,
    PayloadEncodingFailure,
    SignerFailure,
}
