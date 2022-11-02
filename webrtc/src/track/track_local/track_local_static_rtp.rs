use std::collections::HashMap;

use super::*;

use crate::error::flatten_errs;
use bytes::BytesMut;
use tokio::sync::Mutex;
use util::{Marshal, MarshalSize};

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug)]
pub struct TrackLocalStaticRTP {
    pub(crate) bindings: Mutex<Vec<Arc<TrackBinding>>>,
    codec: RTCRtpCodecCapability,
    id: String,
    stream_id: String,
}

impl TrackLocalStaticRTP {
    /// returns a TrackLocalStaticRTP.
    pub fn new(codec: RTCRtpCodecCapability, id: String, stream_id: String) -> Self {
        TrackLocalStaticRTP {
            codec,
            bindings: Mutex::new(vec![]),
            id,
            stream_id,
        }
    }

    /// codec gets the Codec of the track
    pub fn codec(&self) -> RTCRtpCodecCapability {
        self.codec.clone()
    }

    pub async fn any_binding_paused(&self) -> bool {
        let bindings = self.bindings.lock().await;
        bindings
            .iter()
            .any(|b| b.sender_paused.load(Ordering::SeqCst))
    }

    pub async fn all_binding_paused(&self) -> bool {
        let bindings = self.bindings.lock().await;
        bindings
            .iter()
            .all(|b| b.sender_paused.load(Ordering::SeqCst))
    }

    /// write_rtp_with_extensions writes a RTP Packet to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    ///
    /// If the RTCRtpSender direction is such that no packets should be sent, any call to this
    /// function are blocked internally. Care must be taken to not increase the sequence number
    /// while the sender is paused. While the actual _sending_ is blocked, the receiver will
    /// miss out when the sequence number "rolls over", which in turn will break SRTP.
    ///
    /// Extensions that are already configured on the packet are overwritten by extensions in
    /// `extensions`.
    pub async fn write_rtp_with_extensions(
        &self,
        p: &rtp::packet::Packet,
        extensions: &[rtp::extension::HeaderExtension],
    ) -> Result<usize> {
        let mut n = 0;
        let mut write_errs = vec![];
        let mut pkt = p.clone();

        let bindings = {
            let bindings = self.bindings.lock().await;
            bindings.clone()
        };
        // Prepare the extensions data
        let extension_data: HashMap<_, _> = extensions
            .iter()
            .flat_map(|extension| {
                let buf = {
                    let mut buf = BytesMut::with_capacity(extension.marshal_size());
                    buf.resize(extension.marshal_size(), 0);
                    if let Err(err) = extension.marshal_to(&mut buf) {
                        write_errs.push(Error::Util(err));
                        return None;
                    }

                    buf.freeze()
                };

                Some((extension.uri(), buf))
            })
            .collect();

        for b in bindings.into_iter() {
            if b.is_sender_paused() {
                // See caveat in function doc.
                continue;
            }
            pkt.header.ssrc = b.ssrc;
            pkt.header.payload_type = b.payload_type;

            for (uri, data) in extension_data.iter() {
                if let Some(id) = b
                    .params
                    .header_extensions
                    .iter()
                    .find(|ext| &ext.uri == uri)
                    .map(|ext| ext.id)
                {
                    if let Err(err) = pkt.header.set_extension(id as u8, data.clone()) {
                        write_errs.push(Error::Rtp(err));
                        continue;
                    }
                }
            }

            if let Some(write_stream) = &b.write_stream {
                match write_stream.write_rtp(&pkt).await {
                    Ok(m) => {
                        n += m;
                    }
                    Err(err) => {
                        write_errs.push(err);
                    }
                }
            } else {
                write_errs.push(Error::new("track binding has none write_stream".to_owned()));
            }
        }

        flatten_errs(write_errs)?;
        Ok(n)
    }
}

#[async_trait]
impl TrackLocal for TrackLocalStaticRTP {
    /// bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTCRtpCodecParameters> {
        let parameters = RTCRtpCodecParameters {
            capability: self.codec.clone(),
            ..Default::default()
        };

        let (codec, match_type) = codec_parameters_fuzzy_search(&parameters, t.codec_parameters());
        if match_type != CodecMatch::None {
            {
                let mut bindings = self.bindings.lock().await;
                bindings.push(Arc::new(TrackBinding {
                    ssrc: t.ssrc(),
                    payload_type: codec.payload_type,
                    write_stream: t.write_stream(),
                    params: t.params.clone(),
                    id: t.id(),
                    sender_paused: t.paused.clone(),
                }));
            }

            Ok(codec)
        } else {
            Err(Error::ErrUnsupportedCodec)
        }
    }

    /// unbind implements the teardown logic when the track is no longer needed. This happens
    /// because a track has been stopped.
    async fn unbind(&self, t: &TrackLocalContext) -> Result<()> {
        let mut bindings = self.bindings.lock().await;
        let mut idx = None;
        for (index, binding) in bindings.iter().enumerate() {
            if binding.id == t.id() {
                idx = Some(index);
                break;
            }
        }
        if let Some(index) = idx {
            bindings.remove(index);
            Ok(())
        } else {
            Err(Error::ErrUnbindFailed)
        }
    }

    /// id is the unique identifier for this Track. This should be unique for the
    /// stream, but doesn't have to globally unique. A common example would be 'audio' or 'video'
    /// and StreamID would be 'desktop' or 'webcam'
    fn id(&self) -> &str {
        self.id.as_str()
    }

    /// stream_id is the group this track belongs too. This must be unique
    fn stream_id(&self) -> &str {
        self.stream_id.as_str()
    }

    /// kind controls if this TrackLocal is audio or video
    fn kind(&self) -> RTPCodecType {
        if self.codec.mime_type.starts_with("audio/") {
            RTPCodecType::Audio
        } else if self.codec.mime_type.starts_with("video/") {
            RTPCodecType::Video
        } else {
            RTPCodecType::Unspecified
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[async_trait]
impl TrackLocalWriter for TrackLocalStaticRTP {
    /// write_rtp writes a RTP Packet to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    ///
    /// If the RTCRtpSender direction is such that no packets should be sent, any call to this
    /// function are blocked internally. Care must be taken to not increase the sequence number
    /// while the sender is paused. While the actual _sending_ is blocked, the receiver will
    /// miss out when the sequence number "rolls over", which in turn will break SRTP.
    async fn write_rtp(&self, p: &rtp::packet::Packet) -> Result<usize> {
        self.write_rtp_with_extensions(p, &[]).await
    }

    /// write writes a RTP Packet as a buffer to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    async fn write(&self, mut b: &[u8]) -> Result<usize> {
        let pkt = rtp::packet::Packet::unmarshal(&mut b)?;
        self.write_rtp(&pkt).await?;
        Ok(b.len())
    }
}
