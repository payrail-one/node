use crate::ExternalAssetStateCodecError;

pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, ExternalAssetStateCodecError> {
        self.read_array::<1>().map(|bytes| bytes[0])
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, ExternalAssetStateCodecError> {
        self.read_array().map(u32::from_be_bytes)
    }

    pub(crate) fn read_u64(&mut self) -> Result<u64, ExternalAssetStateCodecError> {
        self.read_array().map(u64::from_be_bytes)
    }

    pub(crate) fn read_u128(&mut self) -> Result<u128, ExternalAssetStateCodecError> {
        self.read_array().map(u128::from_be_bytes)
    }

    pub(crate) fn read_array<const LENGTH: usize>(
        &mut self,
    ) -> Result<[u8; LENGTH], ExternalAssetStateCodecError> {
        self.read_slice(LENGTH)?
            .try_into()
            .map_err(|_| ExternalAssetStateCodecError::UnexpectedEnd)
    }

    pub(crate) fn read_slice(
        &mut self,
        length: usize,
    ) -> Result<&'a [u8], ExternalAssetStateCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ExternalAssetStateCodecError::UnexpectedEnd)?;
        let value = self
            .input
            .get(self.offset..end)
            .ok_or(ExternalAssetStateCodecError::UnexpectedEnd)?;
        self.offset = end;
        Ok(value)
    }

    pub(crate) fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.offset)
    }

    pub(crate) fn finish(self) -> Result<(), ExternalAssetStateCodecError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(ExternalAssetStateCodecError::TrailingBytes)
        }
    }
}
