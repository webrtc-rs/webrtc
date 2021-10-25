use crate::api::media_engine::MediaEngine;
use crate::error::Result;
use crate::rtp_transceiver::{rtp_codec::RTPCodecType, RTCPFeedback};
use interceptor::nack::{generator::Generator, responder::Responder};
use interceptor::registry::Registry;
use interceptor::report::{receiver::ReceiverReport, sender::SenderReport};

/// register_default_interceptors will register some useful interceptors.
/// If you want to customize which interceptors are loaded, you should copy the
/// code from this method and remove unwanted interceptors.
pub fn register_default_interceptors(
    mut registry: Registry,
    media_engine: &mut MediaEngine,
) -> Result<Registry> {
    registry = configure_nack(registry, media_engine);

    registry = configure_rtcp_reports(registry);

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
