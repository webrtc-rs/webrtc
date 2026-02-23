use crate::error::{Error, Result};
use crate::media_stream::Track;
use crate::media_stream::track_local::static_rtp::TrackLocalStaticRTP;
use crate::media_stream::track_local::{TrackLocal, TrackLocalContext};
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::rtp::packetizer::Packetizer;
use rtc::rtp::sequence::Sequencer;
use rtc::rtp_transceiver::SSRC;
use rtc::shared::error::flatten_errs;
use rtc::{rtcp, rtp};
use std::collections::HashMap;

const RTP_OUTBOUND_MTU: usize = 1200;

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Clone)]
pub struct TrackLocalStaticSample {
    rtp_track: TrackLocalStaticRTP,
    packetizers: HashMap<SSRC, Mutex<Box<dyn Packetizer>>>,
    sequencers: HashMap<SSRC, Box<dyn Sequencer>>,
}

impl TrackLocalStaticSample {
    pub fn new(track: MediaStreamTrack) -> Result<Self> {
        let (mut packetizers, mut sequencers) = (HashMap::new(), HashMap::new());
        for ssrc in track.ssrcs() {
            if let Some(codec) = track.codec(ssrc) {
                let payloader = codec.payloader()?;
                let sequencer: Box<dyn Sequencer> = Box::new(rtp::sequence::new_random_sequencer());
                let packetizer: Mutex<Box<dyn Packetizer>> =
                    Mutex::new(Box::new(rtp::packetizer::new_packetizer(
                        RTP_OUTBOUND_MTU,
                        0, // Value is handled when writing
                        ssrc,
                        payloader,
                        sequencer.clone(),
                        codec.clock_rate,
                    )));
                packetizers.insert(ssrc, packetizer);
                sequencers.insert(ssrc, sequencer);
            }
        }

        Ok(Self {
            rtp_track: TrackLocalStaticRTP::new(track),
            packetizers,
            sequencers,
        })
    }

    pub fn sample_writer(&self, ssrc: SSRC) -> SampleWriter<'_> {
        SampleWriter::new(self, ssrc)
    }

    pub async fn write_sample(
        &self,
        ssrc: SSRC,
        sample: &Sample,
        extensions: &[rtp::extension::HeaderExtension],
    ) -> Result<()> {
        // skip packets by the number of previously dropped packets
        if let Some(sequencer) = self.sequencers.get(&ssrc) {
            for _ in 0..sample.prev_dropped_packets {
                sequencer.next_sequence_number();
            }
        }

        let clock_rate = if let Some(codec) = self.track().codec(ssrc) {
            codec.clock_rate as f64
        } else {
            return Err(Error::CodecNotFound);
        };

        let packets = if let Some(packetizer) = self.packetizers.get(&ssrc) {
            let mut packetizer = packetizer.lock().await;
            let samples = (sample.duration.as_secs_f64() * clock_rate) as u32;
            if sample.prev_dropped_packets > 0 {
                packetizer.skip_samples(samples * sample.prev_dropped_packets as u32);
            }
            packetizer.packetize(&sample.data, samples)?
        } else {
            vec![]
        };

        let mut write_errs = vec![];
        for pkt in packets {
            if let Err(err) = self
                .rtp_track
                .write_rtp_with_extensions(pkt, extensions)
                .await
            {
                write_errs.push(err);
            }
        }

        flatten_errs(write_errs)
    }
}

impl Track for TrackLocalStaticSample {
    fn track(&self) -> &MediaStreamTrack {
        &self.rtp_track.track
    }
}

#[async_trait::async_trait]
impl TrackLocal for TrackLocalStaticSample {
    async fn bind(&self, ctx: TrackLocalContext) {
        self.rtp_track.bind(ctx).await;
    }

    async fn unbind(&self) {
        self.rtp_track.unbind().await;
    }

    async fn write_rtp(&self, packet: rtp::Packet) -> Result<()> {
        self.rtp_track.write_rtp(packet).await
    }

    async fn write_rtcp(&self, packets: Vec<Box<dyn rtcp::Packet>>) -> Result<()> {
        self.rtp_track.write_rtcp(packets).await
    }
}

mod sample_writer {
    use super::TrackLocalStaticSample;
    use crate::error::Result;
    use rtc::media::Sample;
    use rtc::rtp::extension::HeaderExtension;
    use rtc::rtp::extension::audio_level_extension::AudioLevelExtension;
    use rtc::rtp::extension::video_orientation_extension::VideoOrientationExtension;
    use rtc::rtp_transceiver::SSRC;

    /// Helper for writing Samples via [`TrackLocalStaticSample`] that carry extra RTP data.
    ///
    /// Created via [`TrackLocalStaticSample::sample_writer`].
    pub struct SampleWriter<'track> {
        ssrc: SSRC,
        track: &'track TrackLocalStaticSample,
        extensions: Vec<HeaderExtension>,
    }

    impl<'track> SampleWriter<'track> {
        pub(super) fn new(track: &'track TrackLocalStaticSample, ssrc: SSRC) -> Self {
            Self {
                ssrc,
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
                .write_sample(self.ssrc, sample, &self.extensions)
                .await
        }
    }
}

use crate::runtime::Mutex;
pub use sample_writer::SampleWriter;
