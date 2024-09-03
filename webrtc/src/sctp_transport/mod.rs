#[cfg(test)]
mod sctp_transport_test;

pub mod sctp_transport_capabilities;
pub mod sctp_transport_state;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use data::data_channel::DataChannel;
use data::message::message_channel_open::ChannelType;
use portable_atomic::{AtomicBool, AtomicU32, AtomicU8};
use sctp::association::Association;
use sctp_transport_state::RTCSctpTransportState;
use tokio::sync::{Mutex, Notify};
use util::Conn;

use crate::api::setting_engine::SettingEngine;
use crate::data_channel::data_channel_parameters::DataChannelParameters;
use crate::data_channel::data_channel_state::RTCDataChannelState;
use crate::data_channel::RTCDataChannel;
use crate::dtls_transport::dtls_role::DTLSRole;
use crate::dtls_transport::*;
use crate::error::*;
use crate::sctp_transport::sctp_transport_capabilities::SCTPTransportCapabilities;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::StatsReportType::{PeerConnection, SCTPTransport};
use crate::stats::{ICETransportStats, PeerConnectionStats};

const SCTP_MAX_CHANNELS: u16 = u16::MAX;

pub type OnDataChannelHdlrFn = Box<
    dyn (FnMut(Arc<RTCDataChannel>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnDataChannelOpenedHdlrFn = Box<
    dyn (FnMut(Arc<RTCDataChannel>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

struct AcceptDataChannelParams {
    notify_rx: Arc<Notify>,
    sctp_association: Arc<Association>,
    data_channels: Arc<Mutex<Vec<Arc<RTCDataChannel>>>>,
    on_error_handler: Arc<ArcSwapOption<Mutex<OnErrorHdlrFn>>>,
    on_data_channel_handler: Arc<ArcSwapOption<Mutex<OnDataChannelHdlrFn>>>,
    on_data_channel_opened_handler: Arc<ArcSwapOption<Mutex<OnDataChannelOpenedHdlrFn>>>,
    data_channels_opened: Arc<AtomicU32>,
    data_channels_accepted: Arc<AtomicU32>,
    setting_engine: Arc<SettingEngine>,
}

/// SCTPTransport provides details about the SCTP transport.
#[derive(Default)]
pub struct RTCSctpTransport {
    pub(crate) dtls_transport: Arc<RTCDtlsTransport>,

    // State represents the current state of the SCTP transport.
    state: AtomicU8, // RTCSctpTransportState

    // SCTPTransportState doesn't have an enum to distinguish between New/Connecting
    // so we need a dedicated field
    is_started: AtomicBool,

    // max_message_size represents the maximum size of data that can be passed to
    // DataChannel's send() method.
    max_message_size: usize,

    // max_channels represents the maximum amount of DataChannel's that can
    // be used simultaneously.
    max_channels: u16,

    sctp_association: Mutex<Option<Arc<Association>>>,

    on_error_handler: Arc<ArcSwapOption<Mutex<OnErrorHdlrFn>>>,
    on_data_channel_handler: Arc<ArcSwapOption<Mutex<OnDataChannelHdlrFn>>>,
    on_data_channel_opened_handler: Arc<ArcSwapOption<Mutex<OnDataChannelOpenedHdlrFn>>>,

    // DataChannels
    pub(crate) data_channels: Arc<Mutex<Vec<Arc<RTCDataChannel>>>>,
    pub(crate) data_channels_opened: Arc<AtomicU32>,
    pub(crate) data_channels_requested: Arc<AtomicU32>,
    data_channels_accepted: Arc<AtomicU32>,

    notify_tx: Arc<Notify>,

    setting_engine: Arc<SettingEngine>,
}

impl RTCSctpTransport {
    pub(crate) fn new(
        dtls_transport: Arc<RTCDtlsTransport>,
        setting_engine: Arc<SettingEngine>,
    ) -> Self {
        RTCSctpTransport {
            dtls_transport,
            state: AtomicU8::new(RTCSctpTransportState::Connecting as u8),
            is_started: AtomicBool::new(false),
            max_message_size: RTCSctpTransport::calc_message_size(65536, 65536),
            max_channels: SCTP_MAX_CHANNELS,
            sctp_association: Mutex::new(None),
            on_error_handler: Arc::new(ArcSwapOption::empty()),
            on_data_channel_handler: Arc::new(ArcSwapOption::empty()),
            on_data_channel_opened_handler: Arc::new(ArcSwapOption::empty()),

            data_channels: Arc::new(Mutex::new(vec![])),
            data_channels_opened: Arc::new(AtomicU32::new(0)),
            data_channels_requested: Arc::new(AtomicU32::new(0)),
            data_channels_accepted: Arc::new(AtomicU32::new(0)),

            notify_tx: Arc::new(Notify::new()),

            setting_engine,
        }
    }

    /// transport returns the DTLSTransport instance the SCTPTransport is sending over.
    pub fn transport(&self) -> Arc<RTCDtlsTransport> {
        Arc::clone(&self.dtls_transport)
    }

    /// get_capabilities returns the SCTPCapabilities of the SCTPTransport.
    pub fn get_capabilities(&self) -> SCTPTransportCapabilities {
        SCTPTransportCapabilities {
            max_message_size: 0,
        }
    }

    /// Start the SCTPTransport. Since both local and remote parties must mutually
    /// create an SCTPTransport, SCTP SO (Simultaneous Open) is used to establish
    /// a connection over SCTP.
    pub async fn start(&self, _remote_caps: SCTPTransportCapabilities) -> Result<()> {
        if self.is_started.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.is_started.store(true, Ordering::SeqCst);

        let dtls_transport = self.transport();
        if let Some(net_conn) = &dtls_transport.conn().await {
            let sctp_association = loop {
                tokio::select! {
                    _ = self.notify_tx.notified() => {
                        // It seems like notify_tx is only notified on Stop so perhaps this check
                        // is redundant.
                        // TODO: Consider renaming notify_tx to shutdown_tx.
                        if self.state.load(Ordering::SeqCst) == RTCSctpTransportState::Closed as u8 {
                            return Err(Error::ErrSCTPTransportDTLS);
                        }
                    },
                    association = sctp::association::Association::client(sctp::association::Config {
                        net_conn: Arc::clone(net_conn) as Arc<dyn Conn + Send + Sync>,
                        max_receive_buffer_size: 0,
                        max_message_size: 0,
                        name: String::new(),
                    }) => {
                        break Arc::new(association?);
                    }
                };
            };

            {
                let mut sa = self.sctp_association.lock().await;
                *sa = Some(Arc::clone(&sctp_association));
            }
            self.state
                .store(RTCSctpTransportState::Connected as u8, Ordering::SeqCst);

            let param = AcceptDataChannelParams {
                notify_rx: self.notify_tx.clone(),
                sctp_association,
                data_channels: Arc::clone(&self.data_channels),
                on_error_handler: Arc::clone(&self.on_error_handler),
                on_data_channel_handler: Arc::clone(&self.on_data_channel_handler),
                on_data_channel_opened_handler: Arc::clone(&self.on_data_channel_opened_handler),
                data_channels_opened: Arc::clone(&self.data_channels_opened),
                data_channels_accepted: Arc::clone(&self.data_channels_accepted),
                setting_engine: Arc::clone(&self.setting_engine),
            };
            tokio::spawn(async move {
                RTCSctpTransport::accept_data_channels(param).await;
            });

            Ok(())
        } else {
            Err(Error::ErrSCTPTransportDTLS)
        }
    }

    /// Stop stops the SCTPTransport
    pub async fn stop(&self) -> Result<()> {
        {
            let mut sctp_association = self.sctp_association.lock().await;
            if let Some(sa) = sctp_association.take() {
                sa.close().await?;
            }
        }

        self.state
            .store(RTCSctpTransportState::Closed as u8, Ordering::SeqCst);

        self.notify_tx.notify_waiters();

        Ok(())
    }

    async fn accept_data_channels(param: AcceptDataChannelParams) {
        let dcs = param.data_channels.lock().await;
        let mut existing_data_channels = Vec::new();
        for dc in dcs.iter() {
            if let Some(dc) = dc.data_channel.lock().await.clone() {
                existing_data_channels.push(dc);
            }
        }
        drop(dcs);

        loop {
            let dc = tokio::select! {
                _ = param.notify_rx.notified() => break,
                result = DataChannel::accept(
                    &param.sctp_association,
                    data::data_channel::Config::default(),
                    &existing_data_channels,
                ) => {
                    match result {
                        Ok(dc) => dc,
                        Err(err) => {
                            if data::Error::ErrStreamClosed == err {
                                log::error!("Failed to accept data channel: {}", err);
                                if let Some(handler) = &*param.on_error_handler.load() {
                                    let mut f = handler.lock().await;
                                    f(err.into()).await;
                                }
                            }
                            break;
                        }
                    }
                }
            };

            let mut max_retransmits = None;
            let mut max_packet_life_time = None;
            let val = dc.config.reliability_parameter as u16;
            let ordered;

            match dc.config.channel_type {
                ChannelType::Reliable => {
                    ordered = true;
                }
                ChannelType::ReliableUnordered => {
                    ordered = false;
                }
                ChannelType::PartialReliableRexmit => {
                    ordered = true;
                    max_retransmits = Some(val);
                }
                ChannelType::PartialReliableRexmitUnordered => {
                    ordered = false;
                    max_retransmits = Some(val);
                }
                ChannelType::PartialReliableTimed => {
                    ordered = true;
                    max_packet_life_time = Some(val);
                }
                ChannelType::PartialReliableTimedUnordered => {
                    ordered = false;
                    max_packet_life_time = Some(val);
                }
            };

            let negotiated = if dc.config.negotiated {
                Some(dc.stream_identifier())
            } else {
                None
            };
            let rtc_dc = Arc::new(RTCDataChannel::new(
                DataChannelParameters {
                    label: dc.config.label.clone(),
                    protocol: dc.config.protocol.clone(),
                    negotiated,
                    ordered,
                    max_packet_life_time,
                    max_retransmits,
                },
                Arc::clone(&param.setting_engine),
            ));

            if let Some(handler) = &*param.on_data_channel_handler.load() {
                let mut f = handler.lock().await;
                f(Arc::clone(&rtc_dc)).await;

                param.data_channels_accepted.fetch_add(1, Ordering::SeqCst);

                let mut dcs = param.data_channels.lock().await;
                dcs.push(Arc::clone(&rtc_dc));
            }

            rtc_dc.handle_open(Arc::new(dc)).await;

            if let Some(handler) = &*param.on_data_channel_opened_handler.load() {
                let mut f = handler.lock().await;
                f(rtc_dc).await;
                param.data_channels_opened.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// on_error sets an event handler which is invoked when
    /// the SCTP connection error occurs.
    pub fn on_error(&self, f: OnErrorHdlrFn) {
        self.on_error_handler.store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_data_channel sets an event handler which is invoked when a data
    /// channel message arrives from a remote peer.
    pub fn on_data_channel(&self, f: OnDataChannelHdlrFn) {
        self.on_data_channel_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    /// on_data_channel_opened sets an event handler which is invoked when a data
    /// channel is opened
    pub fn on_data_channel_opened(&self, f: OnDataChannelOpenedHdlrFn) {
        self.on_data_channel_opened_handler
            .store(Some(Arc::new(Mutex::new(f))));
    }

    fn calc_message_size(remote_max_message_size: usize, can_send_size: usize) -> usize {
        if remote_max_message_size == 0 && can_send_size == 0 {
            usize::MAX
        } else if remote_max_message_size == 0 {
            can_send_size
        } else if can_send_size == 0 || can_send_size > remote_max_message_size {
            remote_max_message_size
        } else {
            can_send_size
        }
    }

    /// max_channels is the maximum number of RTCDataChannels that can be open simultaneously.
    pub fn max_channels(&self) -> u16 {
        if self.max_channels == 0 {
            SCTP_MAX_CHANNELS
        } else {
            self.max_channels
        }
    }

    /// state returns the current state of the SCTPTransport
    pub fn state(&self) -> RTCSctpTransportState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub(crate) async fn collect_stats(
        &self,
        collector: &StatsCollector,
        peer_connection_id: String,
    ) {
        let dtls_transport = self.transport();

        // TODO: should this be collected?
        dtls_transport.collect_stats(collector).await;

        // data channels
        let mut data_channels_closed = 0;
        let data_channels = self.data_channels.lock().await;
        for data_channel in &*data_channels {
            match data_channel.ready_state() {
                RTCDataChannelState::Connecting => (),
                RTCDataChannelState::Open => (),
                _ => data_channels_closed += 1,
            }
            data_channel.collect_stats(collector).await;
        }

        let mut reports = HashMap::new();
        let peer_connection_stats =
            PeerConnectionStats::new(self, peer_connection_id.clone(), data_channels_closed);
        reports.insert(peer_connection_id, PeerConnection(peer_connection_stats));

        // conn
        if let Some(agent) = dtls_transport.ice_transport.gatherer.get_agent().await {
            let stats = ICETransportStats::new("sctp_transport".to_owned(), agent);
            reports.insert(stats.id.clone(), SCTPTransport(stats));
        }

        collector.merge(reports);
    }

    pub(crate) async fn generate_and_set_data_channel_id(
        &self,
        dtls_role: DTLSRole,
    ) -> Result<u16> {
        let mut id = 0u16;
        if dtls_role != DTLSRole::Client {
            id += 1;
        }

        // Create map of ids so we can compare without double-looping each time.
        let mut ids_map = HashSet::new();
        {
            let data_channels = self.data_channels.lock().await;
            for dc in &*data_channels {
                ids_map.insert(dc.id());
            }
        }

        let max = self.max_channels();
        while id < max - 1 {
            if ids_map.contains(&id) {
                id += 2;
            } else {
                return Ok(id);
            }
        }

        Err(Error::ErrMaxDataChannelID)
    }

    pub(crate) async fn association(&self) -> Option<Arc<Association>> {
        let sctp_association = self.sctp_association.lock().await;
        sctp_association.clone()
    }

    pub(crate) fn data_channels_accepted(&self) -> u32 {
        self.data_channels_accepted.load(Ordering::SeqCst)
    }

    pub(crate) fn data_channels_opened(&self) -> u32 {
        self.data_channels_opened.load(Ordering::SeqCst)
    }

    pub(crate) fn data_channels_requested(&self) -> u32 {
        self.data_channels_requested.load(Ordering::SeqCst)
    }
}
