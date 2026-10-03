#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AccountStatus {
    #[default]
    Active,
    SendOnly,
    ReceiveOnly,
    Frozen,
}

impl AccountStatus {
    pub(crate) const fn allows_send(self) -> bool {
        matches!(self, Self::Active | Self::SendOnly)
    }

    pub(crate) const fn allows_receive(self) -> bool {
        matches!(self, Self::Active | Self::ReceiveOnly)
    }
}
