use crate::ProtocolError;

pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    pub(crate) const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    pub(crate) fn read_array<const SIZE: usize>(&mut self) -> Result<[u8; SIZE], ProtocolError> {
        let bytes = self.read_slice(SIZE)?;
        let mut output = [0_u8; SIZE];
        output.copy_from_slice(bytes);
        Ok(output)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.read_array::<1>()?[0])
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn read_u128(&mut self) -> Result<u128, ProtocolError> {
        Ok(u128::from_be_bytes(self.read_array()?))
    }

    pub(crate) fn finish(self) -> Result<(), ProtocolError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(ProtocolError::TrailingBytes)
        }
    }

    pub(crate) fn read_slice(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ProtocolError::UnexpectedEnd)?;
        let bytes = self
            .input
            .get(self.position..end)
            .ok_or(ProtocolError::UnexpectedEnd)?;
        self.position = end;
        Ok(bytes)
    }
}
