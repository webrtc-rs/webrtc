#[cfg(test)]
mod interceptor_registry_test;

use crate::api::media_engine::MediaEngine;
use crate::error::Result;
use crate::rtp_transceiver::rtp_codec::RTCRtpHeaderExtensionCapability;
use crate::rtp_transceiver::{rtp_codec::RTPCodecType, RTCPFeedback, TYPE_RTCP_FB_TRANSPORT_CC};

use interceptor::nack::{generator::Generator, responder::Responder};
use interceptor::registry::Registry;
use interceptor::report::{receiver::ReceiverReport, sender::SenderReport};
use interceptor::twcc::header_extension::HeaderExtension;
use interceptor::twcc::sender::Sender;

/// register_default_interceptors will register some useful interceptors.
/// If you want to customize which interceptors are loaded, you should copy the
/// code from this method and remove unwanted interceptors.
pub async fn register_default_interceptors(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    registry = configure_nack(registry, media_engine);

    registry = configure_rtcp_reports(registry);

    //TODO: temporarily disable twcc until all reference cycle memory leak fixed
    // current when configure_twcc_sender, audio + video cause corrupted audio
    // https://github.com/webrtc-rs/webrtc/issues/129
    // registry = configure_twcc_sender(registry, media_engine).await?;

    Ok(registry)
}

/// configure_rtcp_reports will setup everything necessary for generating Sender and Receiver Reports
pub fn configure_rtcp_reports(mut registry: Registry) -> Registry {
    let receiver = Box::new(ReceiverReport::builder());
    let sender = Box::new(SenderReport::builder());
    registry.add(receiver);
    registry.add(sender);
    registry
}

/// configure_nack will setup everything necessary for handling generating/responding to nack messages.
pub fn configure_nack(mut registry: Registry, media_engine: &mut MediaEngine) -> Registry {
    let generator = Box::new(Generator::builder());
    let responder = Box::new(Responder::builder());

    media_engine.register_feedback(
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "".to_owned(),
        },
        RTPCodecType::Video,
    );
    media_engine.register_feedback(
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
        RTPCodecType::Video,
    );

    registry.add(responder);
    registry.add(generator);
    registry
}

/// configure_twcc_header_extension_sender will setup everything necessary for adding
/// a TWCC header extension to outgoing RTP packets. This will allow the remote peer to generate TWCC reports.
pub async fn configure_twcc_header_extension_sender(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
            },
            RTPCodecType::Video,
            vec![],
        )
        .await?;

    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
            },
            RTPCodecType::Audio,
            vec![],
        )
        .await?;

    let header_extension = Box::new(HeaderExtension::builder());

    registry.add(header_extension);
    Ok(registry)
}

/// configure_twcc_sender will setup everything necessary for generating TWCC reports.
pub async fn configure_twcc_sender(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    media_engine.register_feedback(
        RTCPFeedback {
            typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
            ..Default::default()
        },
        RTPCodecType::Video,
    );
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
            },
            RTPCodecType::Video,
            vec![],
        )
        .await?;

    media_engine.register_feedback(
        RTCPFeedback {
            typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
            ..Default::default()
        },
        RTPCodecType::Audio,
    );
    media_engine
        .register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
            },
            RTPCodecType::Audio,
            vec![],
        )
        .await?;

    let sender = Box::new(Sender::builder());
    registry.add(sender);
    Ok(registry)
}
