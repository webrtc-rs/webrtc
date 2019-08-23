use std::io::{Read, Write};
use utils::Error;

pub mod codecs;
pub mod packet;
pub mod packetizer;
pub mod sequence;

// Depacketizer depacketizes a RTP payload, removing any RTP specific data from the payload
//trait Depacketizer {
//fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error>;
//}
