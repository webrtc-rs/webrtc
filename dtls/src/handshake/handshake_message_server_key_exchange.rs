#[cfg(test)]
mod handshake_message_server_key_exchange_test;

use std::io::{Read, Write};

use byteorder::{BigEndian, WriteBytesExt};

use super::*;
use crate::curve::named_curve::*;
use crate::curve::*;
use crate::signature_hash_algorithm::*;

// Structure supports ECDH and PSK
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMessageServerKeyExchange {
    pub(crate) identity_hint: Vec<u8>,

    pub(crate) elliptic_curve_type: EllipticCurveType,
    pub(crate) named_curve: NamedCurve,
    pub(crate) public_key: Vec<u8>,
    pub(crate) algorithm: SignatureHashAlgorithm,
    pub(crate) signature: Vec<u8>,
}

impl HandshakeMessageServerKeyExchange {
    pub fn handshake_type(&self) -> HandshakeType {
        HandshakeType::ServerKeyExchange
    }

    pub fn size(&self) -> usize {
        if !self.identity_hint.is_empty() {
            2 + self.identity_hint.len()
        } else {
            1 + 2 + 1 + self.public_key.len() + 2 + 2 + self.signature.len()
        }
    }

    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        if !self.identity_hint.is_empty() {
            writer.write_u16::<BigEndian>(self.identity_hint.len() as u16)?;
            writer.write_all(&self.identity_hint)?;
            return Ok(writer.flush()?);
        }

        writer.write_u8(self.elliptic_curve_type as u8)?;
        writer.write_u16::<BigEndian>(self.named_curve as u16)?;

        writer.write_u8(self.public_key.len() as u8)?;
        writer.write_all(&self.public_key)?;

        writer.write_u8(self.algorithm.hash as u8)?;
        writer.write_u8(self.algorithm.signature as u8)?;

        writer.write_u16::<BigEndian>(self.signature.len() as u16)?;
        writer.write_all(&self.signature)?;

        Ok(writer.flush()?)
    }

    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut data = vec![];
        reader.read_to_end(&mut data)?;

        // If parsed as PSK return early and only populate PSK Identity Hint
        let psk_length = ((data[0] as u16) << 8) | data[1] as u16;
        if data.len() == psk_length as usize + 2 {
            return Ok(HandshakeMessageServerKeyExchange {
                identity_hint: data[2..].to_vec(),

                elliptic_curve_type: EllipticCurveType::Unsupported,
                named_curve: NamedCurve::Unsupported,
                public_key: vec![],
                algorithm: SignatureHashAlgorithm {
                    hash: HashAlgorithm::Unsupported,
                    signature: SignatureAlgorithm::Unsupported,
                },
                signature: vec![],
            });
        }

        let elliptic_curve_type = data[0].into();
        if data[1..].len() < 2 {
            return Err(Error::ErrBufferTooSmall);
        }

        let named_curve = (((data[1] as u16) << 8) | data[2] as u16).into();
        if data.len() < 4 {
            return Err(Error::ErrBufferTooSmall);
        }

        let public_key_length = data[3] as usize;
        let mut offset = 4 + public_key_length;
        if data.len() < offset {
            return Err(Error::ErrBufferTooSmall);
        }
        let public_key = data[4..offset].to_vec();
        if data.len() <= offset {
            return Err(Error::ErrBufferTooSmall);
        }

        let hash_algorithm = data[offset].into();
        offset += 1;
        if data.len() <= offset {
            return Err(Error::ErrBufferTooSmall);
        }

        let signature_algorithm = data[offset].into();
        offset += 1;
        if data.len() < offset + 2 {
            return Err(Error::ErrBufferTooSmall);
        }

        let signature_length = (((data[offset] as u16) << 8) | data[offset + 1] as u16) as usize;
        offset += 2;
        if data.len() < offset + signature_length {
            return Err(Error::ErrBufferTooSmall);
        }
        let signature = data[offset..offset + signature_length].to_vec();

        Ok(HandshakeMessageServerKeyExchange {
            identity_hint: vec![],

            elliptic_curve_type,
            named_curve,
            public_key,
            algorithm: SignatureHashAlgorithm {
                hash: hash_algorithm,
                signature: signature_algorithm,
            },
            signature,
        })
    }
}
