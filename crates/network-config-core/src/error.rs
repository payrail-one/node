#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkConfigError {
    InvalidLength,
    InvalidDomain,
    UnsupportedValue,
    ZeroProtocolVersion,
    ZeroNetwork,
    ZeroConfigurationKey,
    ZeroGenesisBlockHash,
    ZeroGenesisStateRoot,
    ZeroValidatorSetHash,
    ActivationBeforeGenesis,
    WrongNetwork,
    UntrustedConfigurationKey,
    InvalidSignature,
    GenesisStateMismatch,
}
