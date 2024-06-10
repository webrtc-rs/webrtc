use std::future::Future;
use std::num::ParseIntError;
use std::pin::Pin;
use std::string::FromUtf8Error;

use thiserror::Error;
use tokio::sync::mpsc::error::SendError as MpscSendError;

use crate::peer_connection::sdp::sdp_type::RTCSdpType;
use crate::peer_connection::signaling_state::RTCSignalingState;
use crate::rtp_transceiver::rtp_receiver;
#[cfg(doc)]
use crate::rtp_transceiver::rtp_sender;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// ErrUnknownType indicates an error with Unknown info.
    #[error("unknown")]
    ErrUnknownType,

    /// ErrConnectionClosed indicates an operation executed after connection
    /// has already been closed.
    #[error("connection closed")]
    ErrConnectionClosed,

    /// ErrDataChannelNotOpen indicates an operation executed when the data
    /// channel is not (yet) open.
    #[error("data channel not open")]
    ErrDataChannelNotOpen,

    /// ErrCertificateExpired indicates that an x509 certificate has expired.
    #[error("x509Cert expired")]
    ErrCertificateExpired,

    /// ErrNoTurnCredentials indicates that a TURN server URL was provided
    /// without required credentials.
    #[error("turn server credentials required")]
    ErrNoTurnCredentials,

    /// ErrTurnCredentials indicates that provided TURN credentials are partial
    /// or malformed.
    #[error("invalid turn server credentials")]
    ErrTurnCredentials,

    /// ErrExistingTrack indicates that a track already exists.
    #[error("track already exists")]
    ErrExistingTrack,

    /// ErrPrivateKeyType indicates that a particular private key encryption
    /// chosen to generate a certificate is not supported.
    #[error("private key type not supported")]
    ErrPrivateKeyType,

    /// ErrModifyingPeerIdentity indicates that an attempt to modify
    /// PeerIdentity was made after PeerConnection has been initialized.
    #[error("peerIdentity cannot be modified")]
    ErrModifyingPeerIdentity,

    /// ErrModifyingCertificates indicates that an attempt to modify
    /// Certificates was made after PeerConnection has been initialized.
    #[error("certificates cannot be modified")]
    ErrModifyingCertificates,

    /// ErrNonCertificate indicates that there is no certificate
    #[error("no certificate")]
    ErrNonCertificate,

    /// ErrModifyingBundlePolicy indicates that an attempt to modify
    /// BundlePolicy was made after PeerConnection has been initialized.
    #[error("bundle policy cannot be modified")]
    ErrModifyingBundlePolicy,

    /// ErrModifyingRTCPMuxPolicy indicates that an attempt to modify
    /// RTCPMuxPolicy was made after PeerConnection has been initialized.
    #[error("rtcp mux policy cannot be modified")]
    ErrModifyingRTCPMuxPolicy,

    /// ErrModifyingICECandidatePoolSize indicates that an attempt to modify
    /// ICECandidatePoolSize was made after PeerConnection has been initialized.
    #[error("ice candidate pool size cannot be modified")]
    ErrModifyingICECandidatePoolSize,

    /// ErrStringSizeLimit indicates that the character size limit of string is
    /// exceeded. The limit is hardcoded to 65535 according to specifications.
    #[error("data channel label exceeds size limit")]
    ErrStringSizeLimit,

    /// ErrMaxDataChannelID indicates that the maximum number ID that could be
    /// specified for a data channel has been exceeded.
    #[error("maximum number ID for datachannel specified")]
    ErrMaxDataChannelID,

    /// ErrNegotiatedWithoutID indicates that an attempt to create a data channel
    /// was made while setting the negotiated option to true without providing
    /// the negotiated channel ID.
    #[error("negotiated set without channel id")]
    ErrNegotiatedWithoutID,

    /// ErrRetransmitsOrPacketLifeTime indicates that an attempt to create a data
    /// channel was made with both options max_packet_life_time and max_retransmits
    /// set together. Such configuration is not supported by the specification
    /// and is mutually exclusive.
    #[error("both max_packet_life_time and max_retransmits was set")]
    ErrRetransmitsOrPacketLifeTime,

    /// ErrCodecNotFound is returned when a codec search to the Media Engine fails
    #[error("codec not found")]
    ErrCodecNotFound,

    /// ErrNoRemoteDescription indicates that an operation was rejected because
    /// the remote description is not set
    #[error("remote description is not set")]
    ErrNoRemoteDescription,

    /// ErrIncorrectSDPSemantics indicates that the PeerConnection was configured to
    /// generate SDP Answers with different SDP Semantics than the received Offer
    #[error("offer SDP semantics does not match configuration")]
    ErrIncorrectSDPSemantics,

    /// ErrIncorrectSignalingState indicates that the signaling state of PeerConnection is not correct
    #[error("operation can not be run in current signaling state")]
    ErrIncorrectSignalingState,

    /// ErrProtocolTooLarge indicates that value given for a DataChannelInit protocol is
    /// longer then 65535 bytes
    #[error("protocol is larger then 65535 bytes")]
    ErrProtocolTooLarge,

    /// ErrSenderNotCreatedByConnection indicates remove_track was called with a
    /// [`rtp_sender::RTCRtpSender`] not created by this PeerConnection
    #[error("RtpSender not created by this PeerConnection")]
    ErrSenderNotCreatedByConnection,

    /// ErrSenderInitialTrackIdAlreadySet indicates a second call to
    /// `RTCRtpSender::set_initial_track_id` which is not allowed. Purely internal error, should not happen in practice.
    #[error("RtpSender's initial_track_id has already been set")]
    ErrSenderInitialTrackIdAlreadySet,

    /// ErrSessionDescriptionNoFingerprint indicates set_remote_description was called with a SessionDescription that has no
    /// fingerprint
    #[error("set_remote_description called with no fingerprint")]
    ErrSessionDescriptionNoFingerprint,

    /// ErrSessionDescriptionInvalidFingerprint indicates set_remote_description was called with a SessionDescription that
    /// has an invalid fingerprint
    #[error("set_remote_description called with an invalid fingerprint")]
    ErrSessionDescriptionInvalidFingerprint,

    /// ErrSessionDescriptionConflictingFingerprints indicates set_remote_description was called with a SessionDescription that
    /// has an conflicting fingerprints
    #[error("set_remote_description called with multiple conflicting fingerprint")]
    ErrSessionDescriptionConflictingFingerprints,

    /// ErrSessionDescriptionMissingIceUfrag indicates set_remote_description was called with a SessionDescription that
    /// is missing an ice-ufrag value
    #[error("set_remote_description called with no ice-ufrag")]
    ErrSessionDescriptionMissingIceUfrag,

    /// ErrSessionDescriptionMissingIcePwd indicates set_remote_description was called with a SessionDescription that
    /// is missing an ice-pwd value
    #[error("set_remote_description called with no ice-pwd")]
    ErrSessionDescriptionMissingIcePwd,

    /// ErrSessionDescriptionConflictingIceUfrag  indicates set_remote_description was called with a SessionDescription that
    /// contains multiple conflicting ice-ufrag values
    #[error("set_remote_description called with multiple conflicting ice-ufrag values")]
    ErrSessionDescriptionConflictingIceUfrag,

    /// ErrSessionDescriptionConflictingIcePwd indicates set_remote_description was called with a SessionDescription that
    /// contains multiple conflicting ice-pwd values
    #[error("set_remote_description called with multiple conflicting ice-pwd values")]
    ErrSessionDescriptionConflictingIcePwd,

    /// ErrNoSRTPProtectionProfile indicates that the DTLS handshake completed and no SRTP Protection Profile was chosen
    #[error("DTLS Handshake completed and no SRTP Protection Profile was chosen")]
    ErrNoSRTPProtectionProfile,

    /// ErrFailedToGenerateCertificateFingerprint indicates that we failed to generate the fingerprint used for comparing certificates
    #[error("failed to generate certificate fingerprint")]
    ErrFailedToGenerateCertificateFingerprint,

    /// ErrNoCodecsAvailable indicates that operation isn't possible because the MediaEngine has no codecs available
    #[error("operation failed no codecs are available")]
    ErrNoCodecsAvailable,

    /// ErrUnsupportedCodec indicates the remote peer doesn't support the requested codec
    #[error("unable to start track, codec is not supported by remote")]
    ErrUnsupportedCodec,

    /// ErrSenderWithNoCodecs indicates that a RTPSender was created without any codecs. To send media the MediaEngine needs at
    /// least one configured codec.
    #[error("unable to populate media section, RTPSender created with no codecs")]
    ErrSenderWithNoCodecs,

    /// ErrRTPSenderNewTrackHasIncorrectKind indicates that the new track is of a different kind than the previous/original
    #[error("new track must be of the same kind as previous")]
    ErrRTPSenderNewTrackHasIncorrectKind,

    /// ErrRTPSenderNewTrackHasIncorrectEnvelope indicates that the new track has a different envelope than the previous/original
    #[error("new track must have the same envelope as previous")]
    ErrRTPSenderNewTrackHasIncorrectEnvelope,

    /// ErrRTPSenderDataSent indicates that the sequence number transformer tries to be enabled after the data sending began
    #[error("Sequence number transformer must be enabled before sending data")]
    ErrRTPSenderDataSent,

    /// ErrRTPSenderSeqTransEnabled indicates that the sequence number transformer has been already enabled
    #[error("Sequence number transformer has been already enabled")]
    ErrRTPSenderSeqTransEnabled,

    /// ErrUnbindFailed indicates that a TrackLocal was not able to be unbind
    #[error("failed to unbind TrackLocal from PeerConnection")]
    ErrUnbindFailed,

    /// ErrNoPayloaderForCodec indicates that the requested codec does not have a payloader
    #[error("the requested codec does not have a payloader")]
    ErrNoPayloaderForCodec,

    /// ErrRegisterHeaderExtensionInvalidDirection indicates that a extension was registered with different
    /// directions for two different calls.
    #[error("a header extension must be registered with the same direction each time")]
    ErrRegisterHeaderExtensionInvalidDirection,

    /// ErrRegisterHeaderExtensionNoFreeID indicates that there was no extension ID available which
    /// in turn means that all 15 available id(1 through 14) have been used.
    #[error("no header extension ID was free to use(this means the maximum of 15 extensions have been registered)")]
    ErrRegisterHeaderExtensionNoFreeID,

    /// ErrSimulcastProbeOverflow indicates that too many Simulcast probe streams are in flight and the requested SSRC was ignored
    #[error("simulcast probe limit has been reached, new SSRC has been discarded")]
    ErrSimulcastProbeOverflow,

    #[error("enable detaching by calling webrtc.DetachDataChannels()")]
    ErrDetachNotEnabled,
    #[error("datachannel not opened yet, try calling Detach from OnOpen")]
    ErrDetachBeforeOpened,
    #[error("the DTLS transport has not started yet")]
    ErrDtlsTransportNotStarted,
    #[error("failed extracting keys from DTLS for SRTP")]
    ErrDtlsKeyExtractionFailed,
    #[error("failed to start SRTP")]
    ErrFailedToStartSRTP,
    #[error("failed to start SRTCP")]
    ErrFailedToStartSRTCP,
    #[error("attempted to start DTLSTransport that is not in new state")]
    ErrInvalidDTLSStart,
    #[error("peer didn't provide certificate via DTLS")]
    ErrNoRemoteCertificate,
    #[error("identity provider is not implemented")]
    ErrIdentityProviderNotImplemented,
    #[error("remote certificate does not match any fingerprint")]
    ErrNoMatchingCertificateFingerprint,
    #[error("unsupported fingerprint algorithm")]
    ErrUnsupportedFingerprintAlgorithm,
    #[error("ICE connection not started")]
    ErrICEConnectionNotStarted,
    #[error("unknown candidate type")]
    ErrICECandidateTypeUnknown,
    #[error("cannot convert ice.CandidateType into webrtc.ICECandidateType, invalid type")]
    ErrICEInvalidConvertCandidateType,
    #[error("ICEAgent does not exist")]
    ErrICEAgentNotExist,
    #[error("unable to convert ICE candidates to ICECandidates")]
    ErrICECandidatesConversionFailed,
    #[error("unknown ICE Role")]
    ErrICERoleUnknown,
    #[error("unknown protocol")]
    ErrICEProtocolUnknown,
    #[error("gatherer not started")]
    ErrICEGathererNotStarted,
    #[error("unknown network type")]
    ErrNetworkTypeUnknown,
    #[error("new sdp does not match previous offer")]
    ErrSDPDoesNotMatchOffer,
    #[error("new sdp does not match previous answer")]
    ErrSDPDoesNotMatchAnswer,
    #[error("provided value is not a valid enum value of type SDPType")]
    ErrPeerConnSDPTypeInvalidValue,
    #[error("invalid state change op")]
    ErrPeerConnStateChangeInvalid,
    #[error("unhandled state change op")]
    ErrPeerConnStateChangeUnhandled,
    #[error("invalid SDP type supplied to SetLocalDescription()")]
    ErrPeerConnSDPTypeInvalidValueSetLocalDescription,
    #[error("remoteDescription contained media section without mid value")]
    ErrPeerConnRemoteDescriptionWithoutMidValue,
    #[error("remoteDescription has not been set yet")]
    ErrPeerConnRemoteDescriptionNil,
    #[error("single media section has an explicit SSRC")]
    ErrPeerConnSingleMediaSectionHasExplicitSSRC,
    #[error("could not add transceiver for remote SSRC")]
    ErrPeerConnRemoteSSRCAddTransceiver,
    #[error("mid RTP Extensions required for Simulcast")]
    ErrPeerConnSimulcastMidRTPExtensionRequired,
    #[error("stream id RTP Extensions required for Simulcast")]
    ErrPeerConnSimulcastStreamIDRTPExtensionRequired,
    #[error("incoming SSRC failed Simulcast probing")]
    ErrPeerConnSimulcastIncomingSSRCFailed,
    #[error("failed collecting stats")]
    ErrPeerConnStatsCollectionFailed,
    #[error("add_transceiver_from_kind only accepts one RTPTransceiverInit")]
    ErrPeerConnAddTransceiverFromKindOnlyAcceptsOne,
    #[error("add_transceiver_from_track only accepts one RTPTransceiverInit")]
    ErrPeerConnAddTransceiverFromTrackOnlyAcceptsOne,
    #[error("add_transceiver_from_kind currently only supports recvonly")]
    ErrPeerConnAddTransceiverFromKindSupport,
    #[error("add_transceiver_from_track currently only supports sendonly and sendrecv")]
    ErrPeerConnAddTransceiverFromTrackSupport,
    #[error("TODO set_identity_provider")]
    ErrPeerConnSetIdentityProviderNotImplemented,
    #[error("write_rtcp failed to open write_stream")]
    ErrPeerConnWriteRTCPOpenWriteStream,
    #[error("cannot find transceiver with mid")]
    ErrPeerConnTransceiverMidNil,
    #[error("DTLSTransport must not be nil")]
    ErrRTPReceiverDTLSTransportNil,
    #[error("Receive has already been called")]
    ErrRTPReceiverReceiveAlreadyCalled,
    #[error("unable to find stream for Track with SSRC")]
    ErrRTPReceiverWithSSRCTrackStreamNotFound,
    #[error("no trackStreams found for SSRC")]
    ErrRTPReceiverForSSRCTrackStreamNotFound,
    #[error("no trackStreams found for RID")]
    ErrRTPReceiverForRIDTrackStreamNotFound,
    #[error("invalid RTP Receiver transition from {from} to {to}")]
    ErrRTPReceiverStateChangeInvalid {
        from: rtp_receiver::State,
        to: rtp_receiver::State,
    },
    #[error("Track must not be nil")]
    ErrRTPSenderTrackNil,
    #[error("Sender has already been stopped")]
    ErrRTPSenderStopped,
    #[error("Sender Track has been removed or replaced to nil")]
    ErrRTPSenderTrackRemoved,
    #[error("Sender cannot add encoding as rid is empty")]
    ErrRTPSenderRidNil,
    #[error("Sender cannot add encoding as there is no base track")]
    ErrRTPSenderNoBaseEncoding,
    #[error("Sender cannot add encoding as provided track does not match base track")]
    ErrRTPSenderBaseEncodingMismatch,
    #[error("Sender cannot encoding due to RID collision")]
    ErrRTPSenderRIDCollision,
    #[error("Sender does not have track for RID")]
    ErrRTPSenderNoTrackForRID,
    #[error("RTPSender must not be nil")]
    ErrRTPSenderNil,
    #[error("RTPReceiver must not be nil")]
    ErrRTPReceiverNil,
    #[error("DTLSTransport must not be nil")]
    ErrRTPSenderDTLSTransportNil,
    #[error("Send has already been called")]
    ErrRTPSenderSendAlreadyCalled,
    #[error("errRTPSenderTrackNil")]
    ErrRTPTransceiverCannotChangeMid,
    #[error("invalid state change in RTPTransceiver.setSending")]
    ErrRTPTransceiverSetSendingInvalidState,
    #[error("unsupported codec type by this transceiver")]
    ErrRTPTransceiverCodecUnsupported,
    #[error("DTLS not established")]
    ErrSCTPTransportDTLS,
    #[error("add_transceiver_sdp() called with 0 transceivers")]
    ErrSDPZeroTransceivers,
    #[error("invalid Media Section. Media + DataChannel both enabled")]
    ErrSDPMediaSectionMediaDataChanInvalid,
    #[error(
        "invalid Media Section. Can not have multiple tracks in one MediaSection in UnifiedPlan"
    )]
    ErrSDPMediaSectionMultipleTrackInvalid,
    #[error("set_answering_dtlsrole must DTLSRoleClient or DTLSRoleServer")]
    ErrSettingEngineSetAnsweringDTLSRole,
    #[error("can't rollback from stable state")]
    ErrSignalingStateCannotRollback,
    #[error(
        "invalid proposed signaling state transition from {} applying {} {}",
        from,
        if *is_local { "local" } else {  "remote" },
        applying
    )]
    ErrSignalingStateProposedTransitionInvalid {
        from: RTCSignalingState,
        applying: RTCSdpType,
        is_local: bool,
    },
    #[error("cannot convert to StatsICECandidatePairStateSucceeded invalid ice candidate state")]
    ErrStatsICECandidateStateInvalid,
    #[error("ICETransport can only be called in ICETransportStateNew")]
    ErrICETransportNotInNew,
    #[error("bad Certificate PEM format")]
    ErrCertificatePEMFormatError,
    #[error("SCTP is not established")]
    ErrSCTPNotEstablished,

    #[error("DataChannel is not opened")]
    ErrClosedPipe,
    #[error("Interceptor is not bind")]
    ErrInterceptorNotBind,
    #[error("excessive retries in CreateOffer")]
    ErrExcessiveRetries,

    #[error("not long enough to be a RTP Packet")]
    ErrRTPTooShort,

    #[error("{0}")]
    Util(#[from] util::Error),
    #[error("{0}")]
    Ice(#[from] ice::Error),
    #[error("{0}")]
    Srtp(#[from] srtp::Error),
    #[error("{0}")]
    Dtls(#[from] dtls::Error),
    #[error("{0}")]
    Data(#[from] data::Error),
    #[error("{0}")]
    Sctp(#[from] sctp::Error),
    #[error("{0}")]
    Sdp(#[from] sdp::Error),
    #[error("{0}")]
    Interceptor(#[from] interceptor::Error),
    #[error("{0}")]
    Rtcp(#[from] rtcp::Error),
    #[error("{0}")]
    Rtp(#[from] rtp::Error),

    #[error("utf-8 error: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("{0}")]
    RcGen(#[from] rcgen::Error),
    #[error("mpsc send: {0}")]
    MpscSend(String),
    #[error("parse int: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("parse url: {0}")]
    ParseUrl(#[from] url::ParseError),

    /// Error parsing a given PEM string.
    #[error("invalid PEM: {0}")]
    InvalidPEM(String),

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

pub type OnErrorHdlrFn =
    Box<dyn (FnMut(Error) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

// Because Tokio SendError is parameterized, we sadly lose the backtrace.
impl<T> From<MpscSendError<T>> for Error {
    fn from(e: MpscSendError<T>) -> Self {
        Error::MpscSend(e.to_string())
    }
}

impl From<Error> for interceptor::Error {
    fn from(e: Error) -> Self {
        // this is a bit lol, but we do preserve the stack trace
        interceptor::Error::Util(util::Error::from_std(e))
    }
}

impl PartialEq<ice::Error> for Error {
    fn eq(&self, other: &ice::Error) -> bool {
        if let Error::Ice(e) = self {
            return e == other;
        }
        false
    }
}

/// flatten_errs flattens multiple errors into one
pub fn flatten_errs(errs: Vec<impl Into<Error>>) -> Result<()> {
    if errs.is_empty() {
        Ok(())
    } else {
        let errs_strs: Vec<String> = errs.into_iter().map(|e| e.into().to_string()).collect();
        Err(Error::new(errs_strs.join("\n")))
    }
}
