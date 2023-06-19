pub mod record_layer_header;

#[cfg(test)]
mod record_layer_test;

use std::io::{Read, Write};

use record_layer_header::*;

use super::content::*;
use super::error::*;
use crate::alert::Alert;
use crate::application_data::ApplicationData;
use crate::change_cipher_spec::ChangeCipherSpec;
use crate::handshake::Handshake;

/*
 The TLS Record Layer which handles all data transport.
 The record layer is assumed to sit directly on top of some
 reliable transport such as TCP. The record layer can carry four types of content:

 1. Handshake messages—used for algorithm negotiation and key establishment.
 2. ChangeCipherSpec messages—really part of the handshake but technically a separate kind of message.
 3. Alert messages—used to signal that errors have occurred
 4. Application layer data

 The DTLS record layer is extremely similar to that of TLS 1.1.  The
 only change is the inclusion of an explicit sequence number in the
 record.  This sequence number allows the recipient to correctly
 verify the TLS MAC.
 https://tools.ietf.org/html/rfc4347#section-4.1
*/
#[derive(Debug, Clone, PartialEq)]
pub struct RecordLayer {
    pub record_layer_header: RecordLayerHeader,
    pub content: Content,
}

impl RecordLayer {
    pub fn new(protocol_version: ProtocolVersion, epoch: u16, content: Content) -> Self {
        RecordLayer {
            record_layer_header: RecordLayerHeader {
                content_type: content.content_type(),
                protocol_version,
                epoch,
                sequence_number: 0,
                content_len: content.size() as u16,
            },
            content,
        }
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        self.record_layer_header.marshal(writer)?;
        self.content.marshal(writer)?;
        Ok(())
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let record_layer_header = RecordLayerHeader::unmarshal(reader)?;
        let content = match record_layer_header.content_type {
            ContentType::Alert => Content::Alert(Alert::unmarshal(reader)?),
            ContentType::ApplicationData => {
                Content::ApplicationData(ApplicationData::unmarshal(reader)?)
            }
            ContentType::ChangeCipherSpec => {
                Content::ChangeCipherSpec(ChangeCipherSpec::unmarshal(reader)?)
            }
            ContentType::Handshake => Content::Handshake(Handshake::unmarshal(reader)?),
            _ => return Err(Error::Other("Invalid Content Type".to_owned())),
        };

        Ok(RecordLayer {
            record_layer_header,
            content,
        })
    }
}

// Note that as with TLS, multiple handshake messages may be placed in
// the same DTLS record, provided that there is room and that they are
// part of the same flight.  Thus, there are two acceptable ways to pack
// two DTLS messages into the same datagram: in the same record or in
// separate records.
// https://tools.ietf.org/html/rfc6347#section-4.2.3
pub(crate) fn unpack_datagram(buf: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut out = vec![];

    let mut offset = 0;
    while buf.len() != offset {
        if buf.len() - offset <= RECORD_LAYER_HEADER_SIZE {
            return Err(Error::ErrInvalidPacketLength);
        }

        let pkt_len = RECORD_LAYER_HEADER_SIZE
            + (((buf[offset + RECORD_LAYER_HEADER_SIZE - 2] as usize) << 8)
                | buf[offset + RECORD_LAYER_HEADER_SIZE - 1] as usize);
        if offset + pkt_len > buf.len() {
            return Err(Error::ErrInvalidPacketLength);
        }

        out.push(buf[offset..offset + pkt_len].to_vec());
        offset += pkt_len
    }

    Ok(out)
}
