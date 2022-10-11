use super::track_local_static_rtp::TrackLocalStaticRTP;
use super::*;
use crate::error::flatten_errs;

use crate::track::RTP_OUTBOUND_MTU;
use log::warn;
use media::Sample;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
struct TrackLocalStaticSampleInternal {
    packetizer: Option<Box<dyn rtp::packetizer::Packetizer + Send + Sync>>,
    sequencer: Option<Box<dyn rtp::sequence::Sequencer + Send + Sync>>,
    clock_rate: f64,
    did_warn_about_wonky_pause: bool,
}

/// TrackLocalStaticSample is a TrackLocal that has a pre-set codec and accepts Samples.
/// If you wish to send a RTP Packet use TrackLocalStaticRTP
#[derive(Debug)]
pub struct TrackLocalStaticSample {
    rtp_track: TrackLocalStaticRTP,
    internal: Mutex<TrackLocalStaticSampleInternal>,
}

impl TrackLocalStaticSample {
    /// returns a TrackLocalStaticSample
    pub fn new(codec: RTCRtpCodecCapability, id: String, stream_id: String) -> Self {
        let rtp_track = TrackLocalStaticRTP::new(codec, id, stream_id);

        TrackLocalStaticSample {
            rtp_track,
            internal: Mutex::new(TrackLocalStaticSampleInternal {
                packetizer: None,
                sequencer: None,
                clock_rate: 0.0f64,
                did_warn_about_wonky_pause: false,
            }),
        }
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> RTCRtpCodecCapability {
        self.rtp_track.codec()
    }

    /// write_sample writes a Sample to the TrackLocalStaticSample
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    pub async fn write_sample(&self, sample: &Sample) -> Result<()> {
        let mut internal = self.internal.lock().await;

        if internal.packetizer.is_none() || internal.sequencer.is_none() {
            return Ok(());
        }

        let (any_paused, all_paused) = (
            self.rtp_track.any_binding_paused().await,
            self.rtp_track.all_binding_paused().await,
        );

        if all_paused {
            // Abort already here to not increment sequence numbers.
            return Ok(());
        }

        if any_paused {
            // This is a problem state due to how this impl is structured. The sequencer will allocate
            // one sequence number per RTP packet regardless of how many TrackBinding that will send
            // the packet. I.e. we get the same sequence number per multiple SSRC, which is not good
            // for SRTP, but that's how it works.
            //
            // Chrome has further a problem with regards to jumps in sequence number. Consider this:
            //
            // 1. Create track local
            // 2. Bind track local to track 1.
            // 3. Bind track local to track 2.
            // 4. Pause track 1.
            // 5. Keep sending...
            //
            // At this point, the track local will keep incrementing the sequence number, because we have
            // one binding that is still active. However Chrome can only accept a relatively small jump
            // in SRTP key deriving, which means if this pause state of one binding persists for a longer
            // time, the track can never be resumed (against Chrome).
            if !internal.did_warn_about_wonky_pause {
                internal.did_warn_about_wonky_pause = true;
                warn!("Detected multiple track bindings where only one was paused");
            }
        }

        // skip packets by the number of previously dropped packets
        if let Some(sequencer) = &internal.sequencer {
            for _ in 0..sample.prev_dropped_packets {
                sequencer.next_sequence_number();
            }
        }

        let clock_rate = internal.clock_rate;

        let packets = if let Some(packetizer) = &mut internal.packetizer {
            let samples = (sample.duration.as_secs_f64() * clock_rate) as u32;
            if sample.prev_dropped_packets > 0 {
                packetizer.skip_samples(samples * sample.prev_dropped_packets as u32);
            }
            /*println!(
                "clock_rate={}, samples={}, {}",
                clock_rate,
                samples,
                sample.duration.as_secs_f64()
            );*/
            packetizer.packetize(&sample.data, samples).await?
        } else {
            vec![]
        };

        let mut write_errs = vec![];
        for p in packets {
            if let Err(err) = self.rtp_track.write_rtp(&p).await {
                write_errs.push(err);
            }
        }

        flatten_errs(write_errs)
    }
}

#[async_trait]
impl TrackLocal for TrackLocalStaticSample {
    /// Bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTCRtpCodecParameters> {
        let codec = self.rtp_track.bind(t).await?;

        let mut internal = self.internal.lock().await;

        // We only need one packetizer
        if internal.packetizer.is_some() {
            return Ok(codec);
        }

        let payloader = codec.capability.payloader_for_codec()?;
        let sequencer: Box<dyn rtp::sequence::Sequencer + Send + Sync> =
            Box::new(rtp::sequence::new_random_sequencer());
        internal.packetizer = Some(Box::new(rtp::packetizer::new_packetizer(
            RTP_OUTBOUND_MTU,
            0, // Value is handled when writing
            0, // Value is handled when writing
            payloader,
            sequencer.clone(),
            codec.capability.clock_rate,
        )));
        internal.sequencer = Some(sequencer);
        internal.clock_rate = codec.capability.clock_rate as f64;

        Ok(codec)
    }

    /// unbind implements the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self, t: &TrackLocalContext) -> Result<()> {
        self.rtp_track.unbind(t).await
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    fn id(&self) -> &str {
        self.rtp_track.id()
    }

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> &str {
        self.rtp_track.stream_id()
    }

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType {
        self.rtp_track.kind()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
