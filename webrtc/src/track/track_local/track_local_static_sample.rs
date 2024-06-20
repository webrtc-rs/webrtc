use log::warn;
use media::Sample;
use tokio::sync::Mutex;

use super::track_local_static_rtp::TrackLocalStaticRTP;
use super::*;
use crate::error::flatten_errs;
use crate::track::RTP_OUTBOUND_MTU;

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
    /// returns a TrackLocalStaticSample without RID
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

    /// returns a TrackLocalStaticSample with RID
    pub fn new_with_rid(
        codec: RTCRtpCodecCapability,
        id: String,
        rid: String,
        stream_id: String,
    ) -> Self {
        let rtp_track = TrackLocalStaticRTP::new_with_rid(codec, id, rid, stream_id);

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
        self.write_sample_with_extensions(sample, &[]).await
    }

    /// Write a sample with provided RTP extensions.
    ///
    /// Alternatively to this method [`TrackLocalStaticSample::sample_writer`] can be used instead.
    ///
    /// See [`TrackLocalStaticSample::write_sample`]  for further details.
    pub async fn write_sample_with_extensions(
        &self,
        sample: &Sample,
        extensions: &[rtp::extension::HeaderExtension],
    ) -> Result<()> {
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
            // SRTP has a further problem with regards to jumps in sequence number. Consider this:
            //
            // 1. Create track local
            // 2. Bind track local to track 1.
            // 3. Bind track local to track 2.
            // 4. Pause track 1.
            // 5. Keep sending...
            //
            // At this point, the track local will keep incrementing the sequence number, because we have
            // one binding that is still active. However SRTP hmac verifying (tag), can only accept a
            // relatively small jump in sequence numbers since it uses the ROC (i.e. how many times the
            // sequence number has rolled over), which means if this pause state of one binding persists
            // for a longer time, the track can never be resumed since the receiver would have missed
            // the rollovers.
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
            packetizer.packetize(&sample.data, samples)?
        } else {
            vec![]
        };

        let mut write_errs = vec![];
        for p in packets {
            if let Err(err) = self
                .rtp_track
                .write_rtp_with_extensions(&p, extensions)
                .await
            {
                write_errs.push(err);
            }
        }

        flatten_errs(write_errs)
    }

    /// Create a builder for writing samples with additional data.
    ///
    /// # Example
    /// ```no_run
    /// use rtp::extension::audio_level_extension::AudioLevelExtension;
    /// use std::time::Duration;
    /// use webrtc::api::media_engine::MIME_TYPE_VP8;
    /// use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
    /// use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let track = TrackLocalStaticSample::new(
    ///        RTCRtpCodecCapability {
    ///            mime_type: MIME_TYPE_VP8.to_owned(),
    ///            ..Default::default()
    ///        },
    ///        "video".to_owned(),
    ///        "webrtc-rs".to_owned(),
    ///     );
    ///     let result = track
    ///         .sample_writer()
    ///         .with_audio_level(AudioLevelExtension {
    ///             level: 10,
    ///             voice: true,
    ///         })
    ///         .write_sample(&media::Sample{
    ///              data: bytes::Bytes::new(),
    ///              duration: Duration::from_secs(1),
    ///              ..Default::default()
    ///         })
    ///         .await;
    /// }
    /// ```
    pub fn sample_writer(&self) -> SampleWriter<'_> {
        SampleWriter::new(self)
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

    /// RID is the RTP Stream ID for this track.
    fn rid(&self) -> Option<&str> {
        self.rtp_track.rid()
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

mod sample_writer {
    use media::Sample;
    use rtp::extension::audio_level_extension::AudioLevelExtension;
    use rtp::extension::video_orientation_extension::VideoOrientationExtension;
    use rtp::extension::HeaderExtension;

    use super::TrackLocalStaticSample;
    use crate::error::Result;

    /// Helper for writing Samples via [`TrackLocalStaticSample`] that carry extra RTP data.
    ///
    /// Created via [`TrackLocalStaticSample::sample_writer`].
    pub struct SampleWriter<'track> {
        track: &'track TrackLocalStaticSample,
        extensions: Vec<HeaderExtension>,
    }

    impl<'track> SampleWriter<'track> {
        pub(super) fn new(track: &'track TrackLocalStaticSample) -> Self {
            Self {
                track,
                extensions: vec![],
            }
        }

        /// Add a RTP audio level extension to all packets written for the sample.
        ///
        /// This overwrites any previously configured audio level extension.
        pub fn with_audio_level(self, ext: AudioLevelExtension) -> Self {
            self.with_extension(HeaderExtension::AudioLevel(ext))
        }

        /// Add a RTP video orientation extension to all packets written for the sample.
        ///
        /// This overwrites any previously configured video orientation extension.
        pub fn with_video_orientation(self, ext: VideoOrientationExtension) -> Self {
            self.with_extension(HeaderExtension::VideoOrientation(ext))
        }

        /// Add any RTP extension to all packets written for the sample.
        pub fn with_extension(mut self, ext: HeaderExtension) -> Self {
            self.extensions.retain(|e| !e.is_same(&ext));

            self.extensions.push(ext);

            self
        }

        /// Write the sample to the track.
        ///
        /// Creates one or more RTP packets with any extensions specified for each packet and sends
        /// them.
        pub async fn write_sample(self, sample: &Sample) -> Result<()> {
            self.track
                .write_sample_with_extensions(sample, &self.extensions)
                .await
        }
    }
}

pub use sample_writer::SampleWriter;
