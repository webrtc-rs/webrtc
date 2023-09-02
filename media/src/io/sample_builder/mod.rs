#[cfg(test)]
mod sample_builder_test;
#[cfg(test)]
mod sample_sequence_location_test;

pub mod sample_sequence_location;

use std::time::{Duration, SystemTime};

use bytes::Bytes;
use rtp::packet::Packet;
use rtp::packetizer::Depacketizer;

use self::sample_sequence_location::{Comparison, SampleSequenceLocation};
use crate::Sample;

/// SampleBuilder buffers packets until media frames are complete.
pub struct SampleBuilder<T: Depacketizer> {
    /// how many packets to wait until we get a valid Sample
    max_late: u16,
    /// max timestamp between old and new timestamps before dropping packets
    max_late_timestamp: u32,
    buffer: Vec<Option<Packet>>,
    prepared_samples: Vec<Option<Sample>>,
    last_sample_timestamp: Option<u32>,

    /// Interface that allows us to take RTP packets to samples
    depacketizer: T,

    /// sample_rate allows us to compute duration of media.SamplecA
    sample_rate: u32,

    /// filled contains the head/tail of the packets inserted into the buffer
    filled: SampleSequenceLocation,

    /// active contains the active head/tail of the timestamp being actively processed
    active: SampleSequenceLocation,

    /// prepared contains the samples that have been processed to date
    prepared: SampleSequenceLocation,

    /// number of packets forced to be dropped
    dropped_packets: u16,

    /// number of padding packets detected and dropped. This number will be a subset of
    /// `dropped_packets`
    padding_packets: u16,
}

impl<T: Depacketizer> SampleBuilder<T> {
    /// Constructs a new SampleBuilder.
    /// `max_late` is how long to wait until we can construct a completed [`Sample`].
    /// `max_late` is measured in RTP packet sequence numbers.
    /// A large max_late will result in less packet loss but higher latency.
    /// The depacketizer extracts media samples from RTP packets.
    /// Several depacketizers are available in package [github.com/pion/rtp/codecs](https://github.com/webrtc-rs/rtp/tree/main/src/codecs).
    pub fn new(max_late: u16, depacketizer: T, sample_rate: u32) -> Self {
        Self {
            max_late,
            max_late_timestamp: 0,
            buffer: vec![None; u16::MAX as usize + 1],
            prepared_samples: (0..=u16::MAX as usize).map(|_| None).collect(),
            last_sample_timestamp: None,
            depacketizer,
            sample_rate,
            filled: SampleSequenceLocation::new(),
            active: SampleSequenceLocation::new(),
            prepared: SampleSequenceLocation::new(),
            dropped_packets: 0,
            padding_packets: 0,
        }
    }

    pub fn with_max_time_delay(mut self, max_late_duration: Duration) -> Self {
        self.max_late_timestamp =
            (self.sample_rate as u128 * max_late_duration.as_millis() / 1000) as u32;
        self
    }

    fn too_old(&self, location: &SampleSequenceLocation) -> bool {
        if self.max_late_timestamp == 0 {
            return false;
        }

        let mut found_head: Option<u32> = None;
        let mut found_tail: Option<u32> = None;

        let mut i = location.head;
        while i != location.tail {
            if let Some(ref packet) = self.buffer[i as usize] {
                found_head = Some(packet.header.timestamp);
                break;
            }
            i = i.wrapping_add(1);
        }

        if found_head.is_none() {
            return false;
        }

        let mut i = location.tail.wrapping_sub(1);
        while i != location.head {
            if let Some(ref packet) = self.buffer[i as usize] {
                found_tail = Some(packet.header.timestamp);
                break;
            }
            i = i.wrapping_sub(1);
        }

        if found_tail.is_none() {
            return false;
        }

        found_tail.unwrap() - found_head.unwrap() > self.max_late_timestamp
    }

    /// Returns the timestamp associated with a given sample location
    fn fetch_timestamp(&self, location: &SampleSequenceLocation) -> Option<u32> {
        if location.empty() {
            None
        } else {
            Some(
                (self.buffer[location.head as usize])
                    .as_ref()?
                    .header
                    .timestamp,
            )
        }
    }

    fn release_packet(&mut self, i: u16) {
        self.buffer[i as usize] = None;
    }

    /// Clears all buffers that have already been consumed by
    /// popping.
    fn purge_consumed_buffers(&mut self) {
        let active = self.active;
        self.purge_consumed_location(&active, false);
    }

    /// Clears all buffers that have already been consumed
    /// during a sample building method.
    fn purge_consumed_location(&mut self, consume: &SampleSequenceLocation, force_consume: bool) {
        if !self.filled.has_data() {
            return;
        }
        match consume.compare(self.filled.head) {
            Comparison::Inside if force_consume => {
                self.release_packet(self.filled.head);
                self.filled.head = self.filled.head.wrapping_add(1);
            }
            Comparison::Before => {
                self.release_packet(self.filled.head);
                self.filled.head = self.filled.head.wrapping_add(1);
            }
            _ => {}
        }
    }

    /// Flushes all buffers that are already consumed or those buffers
    /// that are too late to consume.
    fn purge_buffers(&mut self) {
        self.purge_consumed_buffers();

        while (self.too_old(&self.filled) || (self.filled.count() > self.max_late))
            && self.filled.has_data()
        {
            if self.active.empty() {
                // refill the active based on the filled packets
                self.active = self.filled;
            }

            if self.active.has_data() && (self.active.head == self.filled.head) {
                // attempt to force the active packet to be consumed even though
                // outstanding data may be pending arrival
                let err = match self.build_sample(true) {
                    Ok(_) => continue,
                    Err(e) => e,
                };

                if !matches!(err, BuildError::InvalidPartition(_)) {
                    // In the InvalidPartition case `build_sample` will have already adjusted `dropped_packets`.
                    self.dropped_packets += 1;
                }

                // could not build the sample so drop it
                self.active.head = self.active.head.wrapping_add(1);
            }

            self.release_packet(self.filled.head);
            self.filled.head = self.filled.head.wrapping_add(1);
        }
    }

    /// Adds an RTP Packet to self's buffer.
    ///
    /// Push does not copy the input. If you wish to reuse
    /// this memory make sure to copy before calling push
    pub fn push(&mut self, p: Packet) {
        let sequence_number = p.header.sequence_number;
        self.buffer[sequence_number as usize] = Some(p);
        match self.filled.compare(sequence_number) {
            Comparison::Void => {
                self.filled.head = sequence_number;
                self.filled.tail = sequence_number.wrapping_add(1);
            }
            Comparison::Before => {
                self.filled.head = sequence_number;
            }
            Comparison::After => {
                self.filled.tail = sequence_number.wrapping_add(1);
            }
            _ => {}
        }
        self.purge_buffers();
    }

    /// Creates a sample from a valid collection of RTP Packets by
    /// walking forwards building a sample if everything looks good clear and
    /// update buffer+values
    fn build_sample(
        &mut self,
        purging_buffers: bool,
    ) -> Result<SampleSequenceLocation, BuildError> {
        if self.active.empty() {
            self.active = self.filled;
        }

        if self.active.empty() {
            return Err(BuildError::NoActiveSegment);
        }

        if self.filled.compare(self.active.tail) == Comparison::Inside {
            self.active.tail = self.filled.tail;
        }

        let mut consume = SampleSequenceLocation::new();

        let mut i = self.active.head;
        // `self.active` isn't modified in the loop, fetch the timestamp once and cache it.
        let head_timestamp = self.fetch_timestamp(&self.active);
        while let Some(ref packet) = self.buffer[i as usize] {
            if self.active.compare(i) == Comparison::After {
                break;
            }
            let is_same_timestamp = head_timestamp.map(|t| packet.header.timestamp == t);
            let is_different_timestamp = is_same_timestamp.map(std::ops::Not::not);
            let is_partition_tail = self
                .depacketizer
                .is_partition_tail(packet.header.marker, &packet.payload);

            // If the timestamp is not the same it might be because the next packet is both a start
            // and end of the next partition in which case a sample should be generated now. This
            // can happen when padding packets are used .e.g:
            //
            // p1(t=1), p2(t=1), p3(t=1), p4(t=2, marker=true, start=true)
            //
            // In thic case the generated sample should be p1 through p3, but excluding p4 which is
            // its own sample.
            if is_partition_tail && is_same_timestamp.unwrap_or(true) {
                consume.head = self.active.head;
                consume.tail = i.wrapping_add(1);
                break;
            }

            if is_different_timestamp.unwrap_or(false) {
                consume.head = self.active.head;
                consume.tail = i;
                break;
            }
            i = i.wrapping_add(1);
        }

        if consume.empty() {
            return Err(BuildError::NothingToConsume);
        }

        if !purging_buffers && self.buffer[consume.tail as usize].is_none() {
            // wait for the next packet after this set of packets to arrive
            // to ensure at least one post sample timestamp is known
            // (unless we have to release right now)
            return Err(BuildError::PendingTimestampPacket);
        }

        let sample_timestamp = self.fetch_timestamp(&self.active).unwrap_or(0);
        let mut after_timestamp = sample_timestamp;

        // scan for any packet after the current and use that time stamp as the diff point
        for i in consume.tail..self.active.tail {
            if let Some(ref packet) = self.buffer[i as usize] {
                after_timestamp = packet.header.timestamp;
                break;
            }
        }

        // prior to decoding all the packets, check if this packet
        // would end being disposed anyway
        let head_payload = self.buffer[consume.head as usize]
            .as_ref()
            .map(|p| &p.payload)
            .ok_or(BuildError::GapInSegment)?;
        if !self.depacketizer.is_partition_head(head_payload) {
            // libWebRTC will sometimes send several empty padding packets to smooth out send
            // rate. These packets don't carry any media payloads.
            let is_padding = consume.range(&self.buffer).all(|p| {
                p.map(|p| {
                    self.last_sample_timestamp == Some(p.header.timestamp) && p.payload.is_empty()
                })
                .unwrap_or(false)
            });

            self.dropped_packets += consume.count();
            if is_padding {
                self.padding_packets += consume.count();
            }
            self.purge_consumed_location(&consume, true);
            self.purge_consumed_buffers();

            self.active.head = consume.tail;
            return Err(BuildError::InvalidPartition(consume));
        }

        // the head set of packets is now fully consumed
        self.active.head = consume.tail;

        // merge all the buffers into a sample
        let mut data: Vec<u8> = Vec::new();
        let mut i = consume.head;
        while i != consume.tail {
            let payload = self.buffer[i as usize]
                .as_ref()
                .map(|p| &p.payload)
                .ok_or(BuildError::GapInSegment)?;

            let p = self
                .depacketizer
                .depacketize(payload)
                .map_err(|_| BuildError::DepacketizerFailed)?;

            data.extend_from_slice(&p);
            i = i.wrapping_add(1);
        }
        let samples = after_timestamp - sample_timestamp;

        let sample = Sample {
            data: Bytes::copy_from_slice(&data),
            timestamp: SystemTime::now(),
            duration: Duration::from_secs_f64((samples as f64) / (self.sample_rate as f64)),
            packet_timestamp: sample_timestamp,
            prev_dropped_packets: self.dropped_packets,
            prev_padding_packets: self.padding_packets,
        };

        self.dropped_packets = 0;
        self.padding_packets = 0;
        self.last_sample_timestamp = Some(sample_timestamp);

        self.prepared_samples[self.prepared.tail as usize] = Some(sample);
        self.prepared.tail = self.prepared.tail.wrapping_add(1);

        self.purge_consumed_location(&consume, true);
        self.purge_consumed_buffers();

        Ok(consume)
    }

    /// Compiles pushed RTP packets into media samples and then
    /// returns the next valid sample (or None if no sample is compiled).
    pub fn pop(&mut self) -> Option<Sample> {
        let _ = self.build_sample(false);

        if self.prepared.empty() {
            return None;
        }
        let result = self.prepared_samples[self.prepared.head as usize].take();
        self.prepared.head = self.prepared.head.wrapping_add(1);
        result
    }

    /// Compiles pushed RTP packets into media samples and then
    /// returns the next valid sample with its associated RTP timestamp (or `None` if
    /// no sample is compiled).
    pub fn pop_with_timestamp(&mut self) -> Option<(Sample, u32)> {
        if let Some(sample) = self.pop() {
            let timestamp = sample.packet_timestamp;
            Some((sample, timestamp))
        } else {
            None
        }
    }
}

/// Computes the distance between two sequence numbers
/*pub(crate) fn seqnum_distance(head: u16, tail: u16) -> u16 {
    if head > tail {
        head.wrapping_add(tail)
    } else {
        tail - head
    }
}*/

pub(crate) fn seqnum_distance(x: u16, y: u16) -> u16 {
    let diff = x.wrapping_sub(y);
    if diff > 0xFFFF / 2 {
        0xFFFF - diff + 1
    } else {
        diff
    }
}

#[derive(Debug)]
enum BuildError {
    /// There's no active segment of RTP packets to consider yet.
    NoActiveSegment,

    /// No sample partition could be found in the active segment.
    NothingToConsume,

    /// A segment to consume was identified, but a subsequent packet is needed to determine the
    /// duration of the sample.
    PendingTimestampPacket,

    /// The active segment's head was not aligned with a sample partition head. Some packets were
    /// dropped.
    InvalidPartition(SampleSequenceLocation),

    /// There was a gap in the active segment because of one or more missing RTP packets.
    GapInSegment,

    /// We failed to depacketize an RTP packet.
    DepacketizerFailed,
}
