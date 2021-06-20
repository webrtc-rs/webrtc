use thiserror::Error;

#[derive(Debug, Error)] //, PartialEq, Clone
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
    /// channel was made with both options MaxPacketLifeTime and MaxRetransmits
    /// set together. Such configuration is not supported by the specification
    /// and is mutually exclusive.
    #[error("both MaxPacketLifeTime and MaxRetransmits was set")]
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

    /// ErrSenderNotCreatedByConnection indicates RemoveTrack was called with a RtpSender not created
    /// by this PeerConnection
    #[error("RtpSender not created by this PeerConnection")]
    ErrSenderNotCreatedByConnection,

    /// ErrSessionDescriptionNoFingerprint indicates SetRemoteDescription was called with a SessionDescription that has no
    /// fingerprint
    #[error("SetRemoteDescription called with no fingerprint")]
    ErrSessionDescriptionNoFingerprint,

    /// ErrSessionDescriptionInvalidFingerprint indicates SetRemoteDescription was called with a SessionDescription that
    /// has an invalid fingerprint
    #[error("SetRemoteDescription called with an invalid fingerprint")]
    ErrSessionDescriptionInvalidFingerprint,

    /// ErrSessionDescriptionConflictingFingerprints indicates SetRemoteDescription was called with a SessionDescription that
    /// has an conflicting fingerprints
    #[error("SetRemoteDescription called with multiple conflicting fingerprint")]
    ErrSessionDescriptionConflictingFingerprints,

    /// ErrSessionDescriptionMissingIceUfrag indicates SetRemoteDescription was called with a SessionDescription that
    /// is missing an ice-ufrag value
    #[error("SetRemoteDescription called with no ice-ufrag")]
    ErrSessionDescriptionMissingIceUfrag,

    /// ErrSessionDescriptionMissingIcePwd indicates SetRemoteDescription was called with a SessionDescription that
    /// is missing an ice-pwd value
    #[error("SetRemoteDescription called with no ice-pwd")]
    ErrSessionDescriptionMissingIcePwd,

    /// ErrSessionDescriptionConflictingIceUfrag  indicates SetRemoteDescription was called with a SessionDescription that
    /// contains multiple conflicting ice-ufrag values
    #[error("SetRemoteDescription called with multiple conflicting ice-ufrag values")]
    ErrSessionDescriptionConflictingIceUfrag,

    /// ErrSessionDescriptionConflictingIcePwd indicates SetRemoteDescription was called with a SessionDescription that
    /// contains multiple conflicting ice-pwd values
    #[error("SetRemoteDescription called with multiple conflicting ice-pwd values")]
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

    /// ErrUnbindFailed indicates that a TrackLocal was not able to be unbind
    #[error("failed to unbind TrackLocal from PeerConnection")]
    ErrUnbindFailed,

    /// ErrNoPayloaderForCodec indicates that the requested codec does not have a payloader
    #[error("the requested codec does not have a payloader")]
    ErrNoPayloaderForCodec,

    /// ErrRegisterHeaderExtensionInvalidDirection indicates that a extension was registered with a direction besides `sendonly` or `recvonly`
    #[error("a header extension must be registered as 'recvonly', 'sendonly' or both")]
    ErrRegisterHeaderExtensionInvalidDirection,

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
    #[error("ICE connection not started")]
    ErrICEConnectionNotStarted,
    #[error("unknown candidate type")]
    ErrICECandidateTypeUnknown,
    #[error("cannot convert ice.CandidateType into webrtc.ICECandidateType, invalid type")]
    ErrICEInvalidConvertCandidateType,
    #[error("ICEAgent does not exist")]
    ErrICEAgentNotExist,
    #[error("unable to convert ICE candidates to ICECandidates")]
    ErrICECandiatesCoversionFailed,
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
    #[error("AddTransceiverFromKind only accepts one RTPTransceiverInit")]
    ErrPeerConnAddTransceiverFromKindOnlyAcceptsOne,
    #[error("AddTransceiverFromTrack only accepts one RTPTransceiverInit")]
    ErrPeerConnAddTransceiverFromTrackOnlyAcceptsOne,
    #[error("AddTransceiverFromKind currently only supports recvonly")]
    ErrPeerConnAddTransceiverFromKindSupport,
    #[error("AddTransceiverFromTrack currently only supports sendonly and sendrecv")]
    ErrPeerConnAddTransceiverFromTrackSupport,
    #[error("TODO SetIdentityProvider")]
    ErrPeerConnSetIdentityProviderNotImplemented,
    #[error("write_rtcp failed to open WriteStream")]
    ErrPeerConnWriteRTCPOpenWriteStream,
    #[error("cannot find transceiver with mid")]
    ErrPeerConnTranscieverMidNil,
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
    #[error("Track must not be nil")]
    ErrRTPSenderTrackNil,
    #[error("DTLSTransport must not be nil")]
    ErrRTPSenderDTLSTransportNil,
    #[error("Send has already been called")]
    ErrRTPSenderSendAlreadyCalled,
    #[error("errRTPSenderTrackNil")]
    ErrRTPTransceiverCannotChangeMid,
    #[error("invalid state change in RTPTransceiver.setSending")]
    ErrRTPTransceiverSetSendingInvalidState,
    #[error("DTLS not established")]
    ErrSCTPTransportDTLS,
    #[error("addTransceiverSDP() called with 0 transceivers")]
    ErrSDPZeroTransceivers,
    #[error("invalid Media Section. Media + DataChannel both enabled")]
    ErrSDPMediaSectionMediaDataChanInvalid,
    #[error(
        "invalid Media Section. Can not have multiple tracks in one MediaSection in UnifiedPlan"
    )]
    ErrSDPMediaSectionMultipleTrackInvalid,
    #[error("SetAnsweringDTLSRole must DTLSRoleClient or DTLSRoleServer")]
    ErrSettingEngineSetAnsweringDTLSRole,
    #[error("can't rollback from stable state")]
    ErrSignalingStateCannotRollback,
    #[error("invalid proposed signaling state transition")]
    ErrSignalingStateProposedTransitionInvalid,
    #[error("cannot convert to StatsICECandidatePairStateSucceeded invalid ice candidate state")]
    ErrStatsICECandidateStateInvalid,
    #[error("ICETransport can only be called in ICETransportStateNew")]
    ErrICETransportNotInNew,
    #[error("bad Certificate PEM format")]
    ErrCertificatePEMFormatError,

    #[error("IoError: {0}")]
    ErrIoError(#[from] std::io::Error),

    #[error("SdpError: {0}")]
    ErrSdpError(#[from] sdp::error::Error),
    #[error("UtilError: {0}")]
    ErrUtilError(#[from] util::error::Error),
    #[error("RtcpError: {0}")]
    ErrRtcpError(#[from] rtcp::error::Error),
    #[error("RtpError: {0}")]
    ErrRtpError(#[from] rtp::error::Error),
    #[error("SrtpError: {0}")]
    ErrSrtpError(#[from] srtp::error::Error),

    #[error("Other errors: {0}")]
    ErrOthers(String),
}
