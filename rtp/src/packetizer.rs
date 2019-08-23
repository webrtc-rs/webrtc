use crate::packet::*;
use std::io::{Read, Write};
use utils::Error;

// Payloader payloads a byte array for use as rtp.Packet payloads
trait Payloader {
    fn payload<R: Read>(mtu: isize, reader: &mut R); //TODO: [][]byte
}

// Packetizer packetizes a payload
trait Packetizer {
    fn packetize<R: Read>(reader: &mut R, samples: u32) -> Vec<Packet>;
    fn enable_abs_send_time(value: isize);
}

/*
struct packetizer  {
    mtu:              isize,
    payload_type :     u8,
    ssrc          :   u32,
    payloader      :  dyn Payloader,
    sequencer       : Sequencer,
    timestamp       : u32,
    clock_rate       : u32,
    extensionNumbers struct { //put extension numbers in here. If they're 0, the extension is disabled (0 is not a legal extension number)
        AbsSendTime int //http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
    }
    timegen: func() time.Time,
}
*/
