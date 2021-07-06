use super::*;
use crate::api::setting_engine::SettingEngine;
use crate::data::data_channel_state::DataChannelState;
use crate::error::{Error, OnErrorHdlrFn};

use sctp::stream::OnBufferedAmountLowFn;

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;

/// message size limit for Chromium
const DATA_CHANNEL_BUFFER_SIZE: u16 = u16::MAX;

pub type OnMessageHdlrFn = Box<
    dyn (FnMut(DataChannelMessage) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub type OnOpenHdlrFn =
    Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

pub type OnCloseHdlrFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

/// DataChannel represents a WebRTC DataChannel
/// The DataChannel interface represents a network channel
/// which can be used for bidirectional peer-to-peer transfers of arbitrary data
#[derive(Default, Clone)]
pub struct DataChannel {
    stats_id: String,
    label: String,
    ordered: bool,
    max_packet_lifetime: u16,
    max_retransmits: u16,
    protocol: String,
    negotiated: bool,
    id: u16,
    ready_state: Arc<AtomicU8>, // DataChannelState
    //buffered_amount_low_threshold: u64,
    detach_called: Arc<AtomicBool>,

    // The binaryType represents attribute MUST, on getting, return the value to
    // which it was last set. On setting, if the new value is either the string
    // "blob" or the string "arraybuffer", then set the IDL attribute to this
    // new value. Otherwise, throw a SyntaxError. When an DataChannel object
    // is created, the binaryType attribute MUST be initialized to the string
    // "blob". This attribute controls how binary data is exposed to scripts.
    // binaryType                 string
    on_message_handler: Arc<Mutex<Option<OnMessageHdlrFn>>>,
    on_open_handler: Arc<Mutex<Option<OnOpenHdlrFn>>>,
    on_close_handler: Arc<Mutex<Option<OnCloseHdlrFn>>>,
    on_error_handler: Arc<Mutex<Option<OnErrorHdlrFn>>>,

    //TODO:       sctpTransport *SCTPTransport
    data_channel: Option<Arc<data::data_channel::DataChannel>>,

    // A reference to the associated api object used by this datachannel
    setting_engine: SettingEngine,
}

impl DataChannel {
    // create the DataChannel object before the networking is set up.
    pub fn new(params: DataChannelParameters, setting_engine: SettingEngine) -> Self {
        DataChannel {
            stats_id: format!(
                "DataChannel-{}",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ),
            label: params.label,
            protocol: params.protocol,
            negotiated: params.negotiated,
            id: params.id,
            ordered: params.ordered,
            max_packet_lifetime: params.max_packet_lifetime,
            max_retransmits: params.max_retransmits,
            ready_state: Arc::new(AtomicU8::new(DataChannelState::Connecting as u8)),
            detach_called: Arc::new(AtomicBool::new(false)),
            setting_engine,
            ..Default::default()
        }
    }

    /*
    /// open opens the datachannel over the sctp transport
    pub(crate) fn open(sctpTransport: SCTPTransport) ->Result<()> {
        association := sctpTransport.association()
        if association == nil {
            return errSCTPNotEstablished
        }

        d.mu.Lock()
        if d.sctpTransport != nil { // already open
            d.mu.Unlock()
            return nil
        }
        d.sctpTransport = sctpTransport
        var channelType datachannel.ChannelType
        var reliabilityParameter uint32

        switch {
        case d.max_packet_life_time == nil && d.max_retransmits == nil:
            if d.ordered {
                channelType = datachannel.ChannelTypeReliable
            } else {
                channelType = datachannel.ChannelTypeReliableUnordered
            }

        case d.max_retransmits != nil:
            reliabilityParameter = uint32(*d.max_retransmits)
            if d.ordered {
                channelType = datachannel.ChannelTypePartialReliableRexmit
            } else {
                channelType = datachannel.ChannelTypePartialReliableRexmitUnordered
            }
        default:
            reliabilityParameter = uint32(*d.max_packet_life_time)
            if d.ordered {
                channelType = datachannel.ChannelTypePartialReliableTimed
            } else {
                channelType = datachannel.ChannelTypePartialReliableTimedUnordered
            }
        }

        cfg := &datachannel.Config{
            ChannelType:          channelType,
            Priority:             datachannel.ChannelPriorityNormal,
            ReliabilityParameter: reliabilityParameter,
            Label:                d.label,
            Protocol:             d.protocol,
            Negotiated:           d.negotiated,
            LoggerFactory:        d.api.settingEngine.LoggerFactory,
        }

        if d.id == nil {
            err := d.sctpTransport.generateAndSetDataChannelID(d.sctpTransport.dtlsTransport.role(), &d.id)
            if err != nil {
                return err
            }
        }
        dc, err := datachannel.Dial(association, *d.id, cfg)
        if err != nil {
            d.mu.Unlock()
            return err
        }

        // buffered_amount_low_threshold and on_buffered_amount_low might be set earlier
        dc.set_buffered_amount_low_threshold(d.buffered_amount_low_threshold)
        dc.on_buffered_amount_low(d.on_buffered_amount_low)
        d.mu.Unlock()

        d.handleOpen(dc)
        return nil
    }

    // Transport returns the SCTPTransport instance the DataChannel is sending over.
    func (d *DataChannel) Transport() *SCTPTransport {
        d.mu.RLock()
        defer d.mu.RUnlock()

        return d.sctpTransport
    }*/

    /// After onOpen is complete check that the user called detach
    /// and provide an error message if the call was missed
    fn check_detach_after_open(&self) {
        if self.setting_engine.detach.data_channels && !self.detach_called.load(Ordering::SeqCst) {
            log::warn!(
                "webrtc.DetachDataChannels() enabled but didn't Detach, call Detach from OnOpen"
            );
        }
    }

    /// on_open sets an event handler which is invoked when
    /// the underlying data transport has been established (or re-established).
    pub async fn on_open(&self, f: OnOpenHdlrFn) {
        if self.ready_state() == DataChannelState::Open {
            f().await;
            self.check_detach_after_open();
        } else {
            let mut handler = self.on_open_handler.lock().await;
            *handler = Some(f);
        }
    }

    /// on_close sets an event handler which is invoked when
    /// the underlying data transport has been closed.
    pub async fn on_close(&self, f: OnCloseHdlrFn) {
        let mut handler = self.on_close_handler.lock().await;
        *handler = Some(f);
    }

    /// on_message sets an event handler which is invoked on a binary
    /// message arrival over the sctp transport from a remote peer.
    /// OnMessage can currently receive messages up to 16384 bytes
    /// in size. Check out the detach API if you want to use larger
    /// message sizes. Note that browser support for larger messages
    /// is also limited.
    pub async fn on_message(&self, f: OnMessageHdlrFn) {
        let mut handler = self.on_message_handler.lock().await;
        *handler = Some(f);
    }

    pub(crate) async fn handle_open(&self) {
        self.set_ready_state(DataChannelState::Open);

        {
            let mut handler = self.on_open_handler.lock().await;
            if let Some(f) = handler.take() {
                f().await;
                self.check_detach_after_open();
            }
        }

        if let Some(dc) = &self.data_channel {
            if !self.setting_engine.detach.data_channels {
                let data_channel = Arc::clone(dc);
                let ready_state = Arc::clone(&self.ready_state);
                let on_message_handler = Arc::clone(&self.on_message_handler);
                let on_close_handler = Arc::clone(&self.on_close_handler);
                let on_error_handler = Arc::clone(&self.on_error_handler);
                tokio::spawn(async move {
                    DataChannel::read_loop(
                        data_channel,
                        ready_state,
                        on_message_handler,
                        on_close_handler,
                        on_error_handler,
                    )
                    .await;
                });
            }
        }
    }

    /// on_error sets an event handler which is invoked when
    /// the underlying data transport cannot be read.
    pub async fn on_error(&self, f: OnErrorHdlrFn) {
        let mut handler = self.on_error_handler.lock().await;
        *handler = Some(f);
    }

    async fn read_loop(
        data_channel: Arc<data::data_channel::DataChannel>,
        ready_state: Arc<AtomicU8>,
        on_message_handler: Arc<Mutex<Option<OnMessageHdlrFn>>>,
        on_close_handler: Arc<Mutex<Option<OnCloseHdlrFn>>>,
        on_error_handler: Arc<Mutex<Option<OnErrorHdlrFn>>>,
    ) {
        let mut buffer = vec![0u8; DATA_CHANNEL_BUFFER_SIZE as usize];
        loop {
            let (n, is_string) = match data_channel.read_data_channel(&mut buffer).await {
                Ok((n, is_string)) => (n, is_string),
                Err(err) => {
                    ready_state.store(DataChannelState::Closed as u8, Ordering::SeqCst);
                    if !sctp::error::Error::ErrStreamClosed.equal(&err) {
                        let mut handler = on_error_handler.lock().await;
                        if let Some(f) = &mut *handler {
                            f(err).await;
                        }
                    }

                    {
                        let mut handler = on_close_handler.lock().await;
                        if let Some(f) = &mut *handler {
                            f().await;
                        }
                    }

                    break;
                }
            };

            {
                let mut handler = on_message_handler.lock().await;
                if let Some(f) = &mut *handler {
                    f(DataChannelMessage {
                        is_string,
                        data: Bytes::from(buffer[..n].to_vec()),
                    })
                    .await;
                }
            }
        }
    }

    /// send sends the binary message to the DataChannel peer
    pub async fn send(&self, data: &Bytes) -> Result<usize> {
        self.ensure_open()?;
        if let Some(dc) = &self.data_channel {
            dc.write_data_channel(data, false).await
        } else {
            Err(Error::ErrClosedPipe.into())
        }
    }

    /// send_text sends the text message to the DataChannel peer
    pub async fn send_text(&self, s: String) -> Result<usize> {
        self.ensure_open()?;

        if let Some(dc) = &self.data_channel {
            dc.write_data_channel(&Bytes::from(s), true).await
        } else {
            Err(Error::ErrClosedPipe.into())
        }
    }

    fn ensure_open(&self) -> Result<()> {
        if self.ready_state() != DataChannelState::Open {
            Err(Error::ErrClosedPipe.into())
        } else {
            Ok(())
        }
    }

    /// detach allows you to detach the underlying datachannel. This provides
    /// an idiomatic API to work with, however it disables the OnMessage callback.
    /// Before calling Detach you have to enable this behavior by calling
    /// webrtc.DetachDataChannels(). Combining detached and normal data channels
    /// is not supported.
    /// Please refer to the data-channels-detach example and the
    /// pion/datachannel documentation for the correct way to handle the
    /// resulting DataChannel object.
    pub fn detach(&self) -> Result<Arc<data::data_channel::DataChannel>> {
        if !self.setting_engine.detach.data_channels {
            return Err(Error::ErrDetachNotEnabled.into());
        }

        if let Some(dc) = &self.data_channel {
            self.detach_called.store(true, Ordering::SeqCst);

            Ok(Arc::clone(dc))
        } else {
            Err(Error::ErrDetachBeforeOpened.into())
        }
    }

    /// Close Closes the DataChannel. It may be called regardless of whether
    /// the DataChannel object was created by this peer or the remote peer.
    pub async fn close(&self) -> Result<()> {
        if self.ready_state() == DataChannelState::Closed {
            return Ok(());
        }

        self.set_ready_state(DataChannelState::Closing);

        if let Some(dc) = &self.data_channel {
            dc.close().await
        } else {
            Ok(())
        }
    }

    /// label represents a label that can be used to distinguish this
    /// DataChannel object from other DataChannel objects. Scripts are
    /// allowed to create multiple DataChannel objects with the same label.
    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    /// Ordered represents if the DataChannel is ordered, and false if
    /// out-of-order delivery is allowed.
    pub fn ordered(&self) -> bool {
        self.ordered
    }

    /// max_packet_lifetime represents the length of the time window (msec) during
    /// which transmissions and retransmissions may occur in unreliable mode.
    pub fn max_packet_lifetime(&self) -> u16 {
        self.max_packet_lifetime
    }

    /// max_retransmits represents the maximum number of retransmissions that are
    /// attempted in unreliable mode.
    pub fn max_retransmits(&self) -> u16 {
        self.max_retransmits
    }

    /// protocol represents the name of the sub-protocol used with this
    /// DataChannel.
    pub fn protocol(&self) -> &str {
        self.protocol.as_str()
    }

    /// negotiated represents whether this DataChannel was negotiated by the
    /// application (true), or not (false).
    pub fn negotiated(&self) -> bool {
        self.negotiated
    }

    /// ID represents the ID for this DataChannel. The value is initially
    /// null, which is what will be returned if the ID was not provided at
    /// channel creation time, and the DTLS role of the SCTP transport has not
    /// yet been negotiated. Otherwise, it will return the ID that was either
    /// selected by the script or generated. After the ID is set to a non-null
    /// value, it will not change.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// ready_state represents the state of the DataChannel object.
    pub fn ready_state(&self) -> DataChannelState {
        self.ready_state.load(Ordering::SeqCst).into()
    }

    /// buffered_amount represents the number of bytes of application data
    /// (UTF-8 text and binary data) that have been queued using send(). Even
    /// though the data transmission can occur in parallel, the returned value
    /// MUST NOT be decreased before the current task yielded back to the event
    /// loop to prevent race conditions. The value does not include framing
    /// overhead incurred by the protocol, or buffering done by the operating
    /// system or network hardware. The value of buffered_amount slot will only
    /// increase with each call to the send() method as long as the ready_state is
    /// open; however, buffered_amount does not reset to zero once the channel
    /// closes.
    pub fn buffered_amount(&self) -> usize {
        if let Some(dc) = &self.data_channel {
            dc.buffered_amount()
        } else {
            0
        }
    }

    /// buffered_amount_low_threshold represents the threshold at which the
    /// bufferedAmount is considered to be low. When the bufferedAmount decreases
    /// from above this threshold to equal or below it, the bufferedamountlow
    /// event fires. buffered_amount_low_threshold is initially zero on each new
    /// DataChannel, but the application may change its value at any time.
    /// The threshold is set to 0 by default.
    pub fn buffered_amount_low_threshold(&self) -> usize {
        if let Some(dc) = &self.data_channel {
            dc.buffered_amount_low_threshold()
        } else {
            //self.buffered_amount_low_threshold
            0
        }
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub fn set_buffered_amount_low_threshold(&self, th: usize) {
        //self.buffered_amount_low_threshold = th;
        if let Some(dc) = &self.data_channel {
            dc.set_buffered_amount_low_threshold(th);
        }
    }

    /// on_buffered_amount_low sets an event handler which is invoked when
    /// the number of bytes of outgoing data becomes lower than the
    /// buffered_amount_low_threshold.
    pub async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        //self.onBufferedAmountLow = f
        if let Some(dc) = &self.data_channel {
            dc.on_buffered_amount_low(f).await;
        }
    }

    pub(crate) fn get_stats_id(&self) -> &str {
        self.stats_id.as_str()
    }

    /*TODO:
    func (d *DataChannel) collectStats(collector *statsReportCollector) {
        collector.Collecting()

        d.mu.Lock()
        defer d.mu.Unlock()

        stats := DataChannelStats{
            Timestamp: statsTimestampNow(),
            Type:      StatsTypeDataChannel,
            ID:        d.stats_id,
            Label:     d.label,
            Protocol:  d.protocol,
            // TransportID string `json:"transportId"`
            State: d.ready_state(),
        }

        if d.id != nil {
            stats.DataChannelIdentifier = int32(*d.id)
        }

        if d.dataChannel != nil {
            stats.MessagesSent = d.dataChannel.MessagesSent()
            stats.BytesSent = d.dataChannel.BytesSent()
            stats.MessagesReceived = d.dataChannel.MessagesReceived()
            stats.BytesReceived = d.dataChannel.BytesReceived()
        }

        collector.Collect(stats.ID, stats)
    }

    */

    pub(crate) fn set_ready_state(&self, r: DataChannelState) {
        self.ready_state.store(r as u8, Ordering::SeqCst);
    }
}
