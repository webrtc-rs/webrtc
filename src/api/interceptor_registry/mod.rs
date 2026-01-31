#[cfg(test)]
mod interceptor_registry_test;

use interceptor::nack::generator::Generator;
use interceptor::nack::responder::Responder;
use interceptor::registry::Registry;
use interceptor::report::receiver::ReceiverReport;
use interceptor::report::sender::SenderReport;
use interceptor::twcc::receiver::Receiver;
use interceptor::twcc::sender::Sender;

use crate::api::media_engine::MediaEngine;
use crate::error::Result;
use crate::rtp_transceiver::rtp_codec::{RTCRtpHeaderExtensionCapability, RTPCodecType};
use crate::rtp_transceiver::{RTCPFeedback, TYPE_RTCP_FB_TRANSPORT_CC};

/// register_default_interceptors will register some useful interceptors.
/// If you want to customize which interceptors are loaded, you should copy the
/// code from this method and remove unwanted interceptors.
pub fn register_default_interceptors(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    registry = configure_nack(registry, media_engine);

    registry = configure_rtcp_reports(registry);

    registry = configure_twcc_receiver_only(registry, media_engine)?;

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

    let generator = Box::new(Generator::builder());
    let responder = Box::new(Responder::builder());
    registry.add(responder);
    registry.add(generator);
    registry
}

/// configure_twcc will setup everything necessary for adding
/// a TWCC header extension to outgoing RTP packets and generating TWCC reports.
pub fn configure_twcc(mut registry: Registry, media_engine: &mut MediaEngine) -> Result<Registry> {
    media_engine.register_feedback(
        RTCPFeedback {
            typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
            ..Default::default()
        },
        RTPCodecType::Video,
    );
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Video,
        None,
    )?;

    media_engine.register_feedback(
        RTCPFeedback {
            typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
            ..Default::default()
        },
        RTPCodecType::Audio,
    );
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Audio,
        None,
    )?;

    let sender = Box::new(Sender::builder());
    let receiver = Box::new(Receiver::builder());
    registry.add(sender);
    registry.add(receiver);
    Ok(registry)
}

/// configure_twcc_sender will setup everything necessary for adding
/// a TWCC header extension to outgoing RTP packets. This will allow the remote peer to generate TWCC reports.
pub fn configure_twcc_sender_only(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Video,
        None,
    )?;

    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Audio,
        None,
    )?;

    let sender = Box::new(Sender::builder());
    registry.add(sender);
    Ok(registry)
}

/// configure_twcc_receiver will setup everything necessary for generating TWCC reports.
pub fn configure_twcc_receiver_only(
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
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Video,
        None,
    )?;

    media_engine.register_feedback(
        RTCPFeedback {
            typ: TYPE_RTCP_FB_TRANSPORT_CC.to_owned(),
            ..Default::default()
        },
        RTPCodecType::Audio,
    );
    media_engine.register_header_extension(
        RTCRtpHeaderExtensionCapability {
            uri: sdp::extmap::TRANSPORT_CC_URI.to_owned(),
        },
        RTPCodecType::Audio,
        None,
    )?;

    let receiver = Box::new(Receiver::builder());
    registry.add(receiver);
    Ok(registry)
}
