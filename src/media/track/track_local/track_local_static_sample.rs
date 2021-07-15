use super::track_local_static_rtp::TrackLocalStaticRTP;
use super::*;
use crate::media::Sample;
use crate::RTP_OUTBOUND_MTU;

/// TrackLocalStaticSample is a TrackLocal that has a pre-set codec and accepts Samples.
/// If you wish to send a RTP Packet use TrackLocalStaticRTP
#[derive(Debug, Clone)]
pub struct TrackLocalStaticSample {
    packetizer: Option<Box<dyn rtp::packetizer::Packetizer + Send + Sync>>,
    sequencer: Option<Box<dyn rtp::sequence::Sequencer + Send + Sync>>,
    rtp_track: TrackLocalStaticRTP,
    clock_rate: f64,
}

impl TrackLocalStaticSample {
    /// returns a TrackLocalStaticSample
    pub fn new(codec: RTPCodecCapability, id: String, stream_id: String) -> Self {
        let rtp_track = TrackLocalStaticRTP::new(codec, id, stream_id);

        TrackLocalStaticSample {
            packetizer: None,
            sequencer: None,
            rtp_track,
            clock_rate: 0.0f64,
        }
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> RTPCodecCapability {
        self.rtp_track.codec()
    }

    /// write_sample writes a Sample to the TrackLocalStaticSample
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    pub fn write_sample(&mut self, sample: &Sample) -> Result<()> {
        if self.packetizer.is_none() || self.sequencer.is_none() {
            return Ok(());
        }

        // skip packets by the number of previously dropped packets
        if let Some(sequencer) = &self.sequencer {
            for _ in 0..sample.prev_dropped_packets {
                sequencer.next_sequence_number();
            }
        }

        let packets = if let Some(packetizer) = &mut self.packetizer {
            let samples = (sample.duration.as_secs() as f64 * self.clock_rate) as u32;
            if sample.prev_dropped_packets > 0 {
                packetizer.skip_samples(samples * sample.prev_dropped_packets as u32);
            }
            packetizer.packetize(&sample.data, samples)?
        } else {
            vec![]
        };

        let mut write_err = None;
        for p in packets {
            if let Err(err) = self.rtp_track.write_rtp(&p) {
                write_err = Some(err);
            }
        }

        if let Some(err) = write_err {
            Err(err)
        } else {
            Ok(())
        }
    }
}

impl TrackLocal for TrackLocalStaticSample {
    /// Bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    fn bind(&mut self, t: TrackLocalContext) -> Result<RTPCodecParameters> {
        let codec = self.rtp_track.bind(t)?;

        // We only need one packetizer
        if self.packetizer.is_some() {
            return Ok(codec);
        }

        let payloader = codec.capability.payloader_for_codec()?;
        let sequencer: Box<dyn rtp::sequence::Sequencer + Send + Sync> =
            Box::new(rtp::sequence::new_random_sequencer());
        self.packetizer = Some(Box::new(rtp::packetizer::new_packetizer(
            RTP_OUTBOUND_MTU,
            0, // Value is handled when writing
            0, // Value is handled when writing
            payloader,
            sequencer.clone(),
            codec.capability.clock_rate,
        )));
        self.sequencer = Some(sequencer);
        self.clock_rate = codec.capability.clock_rate as f64;

        Ok(codec)
    }

    /// unbind implements the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    fn unbind(&mut self, t: TrackLocalContext) -> Result<()> {
        self.rtp_track.unbind(t)
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    fn id(&self) -> String {
        self.rtp_track.id()
    }

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> String {
        self.rtp_track.stream_id()
    }

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType {
        self.rtp_track.kind()
    }
}
