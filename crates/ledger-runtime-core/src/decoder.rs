use crate::RuntimeError;

pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    pub(crate) fn read_array<const SIZE: usize>(&mut self) -> Result<[u8; SIZE], RuntimeError> {
        let mut output = [0_u8; SIZE];
        output.copy_from_slice(self.read_slice(SIZE)?);
        Ok(output)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, RuntimeError> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, RuntimeError> {
        Ok(u16::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, RuntimeError> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u128(&mut self) -> Result<u128, RuntimeError> {
        Ok(u128::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_slice(&mut self, length: usize) -> Result<&'a [u8], RuntimeError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(RuntimeError::UnexpectedEnd)?;
        let bytes = self
            .input
            .get(self.position..end)
            .ok_or(RuntimeError::UnexpectedEnd)?;
        self.position = end;
        Ok(bytes)
    }

    pub(crate) fn finish(self) -> Result<(), RuntimeError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(RuntimeError::TrailingBytes)
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.position)
    }
}
