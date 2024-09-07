use std::collections::VecDeque;
use std::sync::Weak;

use arc_swap::ArcSwapOption;
use portable_atomic::AtomicIsize;
use smol_str::SmolStr;
use tokio::time::Instant;
use util::Unmarshal;

use super::*;
use crate::rtp_transceiver::create_stream_info;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::{
    InboundRTPStats, OutboundRTPStats, RTCStatsType, RemoteInboundRTPStats, RemoteOutboundRTPStats,
    StatsReportType,
};
use crate::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use crate::track::TrackStream;
use crate::SDP_ATTRIBUTE_RID;

pub(crate) struct PeerConnectionInternal {
    /// a value containing the last known greater mid value
    /// we internally generate mids as numbers. Needed since JSEP
    /// requires that when reusing a media section a new unique mid
    /// should be defined (see JSEP 3.4.1).
    pub(super) greater_mid: AtomicIsize,
    pub(super) sdp_origin: Mutex<::sdp::description::session::Origin>,
    pub(super) last_offer: Mutex<String>,
    pub(super) last_answer: Mutex<String>,

    pub(super) on_negotiation_needed_handler: Arc<ArcSwapOption<Mutex<OnNegotiationNeededHdlrFn>>>,
    pub(super) is_closed: Arc<AtomicBool>,

    /// ops is an operations queue which will ensure the enqueued actions are
    /// executed in order. It is used for asynchronously, but serially processing
    /// remote and local descriptions
    pub(crate) ops: Arc<Operations>,
    pub(super) negotiation_needed_state: Arc<AtomicU8>,
    pub(super) is_negotiation_needed: Arc<AtomicBool>,
    pub(super) signaling_state: Arc<AtomicU8>,

    pub(super) ice_transport: Arc<RTCIceTransport>,
    pub(super) dtls_transport: Arc<RTCDtlsTransport>,
    pub(super) on_peer_connection_state_change_handler:
        Arc<ArcSwapOption<Mutex<OnPeerConnectionStateChangeHdlrFn>>>,
    pub(super) peer_connection_state: Arc<AtomicU8>,
    pub(super) ice_connection_state: Arc<AtomicU8>,

    pub(super) sctp_transport: Arc<RTCSctpTransport>,
    pub(super) rtp_transceivers: Arc<Mutex<Vec<Arc<RTCRtpTransceiver>>>>,

    pub(super) on_track_handler: Arc<ArcSwapOption<Mutex<OnTrackHdlrFn>>>,
    pub(super) on_signaling_state_change_handler:
        ArcSwapOption<Mutex<OnSignalingStateChangeHdlrFn>>,
    pub(super) on_ice_connection_state_change_handler:
        Arc<ArcSwapOption<Mutex<OnICEConnectionStateChangeHdlrFn>>>,
    pub(super) on_data_channel_handler: Arc<ArcSwapOption<Mutex<OnDataChannelHdlrFn>>>,

    pub(super) ice_gatherer: Arc<RTCIceGatherer>,

    pub(super) current_local_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(super) current_remote_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(super) pending_local_description: Arc<Mutex<Option<RTCSessionDescription>>>,
    pub(super) pending_remote_description: Arc<Mutex<Option<RTCSessionDescription>>>,

    // A reference to the associated API state used by this connection
    pub(super) setting_engine: Arc<SettingEngine>,
    pub(crate) media_engine: Arc<MediaEngine>,
    pub(super) interceptor: Weak<dyn Interceptor + Send + Sync>,
    stats_interceptor: Arc<stats::StatsInterceptor>,
}

impl PeerConnectionInternal {
    pub(super) async fn new(
        api: &API,
        interceptor: Weak<dyn Interceptor + Send + Sync>,
        stats_interceptor: Arc<stats::StatsInterceptor>,
        mut configuration: RTCConfiguration,
    ) -> Result<(Arc<Self>, RTCConfiguration)> {
        // Create the ice gatherer
        let ice_gatherer = Arc::new(api.new_ice_gatherer(RTCIceGatherOptions {
            ice_servers: configuration.get_ice_servers(),
            ice_gather_policy: configuration.ice_transport_policy,
        })?);

        // Create the ICE transport
        let ice_transport = Arc::new(api.new_ice_transport(Arc::clone(&ice_gatherer)));

        // Create the DTLS transport
        let certificates = configuration.certificates.drain(..).collect();
        let dtls_transport =
            Arc::new(api.new_dtls_transport(Arc::clone(&ice_transport), certificates)?);

        // Create the SCTP transport
        let sctp_transport = Arc::new(api.new_sctp_transport(Arc::clone(&dtls_transport))?);

        let pc = Arc::new(PeerConnectionInternal {
            greater_mid: AtomicIsize::new(-1),
            sdp_origin: Mutex::new(Default::default()),
            last_offer: Mutex::new("".to_owned()),
            last_answer: Mutex::new("".to_owned()),

            on_negotiation_needed_handler: Arc::new(ArcSwapOption::empty()),
            ops: Arc::new(Operations::new()),
            is_closed: Arc::new(AtomicBool::new(false)),
            is_negotiation_needed: Arc::new(AtomicBool::new(false)),
            negotiation_needed_state: Arc::new(AtomicU8::new(NegotiationNeededState::Empty as u8)),
            signaling_state: Arc::new(AtomicU8::new(RTCSignalingState::Stable as u8)),
            ice_transport,
            dtls_transport,
            ice_connection_state: Arc::new(AtomicU8::new(RTCIceConnectionState::New as u8)),
            sctp_transport,
            rtp_transceivers: Arc::new(Default::default()),
            on_track_handler: Arc::new(ArcSwapOption::empty()),
            on_signaling_state_change_handler: ArcSwapOption::empty(),
            on_ice_connection_state_change_handler: Arc::new(ArcSwapOption::empty()),
            on_data_channel_handler: Arc::new(Default::default()),
            ice_gatherer,
            current_local_description: Arc::new(Default::default()),
            current_remote_description: Arc::new(Default::default()),
            pending_local_description: Arc::new(Default::default()),
            peer_connection_state: Arc::new(AtomicU8::new(RTCPeerConnectionState::New as u8)),

            setting_engine: Arc::clone(&api.setting_engine),
            media_engine: if !api.setting_engine.disable_media_engine_copy {
                Arc::new(api.media_engine.clone_to())
            } else {
                Arc::clone(&api.media_engine)
            },
            interceptor,
            stats_interceptor,
            on_peer_connection_state_change_handler: Arc::new(ArcSwapOption::empty()),
            pending_remote_description: Arc::new(Default::default()),
        });

        // Wire up the ice transport connection state change handler
        let ice_connection_state = Arc::clone(&pc.ice_connection_state);
        let peer_connection_state = Arc::clone(&pc.peer_connection_state);
        let is_closed = Arc::clone(&pc.is_closed);
        let dtls_transport = Arc::clone(&pc.dtls_transport);
        let on_ice_connection_state_change_handler =
            Arc::clone(&pc.on_ice_connection_state_change_handler);
        let on_peer_connection_state_change_handler =
            Arc::clone(&pc.on_peer_connection_state_change_handler);

        pc.ice_transport.on_connection_state_change(Box::new(
            move |state: RTCIceTransportState| {
                let cs = match state {
                    RTCIceTransportState::New => RTCIceConnectionState::New,
                    RTCIceTransportState::Checking => RTCIceConnectionState::Checking,
                    RTCIceTransportState::Connected => RTCIceConnectionState::Connected,
                    RTCIceTransportState::Completed => RTCIceConnectionState::Completed,
                    RTCIceTransportState::Failed => RTCIceConnectionState::Failed,
                    RTCIceTransportState::Disconnected => RTCIceConnectionState::Disconnected,
                    RTCIceTransportState::Closed => RTCIceConnectionState::Closed,
                    _ => {
                        log::warn!("on_connection_state_change: unhandled ICE state: {}", state);
                        return Box::pin(async {});
                    }
                };

                let dtls_transport = Arc::clone(&dtls_transport);
                let ice_connection_state = Arc::clone(&ice_connection_state);
                let on_ice_connection_state_change_handler =
                    Arc::clone(&on_ice_connection_state_change_handler);
                let on_peer_connection_state_change_handler =
                    Arc::clone(&on_peer_connection_state_change_handler);
                let is_closed = Arc::clone(&is_closed);
                let peer_connection_state = Arc::clone(&peer_connection_state);
                Box::pin(async move {
                    RTCPeerConnection::do_ice_connection_state_change(
                        &on_ice_connection_state_change_handler,
                        &ice_connection_state,
                        cs,
                    )
                    .await;

                    let dtls_transport_state = dtls_transport.state();
                    RTCPeerConnection::update_connection_state(
                        &on_peer_connection_state_change_handler,
                        &is_closed,
                        &peer_connection_state,
                        cs,
                        dtls_transport_state,
                    )
                    .await;
                })
            },
        ));

        // Wire up the on datachannel handler
        let on_data_channel_handler = Arc::clone(&pc.on_data_channel_handler);
        pc.sctp_transport
            .on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
                let on_data_channel_handler = Arc::clone(&on_data_channel_handler);
                Box::pin(async move {
                    if let Some(handler) = &*on_data_channel_handler.load() {
                        let mut f = handler.lock().await;
                        f(d).await;
                    }
                })
            }));

        Ok((pc, configuration))
    }

    pub(super) async fn start_rtp(
        self: &Arc<Self>,
        is_renegotiation: bool,
        remote_desc: Arc<RTCSessionDescription>,
    ) -> Result<()> {
        let mut track_details = if let Some(parsed) = &remote_desc.parsed {
            track_details_from_sdp(parsed, false)
        } else {
            vec![]
        };

        let current_transceivers = {
            let current_transceivers = self.rtp_transceivers.lock().await;
            current_transceivers.clone()
        };

        if !is_renegotiation {
            self.undeclared_media_processor();
        } else {
            for t in &current_transceivers {
                let receiver = t.receiver().await;
                let tracks = receiver.tracks().await;
                if tracks.is_empty() {
                    continue;
                }

                let mut receiver_needs_stopped = false;

                for t in tracks {
                    if !t.rid().is_empty() {
                        if let Some(details) =
                            track_details_for_rid(&track_details, SmolStr::from(t.rid()))
                        {
                            t.set_id(details.id.clone());
                            t.set_stream_id(details.stream_id.clone());
                            continue;
                        }
                    } else if t.ssrc() != 0 {
                        if let Some(details) = track_details_for_ssrc(&track_details, t.ssrc()) {
                            t.set_id(details.id.clone());
                            t.set_stream_id(details.stream_id.clone());
                            continue;
                        }
                    }

                    receiver_needs_stopped = true;
                }

                if !receiver_needs_stopped {
                    continue;
                }

                log::info!("Stopping receiver {:?}", receiver);
                if let Err(err) = receiver.stop().await {
                    log::warn!("Failed to stop RtpReceiver: {}", err);
                    continue;
                }

                let interceptor = self
                    .interceptor
                    .upgrade()
                    .ok_or(Error::ErrInterceptorNotBind)?;

                let receiver = Arc::new(RTCRtpReceiver::new(
                    self.setting_engine.get_receive_mtu(),
                    receiver.kind(),
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    interceptor,
                ));
                t.set_receiver(receiver).await;
            }
        }

        self.start_rtp_receivers(&mut track_details, &current_transceivers)
            .await?;
        if let Some(parsed) = &remote_desc.parsed {
            if have_application_media_section(parsed) {
                self.start_sctp().await;
            }
        }

        Ok(())
    }

    /// undeclared_media_processor handles RTP/RTCP packets that don't match any a:ssrc lines
    fn undeclared_media_processor(self: &Arc<Self>) {
        let dtls_transport = Arc::clone(&self.dtls_transport);
        let is_closed = Arc::clone(&self.is_closed);
        let pci = Arc::clone(self);

        // SRTP acceptor
        tokio::spawn(async move {
            let simulcast_routine_count = Arc::new(AtomicU64::new(0));
            loop {
                let srtp_session = match dtls_transport.get_srtp_session().await {
                    Some(s) => s,
                    None => {
                        log::warn!("undeclared_media_processor failed to open SrtpSession");
                        return;
                    }
                };

                let stream = match srtp_session.accept().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::warn!("Failed to accept RTP {}", err);
                        return;
                    }
                };

                if is_closed.load(Ordering::SeqCst) {
                    if let Err(err) = stream.close().await {
                        log::warn!("Failed to close RTP stream {}", err);
                    }
                    continue;
                }

                if simulcast_routine_count.fetch_add(1, Ordering::SeqCst) + 1
                    >= SIMULCAST_MAX_PROBE_ROUTINES
                {
                    simulcast_routine_count.fetch_sub(1, Ordering::SeqCst);
                    log::warn!("{:?}", Error::ErrSimulcastProbeOverflow);
                    continue;
                }

                {
                    let dtls_transport = Arc::clone(&dtls_transport);
                    let simulcast_routine_count = Arc::clone(&simulcast_routine_count);
                    let pci = Arc::clone(&pci);
                    tokio::spawn(async move {
                        let ssrc = stream.get_ssrc();

                        dtls_transport
                            .store_simulcast_stream(ssrc, Arc::clone(&stream))
                            .await;

                        if let Err(err) = pci.handle_incoming_ssrc(stream, ssrc).await {
                            log::error!(
                                "Incoming unhandled RTP ssrc({}), on_track will not be fired. {}",
                                ssrc,
                                err
                            );
                        }

                        simulcast_routine_count.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        });

        // SRTCP acceptor
        {
            let dtls_transport = Arc::clone(&self.dtls_transport);
            tokio::spawn(async move {
                loop {
                    let srtcp_session = match dtls_transport.get_srtcp_session().await {
                        Some(s) => s,
                        None => {
                            log::warn!("undeclared_media_processor failed to open SrtcpSession");
                            return;
                        }
                    };

                    let stream = match srtcp_session.accept().await {
                        Ok(stream) => stream,
                        Err(err) => {
                            log::warn!("Failed to accept RTCP {}", err);
                            return;
                        }
                    };
                    log::warn!(
                        "Incoming unhandled RTCP ssrc({}), on_track will not be fired",
                        stream.get_ssrc()
                    );
                }
            });
        }
    }

    /// start_rtp_receivers opens knows inbound SRTP streams from the remote_description
    async fn start_rtp_receivers(
        self: &Arc<Self>,
        incoming_tracks: &mut Vec<TrackDetails>,
        local_transceivers: &[Arc<RTCRtpTransceiver>],
    ) -> Result<()> {
        // Ensure we haven't already started a transceiver for this ssrc
        let mut filtered_tracks = incoming_tracks.clone();
        for incoming_track in incoming_tracks {
            // If we already have a TrackRemote for a given SSRC don't handle it again
            for t in local_transceivers {
                let receiver = t.receiver().await;
                for track in receiver.tracks().await {
                    for ssrc in &incoming_track.ssrcs {
                        if *ssrc == track.ssrc() {
                            filter_track_with_ssrc(&mut filtered_tracks, track.ssrc());
                        }
                    }
                }
            }
        }

        let mut unhandled_tracks = vec![]; // filtered_tracks[:0]
        for incoming_track in filtered_tracks.iter() {
            let mut track_handled = false;
            for t in local_transceivers {
                if t.mid().as_ref() != Some(&incoming_track.mid) {
                    continue;
                }

                if (incoming_track.kind != t.kind())
                    || (t.direction() != RTCRtpTransceiverDirection::Recvonly
                        && t.direction() != RTCRtpTransceiverDirection::Sendrecv)
                {
                    continue;
                }

                let receiver = t.receiver().await;
                if receiver.have_received().await {
                    continue;
                }
                PeerConnectionInternal::start_receiver(
                    self.setting_engine.get_receive_mtu(),
                    incoming_track,
                    receiver,
                    Arc::clone(t),
                    Arc::clone(&self.on_track_handler),
                )
                .await;
                track_handled = true;
            }

            if !track_handled {
                unhandled_tracks.push(incoming_track);
            }
        }

        Ok(())
    }

    /// Start SCTP subsystem
    async fn start_sctp(&self) {
        // Start sctp
        if let Err(err) = self
            .sctp_transport
            .start(SCTPTransportCapabilities {
                max_message_size: 0,
            })
            .await
        {
            log::warn!("Failed to start SCTP: {}", err);
            if let Err(err) = self.sctp_transport.stop().await {
                log::warn!("Failed to stop SCTPTransport: {}", err);
            }

            return;
        }

        // DataChannels that need to be opened now that SCTP is available
        // make a copy we may have incoming DataChannels mutating this while we open
        let data_channels = {
            let data_channels = self.sctp_transport.data_channels.lock().await;
            data_channels.clone()
        };

        let mut opened_dc_count = 0;
        for d in data_channels {
            if d.ready_state() == RTCDataChannelState::Connecting {
                if let Err(err) = d.open(Arc::clone(&self.sctp_transport)).await {
                    log::warn!("failed to open data channel: {}", err);
                    continue;
                }
                opened_dc_count += 1;
            }
        }

        self.sctp_transport
            .data_channels_opened
            .fetch_add(opened_dc_count, Ordering::SeqCst);
    }

    pub(super) async fn add_transceiver_from_kind(
        &self,
        kind: RTPCodecType,
        init: Option<RTCRtpTransceiverInit>,
    ) -> Result<Arc<RTCRtpTransceiver>> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let direction = init
            .map(|value| value.direction)
            .unwrap_or(RTCRtpTransceiverDirection::Sendrecv);

        let t = match direction {
            RTCRtpTransceiverDirection::Sendonly | RTCRtpTransceiverDirection::Sendrecv => {
                let codec = self
                    .media_engine
                    .get_codecs_by_kind(kind)
                    .first()
                    .map(|c| c.capability.clone())
                    .ok_or(Error::ErrNoCodecsAvailable)?;
                let track = Arc::new(TrackLocalStaticSample::new(
                    codec,
                    math_rand_alpha(16),
                    math_rand_alpha(16),
                ));
                self.new_transceiver_from_track(direction, track).await?
            }
            RTCRtpTransceiverDirection::Recvonly => {
                let interceptor = self
                    .interceptor
                    .upgrade()
                    .ok_or(Error::ErrInterceptorNotBind)?;
                let receiver = Arc::new(RTCRtpReceiver::new(
                    self.setting_engine.get_receive_mtu(),
                    kind,
                    Arc::clone(&self.dtls_transport),
                    Arc::clone(&self.media_engine),
                    Arc::clone(&interceptor),
                ));

                let sender = Arc::new(
                    RTCRtpSender::new(
                        None,
                        kind,
                        Arc::clone(&self.dtls_transport),
                        Arc::clone(&self.media_engine),
                        Arc::clone(&self.setting_engine),
                        interceptor,
                        false,
                    )
                    .await,
                );

                RTCRtpTransceiver::new(
                    receiver,
                    sender,
                    direction,
                    kind,
                    vec![],
                    Arc::clone(&self.media_engine),
                    Some(Box::new(self.make_negotiation_needed_trigger())),
                )
                .await
            }
            _ => return Err(Error::ErrPeerConnAddTransceiverFromKindSupport),
        };

        self.add_rtp_transceiver(Arc::clone(&t)).await;

        Ok(t)
    }

    pub(super) async fn new_transceiver_from_track(
        &self,
        direction: RTCRtpTransceiverDirection,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<Arc<RTCRtpTransceiver>> {
        let interceptor = self
            .interceptor
            .upgrade()
            .ok_or(Error::ErrInterceptorNotBind)?;

        if direction == RTCRtpTransceiverDirection::Unspecified {
            return Err(Error::ErrPeerConnAddTransceiverFromTrackSupport);
        }

        let r = Arc::new(RTCRtpReceiver::new(
            self.setting_engine.get_receive_mtu(),
            track.kind(),
            Arc::clone(&self.dtls_transport),
            Arc::clone(&self.media_engine),
            Arc::clone(&interceptor),
        ));

        let s = Arc::new(
            RTCRtpSender::new(
                Some(Arc::clone(&track)),
                track.kind(),
                Arc::clone(&self.dtls_transport),
                Arc::clone(&self.media_engine),
                Arc::clone(&self.setting_engine),
                Arc::clone(&interceptor),
                false,
            )
            .await,
        );

        Ok(RTCRtpTransceiver::new(
            r,
            s,
            direction,
            track.kind(),
            vec![],
            Arc::clone(&self.media_engine),
            Some(Box::new(self.make_negotiation_needed_trigger())),
        )
        .await)
    }

    /// add_rtp_transceiver appends t into rtp_transceivers
    /// and fires onNegotiationNeeded;
    /// caller of this method should hold `self.mu` lock
    pub(super) async fn add_rtp_transceiver(&self, t: Arc<RTCRtpTransceiver>) {
        {
            let mut rtp_transceivers = self.rtp_transceivers.lock().await;
            rtp_transceivers.push(t);
        }
        self.trigger_negotiation_needed().await;
    }

    /// Helper to trigger a negotiation needed.
    pub(crate) async fn trigger_negotiation_needed(&self) {
        RTCPeerConnection::do_negotiation_needed(self.create_negotiation_needed_params()).await;
    }

    /// Creates the parameters needed to trigger a negotiation needed.
    fn create_negotiation_needed_params(&self) -> NegotiationNeededParams {
        NegotiationNeededParams {
            on_negotiation_needed_handler: Arc::clone(&self.on_negotiation_needed_handler),
            is_closed: Arc::clone(&self.is_closed),
            ops: Arc::clone(&self.ops),
            negotiation_needed_state: Arc::clone(&self.negotiation_needed_state),
            is_negotiation_needed: Arc::clone(&self.is_negotiation_needed),
            signaling_state: Arc::clone(&self.signaling_state),
            check_negotiation_needed_params: CheckNegotiationNeededParams {
                sctp_transport: Arc::clone(&self.sctp_transport),
                rtp_transceivers: Arc::clone(&self.rtp_transceivers),
                current_local_description: Arc::clone(&self.current_local_description),
                current_remote_description: Arc::clone(&self.current_remote_description),
            },
        }
    }

    pub(crate) fn make_negotiation_needed_trigger(
        &self,
    ) -> impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send + Sync>> + Send + Sync {
        let params = self.create_negotiation_needed_params();
        move || {
            let params = params.clone();
            Box::pin(async move {
                let params = params.clone();
                RTCPeerConnection::do_negotiation_needed(params).await;
            })
        }
    }

    pub(super) async fn remote_description(&self) -> Option<RTCSessionDescription> {
        let pending_remote_description = self.pending_remote_description.lock().await;
        if pending_remote_description.is_some() {
            pending_remote_description.clone()
        } else {
            let current_remote_description = self.current_remote_description.lock().await;
            current_remote_description.clone()
        }
    }

    pub(super) fn set_gather_complete_handler(&self, f: OnGatheringCompleteHdlrFn) {
        self.ice_gatherer.on_gathering_complete(f);
    }

    /// Start all transports. PeerConnection now has enough state
    pub(super) async fn start_transports(
        self: &Arc<Self>,
        ice_role: RTCIceRole,
        dtls_role: DTLSRole,
        remote_ufrag: String,
        remote_pwd: String,
        fingerprint: String,
        fingerprint_hash: String,
    ) {
        // Start the ice transport
        if let Err(err) = self
            .ice_transport
            .start(
                &RTCIceParameters {
                    username_fragment: remote_ufrag,
                    password: remote_pwd,
                    ice_lite: false,
                },
                Some(ice_role),
            )
            .await
        {
            log::warn!("Failed to start manager ice: {}", err);
            return;
        }

        // Start the dtls_transport transport
        let result = self
            .dtls_transport
            .start(DTLSParameters {
                role: dtls_role,
                fingerprints: vec![RTCDtlsFingerprint {
                    algorithm: fingerprint_hash,
                    value: fingerprint,
                }],
            })
            .await;
        RTCPeerConnection::update_connection_state(
            &self.on_peer_connection_state_change_handler,
            &self.is_closed,
            &self.peer_connection_state,
            self.ice_connection_state.load(Ordering::SeqCst).into(),
            self.dtls_transport.state(),
        )
        .await;
        if let Err(err) = result {
            log::warn!("Failed to start manager dtls: {}", err);
        }
    }

    /// generate_unmatched_sdp generates an SDP that doesn't take remote state into account
    /// This is used for the initial call for CreateOffer
    pub(super) async fn generate_unmatched_sdp(
        &self,
        local_transceivers: Vec<Arc<RTCRtpTransceiver>>,
        use_identity: bool,
    ) -> Result<SessionDescription> {
        let d = SessionDescription::new_jsep_session_description(use_identity);

        let ice_params = self.ice_gatherer.get_local_parameters().await?;

        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let mut media_sections = vec![];

        for t in &local_transceivers {
            if t.stopped.load(Ordering::SeqCst) {
                // An "m=" section is generated for each
                // RtpTransceiver that has been added to the PeerConnection, excluding
                // any stopped RtpTransceivers;
                continue;
            }

            // TODO: This is dubious because of rollbacks.
            t.sender().await.set_negotiated();
            media_sections.push(MediaSection {
                id: t.mid().unwrap().to_string(),
                transceivers: vec![Arc::clone(t)],
                ..Default::default()
            });
        }

        if self
            .sctp_transport
            .data_channels_requested
            .load(Ordering::SeqCst)
            != 0
        {
            media_sections.push(MediaSection {
                id: format!("{}", media_sections.len()),
                data: true,
                ..Default::default()
            });
        }

        let dtls_fingerprints = if let Some(cert) = self.dtls_transport.certificates.first() {
            cert.get_fingerprints()
        } else {
            return Err(Error::ErrNonCertificate);
        };

        let params = PopulateSdpParams {
            media_description_fingerprint: self.setting_engine.sdp_media_level_fingerprints,
            is_icelite: self.setting_engine.candidates.ice_lite,
            extmap_allow_mixed: true,
            connection_role: DEFAULT_DTLS_ROLE_OFFER.to_connection_role(),
            ice_gathering_state: self.ice_gathering_state(),
            match_bundle_group: None,
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &self.media_engine,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    /// generate_matched_sdp generates a SDP and takes the remote state into account
    /// this is used everytime we have a remote_description
    pub(super) async fn generate_matched_sdp(
        &self,
        mut local_transceivers: Vec<Arc<RTCRtpTransceiver>>,
        use_identity: bool,
        include_unmatched: bool,
        connection_role: ConnectionRole,
    ) -> Result<SessionDescription> {
        let d = SessionDescription::new_jsep_session_description(use_identity);

        let ice_params = self.ice_gatherer.get_local_parameters().await?;
        let candidates = self.ice_gatherer.get_local_candidates().await?;

        let remote_description = self.remote_description().await;
        let mut media_sections = vec![];
        let mut already_have_application_media_section = false;
        let mut extmap_allow_mixed = false;

        if let Some(remote_description) = remote_description.as_ref() {
            if let Some(parsed) = &remote_description.parsed {
                extmap_allow_mixed = parsed.has_attribute(ATTR_KEY_EXTMAP_ALLOW_MIXED);

                for media in &parsed.media_descriptions {
                    if let Some(mid_value) = get_mid_value(media) {
                        if mid_value.is_empty() {
                            return Err(Error::ErrPeerConnRemoteDescriptionWithoutMidValue);
                        }

                        if media.media_name.media == MEDIA_SECTION_APPLICATION {
                            media_sections.push(MediaSection {
                                id: mid_value.to_owned(),
                                data: true,
                                ..Default::default()
                            });
                            already_have_application_media_section = true;
                            continue;
                        }

                        let kind = RTPCodecType::from(media.media_name.media.as_str());
                        let direction = get_peer_direction(media);
                        if kind == RTPCodecType::Unspecified
                            || direction == RTCRtpTransceiverDirection::Unspecified
                        {
                            continue;
                        }

                        let extmap_allow_mixed = media.has_attribute(ATTR_KEY_EXTMAP_ALLOW_MIXED);

                        if let Some(t) = find_by_mid(mid_value, &mut local_transceivers).await {
                            t.sender().await.set_negotiated();
                            let media_transceivers = vec![t];

                            // NB: The below could use `then_some`, but with our current MSRV
                            // it's not possible to actually do this. The clippy version that
                            // ships with 1.64.0 complains about this so we disable it for now.
                            #[allow(clippy::unnecessary_lazy_evaluations)]
                            media_sections.push(MediaSection {
                                id: mid_value.to_owned(),
                                transceivers: media_transceivers,
                                rid_map: get_rids(media),
                                offered_direction: (!include_unmatched).then(|| direction),
                                extmap_allow_mixed,
                                ..Default::default()
                            });
                        } else {
                            return Err(Error::ErrPeerConnTransceiverMidNil);
                        }
                    }
                }
            }
        }

        // If we are offering also include unmatched local transceivers
        let match_bundle_group = if include_unmatched {
            for t in &local_transceivers {
                t.sender().await.set_negotiated();
                media_sections.push(MediaSection {
                    id: t.mid().unwrap().to_string(),
                    transceivers: vec![Arc::clone(t)],
                    ..Default::default()
                });
            }

            if self
                .sctp_transport
                .data_channels_requested
                .load(Ordering::SeqCst)
                != 0
                && !already_have_application_media_section
            {
                media_sections.push(MediaSection {
                    id: format!("{}", media_sections.len()),
                    data: true,
                    ..Default::default()
                });
            }
            None
        } else {
            remote_description
                .as_ref()
                .and_then(|d| d.parsed.as_ref())
                .and_then(|d| d.attribute(ATTR_KEY_GROUP))
                .map(ToOwned::to_owned)
                .or(Some(String::new()))
        };

        let dtls_fingerprints = if let Some(cert) = self.dtls_transport.certificates.first() {
            cert.get_fingerprints()
        } else {
            return Err(Error::ErrNonCertificate);
        };

        let params = PopulateSdpParams {
            media_description_fingerprint: self.setting_engine.sdp_media_level_fingerprints,
            is_icelite: self.setting_engine.candidates.ice_lite,
            extmap_allow_mixed,
            connection_role,
            ice_gathering_state: self.ice_gathering_state(),
            match_bundle_group,
        };
        populate_sdp(
            d,
            &dtls_fingerprints,
            &self.media_engine,
            &candidates,
            &ice_params,
            &media_sections,
            params,
        )
        .await
    }

    pub(super) fn ice_gathering_state(&self) -> RTCIceGatheringState {
        match self.ice_gatherer.state() {
            RTCIceGathererState::New => RTCIceGatheringState::New,
            RTCIceGathererState::Gathering => RTCIceGatheringState::Gathering,
            _ => RTCIceGatheringState::Complete,
        }
    }

    async fn handle_undeclared_ssrc(
        self: &Arc<Self>,
        ssrc: SSRC,
        remote_description: &SessionDescription,
    ) -> Result<bool> {
        if remote_description.media_descriptions.len() != 1 {
            return Ok(false);
        }

        let only_media_section = &remote_description.media_descriptions[0];
        let mut stream_id = "";
        let mut id = "";
        let mut has_rid = false;
        let mut has_ssrc = false;

        for a in &only_media_section.attributes {
            match a.key.as_str() {
                ATTR_KEY_MSID => {
                    if let Some(value) = &a.value {
                        let split: Vec<&str> = value.split(' ').collect();
                        if split.len() == 2 {
                            stream_id = split[0];
                            id = split[1];
                        }
                    }
                }
                ATTR_KEY_SSRC => has_ssrc = true,
                SDP_ATTRIBUTE_RID => has_rid = true,
                _ => {}
            };
        }

        if has_rid {
            return Ok(false);
        } else if has_ssrc {
            return Err(Error::ErrPeerConnSingleMediaSectionHasExplicitSSRC);
        }

        let mut incoming = TrackDetails {
            ssrcs: vec![ssrc],
            kind: RTPCodecType::Video,
            stream_id: stream_id.to_owned(),
            id: id.to_owned(),
            ..Default::default()
        };
        if only_media_section.media_name.media == RTPCodecType::Audio.to_string() {
            incoming.kind = RTPCodecType::Audio;
        }

        let t = self
            .add_transceiver_from_kind(
                incoming.kind,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendrecv,
                    send_encodings: vec![],
                }),
            )
            .await?;

        let receiver = t.receiver().await;
        PeerConnectionInternal::start_receiver(
            self.setting_engine.get_receive_mtu(),
            &incoming,
            receiver,
            t,
            Arc::clone(&self.on_track_handler),
        )
        .await;
        Ok(true)
    }

    async fn handle_incoming_ssrc(
        self: &Arc<Self>,
        rtp_stream: Arc<Stream>,
        ssrc: SSRC,
    ) -> Result<()> {
        let parsed = match self.remote_description().await.and_then(|rd| rd.parsed) {
            Some(r) => r,
            None => return Err(Error::ErrPeerConnRemoteDescriptionNil),
        };
        // If the remote SDP was only one media section the ssrc doesn't have to be explicitly declared
        let handled = self.handle_undeclared_ssrc(ssrc, &parsed).await?;
        if handled {
            return Ok(());
        }

        // Get MID extension ID
        let (mid_extension_id, audio_supported, video_supported) = self
            .media_engine
            .get_header_extension_id(RTCRtpHeaderExtensionCapability {
                uri: ::sdp::extmap::SDES_MID_URI.to_owned(),
            })
            .await;
        if !audio_supported && !video_supported {
            return Err(Error::ErrPeerConnSimulcastMidRTPExtensionRequired);
        }

        // Get RID extension ID
        let (sid_extension_id, audio_supported, video_supported) = self
            .media_engine
            .get_header_extension_id(RTCRtpHeaderExtensionCapability {
                uri: ::sdp::extmap::SDES_RTP_STREAM_ID_URI.to_owned(),
            })
            .await;
        if !audio_supported && !video_supported {
            return Err(Error::ErrPeerConnSimulcastStreamIDRTPExtensionRequired);
        }

        let (rsid_extension_id, _, _) = self
            .media_engine
            .get_header_extension_id(RTCRtpHeaderExtensionCapability {
                uri: ::sdp::extmap::SDES_REPAIR_RTP_STREAM_ID_URI.to_owned(),
            })
            .await;

        // Packets that we read as part of simulcast probing that we need to make available
        // if we do find a track later.
        let mut buffered_packets: VecDeque<(rtp::packet::Packet, Attributes)> = VecDeque::default();

        let mut buf = vec![0u8; self.setting_engine.get_receive_mtu()];
        let n = rtp_stream.read(&mut buf).await?;
        let mut b = &buf[..n];

        let (mut mid, mut rid, mut rsid, payload_type) = handle_unknown_rtp_packet(
            b,
            mid_extension_id as u8,
            sid_extension_id as u8,
            rsid_extension_id as u8,
        )?;

        let packet = rtp::packet::Packet::unmarshal(&mut b).unwrap();

        // TODO: Can we have attributes on the first packets?
        buffered_packets.push_back((packet, Attributes::new()));

        let params = self
            .media_engine
            .get_rtp_parameters_by_payload_type(payload_type)
            .await?;

        let icpr = match self.interceptor.upgrade() {
            Some(i) => i,
            None => return Err(Error::ErrInterceptorNotBind),
        };

        let stream_info = create_stream_info(
            "".to_owned(),
            ssrc,
            params.codecs[0].payload_type,
            params.codecs[0].capability.clone(),
            &params.header_extensions,
            None,
        );
        let (rtp_read_stream, rtp_interceptor, rtcp_read_stream, rtcp_interceptor) = self
            .dtls_transport
            .streams_for_ssrc(ssrc, &stream_info, &icpr)
            .await?;

        let a = Attributes::new();
        for _ in 0..=SIMULCAST_PROBE_COUNT {
            if mid.is_empty() || (rid.is_empty() && rsid.is_empty()) {
                let (pkt, _) = rtp_interceptor.read(&mut buf, &a).await?;
                let (m, r, rs, _) = handle_unknown_rtp_packet(
                    &buf[..n],
                    mid_extension_id as u8,
                    sid_extension_id as u8,
                    rsid_extension_id as u8,
                )?;
                mid = m;
                rid = r;
                rsid = rs;

                buffered_packets.push_back((pkt, a.clone()));
                continue;
            }

            let transceivers = self.rtp_transceivers.lock().await;
            for t in &*transceivers {
                if t.mid().as_ref() != Some(&SmolStr::from(&mid)) {
                    continue;
                }

                let receiver = t.receiver().await;

                if !rsid.is_empty() {
                    return receiver
                        .receive_for_rtx(
                            0,
                            rsid,
                            TrackStream {
                                stream_info: Some(stream_info.clone()),
                                rtp_read_stream: Some(rtp_read_stream),
                                rtp_interceptor: Some(rtp_interceptor),
                                rtcp_read_stream: Some(rtcp_read_stream),
                                rtcp_interceptor: Some(rtcp_interceptor),
                            },
                        )
                        .await;
                }

                let track = receiver
                    .receive_for_rid(
                        SmolStr::from(rid),
                        params,
                        TrackStream {
                            stream_info: Some(stream_info.clone()),
                            rtp_read_stream: Some(rtp_read_stream),
                            rtp_interceptor: Some(rtp_interceptor),
                            rtcp_read_stream: Some(rtcp_read_stream),
                            rtcp_interceptor: Some(rtcp_interceptor),
                        },
                    )
                    .await?;
                track.prepopulate_peeked_data(buffered_packets).await;

                RTCPeerConnection::do_track(
                    Arc::clone(&self.on_track_handler),
                    track,
                    receiver,
                    Arc::clone(t),
                );
                return Ok(());
            }
        }

        let _ = rtp_read_stream.close().await;
        let _ = rtcp_read_stream.close().await;
        icpr.unbind_remote_stream(&stream_info).await;
        self.dtls_transport.remove_simulcast_stream(ssrc).await;

        Err(Error::ErrPeerConnSimulcastIncomingSSRCFailed)
    }

    async fn start_receiver(
        receive_mtu: usize,
        incoming: &TrackDetails,
        receiver: Arc<RTCRtpReceiver>,
        transceiver: Arc<RTCRtpTransceiver>,
        on_track_handler: Arc<ArcSwapOption<Mutex<OnTrackHdlrFn>>>,
    ) {
        receiver.start(incoming).await;
        for track in receiver.tracks().await {
            if track.ssrc() == 0 {
                return;
            }

            let receiver = Arc::clone(&receiver);
            let transceiver = Arc::clone(&transceiver);
            let on_track_handler = Arc::clone(&on_track_handler);
            tokio::spawn(async move {
                let mut b = vec![0u8; receive_mtu];
                let pkt = match track.peek(&mut b).await {
                    Ok((pkt, _)) => pkt,
                    Err(err) => {
                        log::warn!(
                            "Could not determine PayloadType for SSRC {} ({})",
                            track.ssrc(),
                            err
                        );
                        return;
                    }
                };

                if let Err(err) = track.check_and_update_track(&pkt).await {
                    log::warn!(
                        "Failed to set codec settings for track SSRC {} ({})",
                        track.ssrc(),
                        err
                    );
                    return;
                }

                RTCPeerConnection::do_track(on_track_handler, track, receiver, transceiver);
            });
        }
    }

    /// has_local_description_changed returns whether local media (rtp_transceivers) has changed
    /// caller of this method should hold `pc.mu` lock
    pub(super) async fn has_local_description_changed(&self, desc: &RTCSessionDescription) -> bool {
        let rtp_transceivers = self.rtp_transceivers.lock().await;
        for t in &*rtp_transceivers {
            let m = match t.mid().and_then(|mid| get_by_mid(mid.as_str(), desc)) {
                Some(m) => m,
                None => return true,
            };

            if get_peer_direction(m) != t.direction() {
                return true;
            }
        }
        false
    }

    pub(super) async fn get_stats(&self, stats_id: String) -> StatsCollector {
        let collector = StatsCollector::new();
        let transceivers = { self.rtp_transceivers.lock().await.clone() };

        tokio::join!(
            self.ice_gatherer.collect_stats(&collector),
            self.ice_transport.collect_stats(&collector),
            self.sctp_transport.collect_stats(&collector, stats_id),
            self.dtls_transport.collect_stats(&collector),
            self.media_engine.collect_stats(&collector),
            self.collect_inbound_stats(&collector, transceivers.clone()),
            self.collect_outbound_stats(&collector, transceivers)
        );

        collector
    }

    async fn collect_inbound_stats(
        &self,
        collector: &StatsCollector,
        transceivers: Vec<Arc<RTCRtpTransceiver>>,
    ) {
        // TODO: There's a lot of await points here that could run concurrently with `futures::join_all`.
        struct TrackInfo {
            ssrc: SSRC,
            mid: SmolStr,
            track_id: String,
            kind: &'static str,
        }
        let mut track_infos = vec![];
        for transeiver in transceivers {
            let receiver = transeiver.receiver().await;

            if let Some(mid) = transeiver.mid() {
                let tracks = receiver.tracks().await;

                for track in tracks {
                    let track_id = track.id();
                    let kind = match track.kind() {
                        RTPCodecType::Unspecified => continue,
                        RTPCodecType::Audio => "audio",
                        RTPCodecType::Video => "video",
                    };

                    track_infos.push(TrackInfo {
                        ssrc: track.ssrc(),
                        mid: mid.clone(),
                        track_id,
                        kind,
                    });
                }
            }
        }

        let stream_stats = self
            .stats_interceptor
            .fetch_inbound_stats(track_infos.iter().map(|t| t.ssrc).collect())
            .await;

        for (stats, info) in
            (stream_stats.into_iter().zip(track_infos)).filter_map(|(s, i)| s.map(|s| (s, i)))
        {
            let ssrc = info.ssrc;
            let kind = info.kind;

            let id = format!("RTCInboundRTP{}Stream_{}", capitalize(kind), ssrc);
            let (
                packets_received,
                header_bytes_received,
                bytes_received,
                last_packet_received_timestamp,
                nack_count,
                remote_packets_sent,
                remote_bytes_sent,
                remote_reports_sent,
                remote_round_trip_time,
                remote_total_round_trip_time,
                remote_round_trip_time_measurements,
            ) = (
                stats.packets_received(),
                stats.header_bytes_received(),
                stats.payload_bytes_received(),
                stats.last_packet_received_timestamp(),
                stats.nacks_sent(),
                stats.remote_packets_sent(),
                stats.remote_bytes_sent(),
                stats.remote_reports_sent(),
                stats.remote_round_trip_time(),
                stats.remote_total_round_trip_time(),
                stats.remote_round_trip_time_measurements(),
            );

            collector.insert(
                id.clone(),
                crate::stats::StatsReportType::InboundRTP(InboundRTPStats {
                    timestamp: Instant::now(),
                    stats_type: RTCStatsType::InboundRTP,
                    id: id.clone(),
                    ssrc,
                    kind: kind.to_owned(),
                    packets_received,
                    track_identifier: info.track_id,
                    mid: info.mid,
                    last_packet_received_timestamp,
                    header_bytes_received,
                    bytes_received,
                    nack_count,

                    fir_count: (info.kind == "video").then(|| stats.firs_sent()),
                    pli_count: (info.kind == "video").then(|| stats.plis_sent()),
                }),
            );

            let local_id = id;
            let id = format!(
                "RTCRemoteOutboundRTP{}Stream_{}",
                capitalize(info.kind),
                info.ssrc
            );
            collector.insert(
                id.clone(),
                crate::stats::StatsReportType::RemoteOutboundRTP(RemoteOutboundRTPStats {
                    timestamp: Instant::now(),
                    stats_type: RTCStatsType::RemoteOutboundRTP,
                    id,

                    ssrc,
                    kind: kind.to_owned(),

                    packets_sent: remote_packets_sent as u64,
                    bytes_sent: remote_bytes_sent as u64,
                    local_id,
                    reports_sent: remote_reports_sent,
                    round_trip_time: remote_round_trip_time,
                    total_round_trip_time: remote_total_round_trip_time,
                    round_trip_time_measurements: remote_round_trip_time_measurements,
                }),
            );
        }
    }

    async fn collect_outbound_stats(
        &self,
        collector: &StatsCollector,
        transceivers: Vec<Arc<RTCRtpTransceiver>>,
    ) {
        // TODO: There's a lot of await points here that could run concurrently with `futures::join_all`.
        struct TrackInfo {
            track_id: String,
            ssrc: SSRC,
            mid: SmolStr,
            rid: Option<SmolStr>,
            kind: &'static str,
        }
        let mut track_infos = vec![];
        for transceiver in transceivers {
            let mid = match transceiver.mid() {
                Some(mid) => mid,
                None => continue,
            };

            let sender = transceiver.sender().await;
            let track_encodings = sender.track_encodings.lock().await;
            for encoding in track_encodings.iter() {
                let track_id = encoding.track.id();
                let kind = match encoding.track.kind() {
                    RTPCodecType::Unspecified => continue,
                    RTPCodecType::Audio => "audio",
                    RTPCodecType::Video => "video",
                };

                track_infos.push(TrackInfo {
                    track_id: track_id.to_owned(),
                    ssrc: encoding.ssrc,
                    mid: mid.to_owned(),
                    rid: encoding.track.rid().map(Into::into),
                    kind,
                });

                if let Some(rtx) = &encoding.rtx {
                    track_infos.push(TrackInfo {
                        track_id: track_id.to_owned(),
                        ssrc: rtx.ssrc,
                        mid: mid.to_owned(),
                        rid: encoding.track.rid().map(Into::into),
                        kind,
                    });
                }
            }
        }

        let stream_stats = self
            .stats_interceptor
            .fetch_outbound_stats(track_infos.iter().map(|t| t.ssrc).collect())
            .await;

        for (stats, info) in stream_stats
            .into_iter()
            .zip(track_infos)
            .filter_map(|(s, i)| s.map(|s| (s, i)))
        {
            // RTCOutboundRtpStreamStats
            let id = format!(
                "RTCOutboundRTP{}Stream_{}",
                capitalize(info.kind),
                info.ssrc
            );
            let (
                packets_sent,
                bytes_sent,
                header_bytes_sent,
                nack_count,
                remote_inbound_packets_received,
                remote_inbound_packets_lost,
                remote_rtt_ms,
                remote_total_rtt_ms,
                remote_rtt_measurements,
                remote_fraction_lost,
            ) = (
                stats.packets_sent(),
                stats.payload_bytes_sent(),
                stats.header_bytes_sent(),
                stats.nacks_received(),
                stats.remote_packets_received(),
                stats.remote_total_lost(),
                stats.remote_round_trip_time(),
                stats.remote_total_round_trip_time(),
                stats.remote_round_trip_time_measurements(),
                stats.remote_fraction_lost(),
            );

            let TrackInfo {
                mid,
                ssrc,
                rid,
                kind,
                track_id: track_identifier,
            } = info;

            collector.insert(
                id.clone(),
                crate::stats::StatsReportType::OutboundRTP(OutboundRTPStats {
                    timestamp: Instant::now(),
                    stats_type: RTCStatsType::OutboundRTP,
                    track_identifier,
                    id: id.clone(),
                    ssrc,
                    kind: kind.to_owned(),
                    packets_sent,
                    mid,
                    rid,
                    header_bytes_sent,
                    bytes_sent,
                    nack_count,

                    fir_count: (info.kind == "video").then(|| stats.firs_received()),
                    pli_count: (info.kind == "video").then(|| stats.plis_received()),
                }),
            );

            let local_id = id;
            let id = format!(
                "RTCRemoteInboundRTP{}Stream_{}",
                capitalize(info.kind),
                info.ssrc
            );

            collector.insert(
                id.clone(),
                StatsReportType::RemoteInboundRTP(RemoteInboundRTPStats {
                    timestamp: Instant::now(),
                    stats_type: RTCStatsType::RemoteInboundRTP,
                    id,
                    ssrc,
                    kind: kind.to_owned(),

                    packets_received: remote_inbound_packets_received,
                    packets_lost: remote_inbound_packets_lost as i64,

                    local_id,

                    round_trip_time: remote_rtt_ms,
                    total_round_trip_time: remote_total_rtt_ms,
                    fraction_lost: remote_fraction_lost.unwrap_or(0.0),
                    round_trip_time_measurements: remote_rtt_measurements,
                }),
            );
        }
    }
}

type IResult<T> = std::result::Result<T, interceptor::Error>;

#[async_trait]
impl RTCPWriter for PeerConnectionInternal {
    async fn write(
        &self,
        pkts: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
        _a: &Attributes,
    ) -> IResult<usize> {
        Ok(self.dtls_transport.write_rtcp(pkts).await?)
    }
}

fn capitalize(s: &str) -> String {
    let first = s
        .chars()
        .next()
        .expect("Must have at least one character to uppercase")
        .to_uppercase();
    let mut result = String::new();

    result.extend(first);
    result.extend(s.chars().skip(1));

    result
}
