#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmRpcError {
    InvalidEndpoint,
    ClientInitialization,
    WrongChain,
    InvalidBridgeContract,
    Transport,
    Rejected,
    ResponseTooLarge,
    InvalidResponse,
    RemoteError,
    MissingBlock,
}

impl std::fmt::Display for EvmRpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EvmRpcError {}
