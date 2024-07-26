#[cfg(test)]
mod twcc_test;

pub mod receiver;
pub mod sender;

use std::cmp::Ordering;

use rtcp::transport_feedbacks::transport_layer_cc::{
    PacketStatusChunk, RecvDelta, RunLengthChunk, StatusChunkTypeTcc, StatusVectorChunk,
    SymbolSizeTypeTcc, SymbolTypeTcc, TransportLayerCc,
};

#[derive(Default, Debug, PartialEq, Clone)]
struct PktInfo {
    sequence_number: u32,
    arrival_time: i64,
}

/// Recorder records incoming RTP packets and their delays and creates
/// transport wide congestion control feedback reports as specified in
/// <https://datatracker.ietf.org/doc/html/draft-holmer-rmcat-transport-wide-cc-extensions-01>
#[derive(Default, Debug, PartialEq, Clone)]
pub struct Recorder {
    received_packets: Vec<PktInfo>,

    cycles: u32,
    last_sequence_number: u16,

    sender_ssrc: u32,
    media_ssrc: u32,
    fb_pkt_cnt: u8,
}

impl Recorder {
    /// new creates a new Recorder which uses the given sender_ssrc in the created
    /// feedback packets.
    pub fn new(sender_ssrc: u32) -> Self {
        Recorder {
            sender_ssrc,
            ..Default::default()
        }
    }

    /// record marks a packet with media_ssrc and a transport wide sequence number sequence_number as received at arrival_time.
    pub fn record(&mut self, media_ssrc: u32, sequence_number: u16, arrival_time: i64) {
        self.media_ssrc = media_ssrc;
        if sequence_number < 0x0fff && self.last_sequence_number > 0xf000 {
            self.cycles += 1 << 16;
        }
        self.received_packets.push(PktInfo {
            sequence_number: self.cycles | sequence_number as u32,
            arrival_time,
        });
        self.last_sequence_number = sequence_number;
    }

    /// build_feedback_packet creates a new RTCP packet containing a TWCC feedback report.
    pub fn build_feedback_packet(&mut self) -> Vec<Box<dyn rtcp::packet::Packet + Send + Sync>> {
        if self.received_packets.len() < 2 {
            return vec![];
        }
        let mut feedback = Feedback::new(self.sender_ssrc, self.media_ssrc, self.fb_pkt_cnt);
        self.fb_pkt_cnt = self.fb_pkt_cnt.wrapping_add(1);

        self.received_packets
            .sort_by(|a: &PktInfo, b: &PktInfo| -> Ordering {
                a.sequence_number.cmp(&b.sequence_number)
            });
        feedback.set_base(
            (self.received_packets[0].sequence_number & 0xffff) as u16,
            self.received_packets[0].arrival_time,
        );

        let mut pkts = vec![];
        for pkt in &self.received_packets {
            let built =
                feedback.add_received((pkt.sequence_number & 0xffff) as u16, pkt.arrival_time);
            if !built {
                let p: Box<dyn rtcp::packet::Packet + Send + Sync> = Box::new(feedback.get_rtcp());
                pkts.push(p);
                feedback = Feedback::new(self.sender_ssrc, self.media_ssrc, self.fb_pkt_cnt);
                self.fb_pkt_cnt = self.fb_pkt_cnt.wrapping_add(1);
                feedback.add_received((pkt.sequence_number & 0xffff) as u16, pkt.arrival_time);
            }
        }
        self.received_packets.clear();
        let p: Box<dyn rtcp::packet::Packet + Send + Sync> = Box::new(feedback.get_rtcp());
        pkts.push(p);
        pkts
    }
}

#[derive(Default, Debug, PartialEq, Clone)]
struct Feedback {
    rtcp: TransportLayerCc,
    base_sequence_number: u16,
    ref_timestamp64ms: i64,
    last_timestamp_us: i64,
    next_sequence_number: u16,
    sequence_number_count: u16,
    len: usize,
    last_chunk: Chunk,
    chunks: Vec<PacketStatusChunk>,
    deltas: Vec<RecvDelta>,
}

impl Feedback {
    fn new(sender_ssrc: u32, media_ssrc: u32, fb_pkt_count: u8) -> Self {
        Feedback {
            rtcp: TransportLayerCc {
                sender_ssrc,
                media_ssrc,
                fb_pkt_count,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn set_base(&mut self, sequence_number: u16, time_us: i64) {
        self.base_sequence_number = sequence_number;
        self.next_sequence_number = self.base_sequence_number;
        self.ref_timestamp64ms = time_us / 64000;
        self.last_timestamp_us = self.ref_timestamp64ms * 64000;
    }

    fn get_rtcp(&mut self) -> TransportLayerCc {
        self.rtcp.packet_status_count = self.sequence_number_count;
        self.rtcp.reference_time = self.ref_timestamp64ms as u32;
        self.rtcp.base_sequence_number = self.base_sequence_number;
        while !self.last_chunk.deltas.is_empty() {
            self.chunks.push(self.last_chunk.encode());
        }
        self.rtcp.packet_chunks.extend_from_slice(&self.chunks);
        self.rtcp.recv_deltas.clone_from(&self.deltas);

        self.rtcp.clone()
    }

    fn add_received(&mut self, sequence_number: u16, timestamp_us: i64) -> bool {
        let delta_us = timestamp_us - self.last_timestamp_us;
        let delta250us = delta_us / 250;
        if delta250us < i16::MIN as i64 || delta250us > i16::MAX as i64 {
            // delta doesn't fit into 16 bit, need to create new packet
            return false;
        }

        while self.next_sequence_number != sequence_number {
            if !self
                .last_chunk
                .can_add(SymbolTypeTcc::PacketNotReceived as u16)
            {
                self.chunks.push(self.last_chunk.encode());
            }
            self.last_chunk.add(SymbolTypeTcc::PacketNotReceived as u16);
            self.sequence_number_count = self.sequence_number_count.wrapping_add(1);
            self.next_sequence_number = self.next_sequence_number.wrapping_add(1);
        }

        let recv_delta = if (0..=0xff).contains(&delta250us) {
            self.len += 1;
            SymbolTypeTcc::PacketReceivedSmallDelta
        } else {
            self.len += 2;
            SymbolTypeTcc::PacketReceivedLargeDelta
        };

        if !self.last_chunk.can_add(recv_delta as u16) {
            self.chunks.push(self.last_chunk.encode());
        }
        self.last_chunk.add(recv_delta as u16);
        self.deltas.push(RecvDelta {
            type_tcc_packet: recv_delta,
            delta: delta_us,
        });
        self.last_timestamp_us = timestamp_us;
        self.sequence_number_count = self.sequence_number_count.wrapping_add(1);
        self.next_sequence_number = self.next_sequence_number.wrapping_add(1);
        true
    }
}

const MAX_RUN_LENGTH_CAP: usize = 0x1fff; // 13 bits
const MAX_ONE_BIT_CAP: usize = 14; // bits
const MAX_TWO_BIT_CAP: usize = 7; // bits

#[derive(Default, Debug, PartialEq, Clone)]
struct Chunk {
    has_large_delta: bool,
    has_different_types: bool,
    deltas: Vec<u16>,
}

impl Chunk {
    fn can_add(&self, delta: u16) -> bool {
        if self.deltas.len() < MAX_TWO_BIT_CAP {
            return true;
        }
        if self.deltas.len() < MAX_ONE_BIT_CAP
            && !self.has_large_delta
            && delta != SymbolTypeTcc::PacketReceivedLargeDelta as u16
        {
            return true;
        }
        if self.deltas.len() < MAX_RUN_LENGTH_CAP
            && !self.has_different_types
            && delta == self.deltas[0]
        {
            return true;
        }
        false
    }

    fn add(&mut self, delta: u16) {
        self.deltas.push(delta);
        self.has_large_delta =
            self.has_large_delta || delta == SymbolTypeTcc::PacketReceivedLargeDelta as u16;
        self.has_different_types = self.has_different_types || delta != self.deltas[0];
    }

    fn encode(&mut self) -> PacketStatusChunk {
        if !self.has_different_types {
            let p = PacketStatusChunk::RunLengthChunk(RunLengthChunk {
                type_tcc: StatusChunkTypeTcc::RunLengthChunk,
                packet_status_symbol: self.deltas[0].into(),
                run_length: self.deltas.len() as u16,
            });
            self.reset();
            return p;
        }
        if self.deltas.len() == MAX_ONE_BIT_CAP {
            let p = PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
                type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
                symbol_size: SymbolSizeTypeTcc::OneBit,
                symbol_list: self
                    .deltas
                    .iter()
                    .map(|x| SymbolTypeTcc::from(*x))
                    .collect::<Vec<SymbolTypeTcc>>(),
            });
            self.reset();
            return p;
        }

        let min_cap = std::cmp::min(MAX_TWO_BIT_CAP, self.deltas.len());
        let svc = PacketStatusChunk::StatusVectorChunk(StatusVectorChunk {
            type_tcc: StatusChunkTypeTcc::StatusVectorChunk,
            symbol_size: SymbolSizeTypeTcc::TwoBit,
            symbol_list: self.deltas[..min_cap]
                .iter()
                .map(|x| SymbolTypeTcc::from(*x))
                .collect::<Vec<SymbolTypeTcc>>(),
        });
        self.deltas.drain(..min_cap);
        self.has_different_types = false;
        self.has_large_delta = false;

        if !self.deltas.is_empty() {
            let tmp = self.deltas[0];
            for d in &self.deltas {
                if tmp != *d {
                    self.has_different_types = true;
                }
                if *d == SymbolTypeTcc::PacketReceivedLargeDelta as u16 {
                    self.has_large_delta = true;
                }
            }
        }

        svc
    }

    fn reset(&mut self) {
        self.deltas = vec![];
        self.has_large_delta = false;
        self.has_different_types = false;
    }
}
