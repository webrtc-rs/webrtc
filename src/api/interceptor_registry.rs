use crate::api::media_engine::MediaEngine;
use anyhow::Result;
use interceptor::registry::Registry;

/// register_default_interceptors will register some useful interceptors.
/// If you want to customize which interceptors are loaded, you should copy the
/// code from this method and remove unwanted interceptors.
pub fn register_default_interceptors(
    registry: Registry,
    _media_engine: &mut MediaEngine,
) -> Result<Registry> {
    Ok(registry)
}

/*TODO:
// ConfigureRTCPReports will setup everything necessary for generating Sender and Receiver Reports
func ConfigureRTCPReports(interceptorRegistry *interceptor.Registry) error {
    reciver, err := report.NewReceiverInterceptor()
    if err != nil {
        return err
    }

    sender, err := report.NewSenderInterceptor()
    if err != nil {
        return err
    }

    interceptorRegistry.Add(reciver)
    interceptorRegistry.Add(sender)
    return nil
}

// ConfigureNack will setup everything necessary for handling generating/responding to nack messages.
func ConfigureNack(mediaEngine *MediaEngine, interceptorRegistry *interceptor.Registry) error {
    generator, err := nack.NewGeneratorInterceptor()
    if err != nil {
        return err
    }

    responder, err := nack.NewResponderInterceptor()
    if err != nil {
        return err
    }

    mediaEngine.RegisterFeedback(RTCPFeedback{Type: "nack"}, RTPCodecTypeVideo)
    mediaEngine.RegisterFeedback(RTCPFeedback{Type: "nack", Parameter: "pli"}, RTPCodecTypeVideo)
    interceptorRegistry.Add(responder)
    interceptorRegistry.Add(generator)
    return nil
}*/
