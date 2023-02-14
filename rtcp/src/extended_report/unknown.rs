use super::*;

/// UnknownReportBlock is used to store bytes for any report block
/// that has an unknown Report Block Type.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct UnknownReportBlock {
    pub bytes: Bytes,
}

impl fmt::Display for UnknownReportBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl UnknownReportBlock {
    pub fn xr_header(&self) -> XRHeader {
        XRHeader {
            block_type: BlockType::Unknown,
            type_specific: 0,
            block_length: (self.raw_size() / 4 - 1) as u16,
        }
    }
}

impl Packet for UnknownReportBlock {
    fn header(&self) -> Header {
        Header::default()
    }

    /// destination_ssrc returns an array of ssrc values that this report block refers to.
    fn destination_ssrc(&self) -> Vec<u32> {
        vec![]
    }

    fn raw_size(&self) -> usize {
        XR_HEADER_LENGTH + self.bytes.len()
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn equal(&self, other: &(dyn Packet + Send + Sync)) -> bool {
        other
            .as_any()
            .downcast_ref::<UnknownReportBlock>()
            .map_or(false, |a| self == a)
    }
    fn cloned(&self) -> Box<dyn Packet + Send + Sync> {
        Box::new(self.clone())
    }
}

impl MarshalSize for UnknownReportBlock {
    fn marshal_size(&self) -> usize {
        self.raw_size()
    }
}

impl Marshal for UnknownReportBlock {
    /// marshal_to encodes the UnknownReportBlock in binary
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        if buf.remaining_mut() < self.marshal_size() {
            return Err(error::Error::BufferTooShort.into());
        }

        let h = self.xr_header();
        let n = h.marshal_to(buf)?;
        buf = &mut buf[n..];

        buf.put(self.bytes.clone());

        Ok(self.marshal_size())
    }
}

impl Unmarshal for UnknownReportBlock {
    /// Unmarshal decodes the UnknownReportBlock from binary
    fn unmarshal<B>(raw_packet: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        if raw_packet.remaining() < XR_HEADER_LENGTH {
            return Err(error::Error::PacketTooShort.into());
        }

        let xr_header = XRHeader::unmarshal(raw_packet)?;
        let block_length = xr_header.block_length * 4;
        if raw_packet.remaining() < block_length as usize {
            return Err(error::Error::PacketTooShort.into());
        }

        let bytes = raw_packet.copy_to_bytes(block_length as usize);

        Ok(UnknownReportBlock { bytes })
    }
}
