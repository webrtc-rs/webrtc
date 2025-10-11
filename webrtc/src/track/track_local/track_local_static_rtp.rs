use bytes::{Bytes, BytesMut};
use rtp::packet::Packet;
use std::any::Any;
use std::sync::atomic::AtomicU64;
use std::{borrow::Cow, collections::HashMap, time::Duration};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, Mutex};
use util::{Marshal, MarshalSize};

use super::*;
use crate::track::track_remote::TrackRemote;
use crate::{error::flatten_errs, track::track_local::packet_cache::PCacheBuffer};

#[derive(Debug)]
pub struct TrackState {
    last_out_seq: u16, // переживает все переключения источников
    last_out_ts: u32,  // переживает все переключения источников
    started_at_ts: i64,

    out_offset: Option<(
        u16, /* смещение порядкового номера */
        u32, /* смещение временной метки timestamp */
    )>,
}

impl TrackState {
    pub fn new() -> Self {
        TrackState {
            // Порядковый номер начинается с 0
            last_out_seq: 0,
            // время трека начинается с 0
            last_out_ts: 0,
            // Сохраняем начало трека в реальной временной шкале дла последующей синхронизации
            started_at_ts: chrono::Utc::now().timestamp(),
            out_offset: None,
        }
    }

    pub fn apply_offset(
        &mut self,
        kind: RTPCodecType,
        pkt_sequence_number: u16,
        pkt_timestamp: u32,
    ) -> (u16, u32) {
        match self.out_offset {
            Some((seq_num_offset, ts_offset)) => {
                self.last_out_seq = pkt_sequence_number.wrapping_add(seq_num_offset);
                self.last_out_ts = pkt_timestamp.wrapping_add(ts_offset);
                (self.last_out_seq, self.last_out_ts)
            }
            None => {
                let seq_num_offset = self
                    .last_out_seq
                    .wrapping_sub(pkt_sequence_number)
                    .wrapping_add(1);
                let ts_offset =
                    self.last_out_ts
                        .wrapping_sub(pkt_timestamp)
                        .wrapping_add(match kind {
                            RTPCodecType::Audio => 900,  // стандартное значение для звука
                            RTPCodecType::Video => 3750, // 90000 clock_rate / 24 кадра
                            _ => 3750,
                        });
                self.out_offset = Some((seq_num_offset, ts_offset));

                self.last_out_seq = pkt_sequence_number.wrapping_add(seq_num_offset);
                self.last_out_ts = pkt_timestamp.wrapping_add(ts_offset);

                println!(
                    "Смещения перезаписаны seq_num: {pkt_sequence_number} -> {}; ts: {pkt_timestamp} -> {}",
                    self.last_out_seq, self.last_out_ts
                );
                (self.last_out_seq, self.last_out_ts)
            }
        }
    }

    pub fn origin_seq(&self, modified_seq: u16) -> u16 {
        match self.out_offset {
            Some((seq_num_offset, _)) => modified_seq.wrapping_sub(seq_num_offset),
            None => modified_seq,
        }
    }
}

/// TrackLocalStaticRTP  is a TrackLocal that has a pre-set codec and accepts RTP Packets.
/// If you wish to send a media.Sample use TrackLocalStaticSample
#[derive(Debug)]
pub struct TrackLocalStaticRTP {
    pub(crate) bindings: Mutex<Vec<Arc<TrackBinding>>>,
    codec: RTCRtpCodecCapability,
    id: String,
    rid: Option<String>,
    stream_id: String,

    pub state: Mutex<TrackState>,
    pub rtp_cache: Arc<PCacheBuffer>,

    pli_last_ms: AtomicU64,
    pli_interval_ms: u64,
}

/// Количество пакетов в кэше
const CAPACITY: usize = 256; // если 24 пакета в секунду, то на 3 секунды нужно 72 ячейки кэша

/// TTL в миллисекундах, время через которое кэш становится невалидным
const TTL_MILLIS: u64 = 3000;

impl TrackLocalStaticRTP {
    /// returns a TrackLocalStaticRTP without rid.
    pub fn new(codec: RTCRtpCodecCapability, id: String, stream_id: String) -> Self {
        TrackLocalStaticRTP {
            codec,
            bindings: Mutex::new(vec![]),
            id,
            rid: None,
            stream_id,

            state: Mutex::new(TrackState::new()),
            rtp_cache: Arc::new(PCacheBuffer::new(
                Duration::from_millis(TTL_MILLIS),
                CAPACITY,
            )),

            pli_last_ms: AtomicU64::new(0),
            pli_interval_ms: 500,
        }
    }

    /// returns a TrackLocalStaticRTP with rid.
    pub fn new_with_rid(
        codec: RTCRtpCodecCapability,
        id: String,
        rid: String,
        stream_id: String,
    ) -> Self {
        TrackLocalStaticRTP {
            codec,
            bindings: Mutex::new(vec![]),
            id,
            rid: Some(rid),
            stream_id,

            state: Mutex::new(TrackState::new()),
            rtp_cache: Arc::new(PCacheBuffer::new(
                Duration::from_millis(TTL_MILLIS),
                CAPACITY,
            )),

            pli_last_ms: AtomicU64::new(0),
            pli_interval_ms: 500,
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

    pub async fn is_binding_active(&self, binding_ssrc: u32) -> bool {
        match {
            let bindings = self.bindings.lock().await;
            bindings
                .iter()
                .find(|b| b.ssrc == binding_ssrc)
                .map(|b| b.clone())
        } {
            Some(b) => !b.is_sender_paused(),
            None => false,
        }
    }

    /// Выполняется, когда мы изменяем источник данных для трека
    pub async fn replace_remote(self: Arc<Self>, remote_track: Arc<TrackRemote>) {
        // 1. Приводим исходящее смещение к начальному состоянию,
        // чтоб определить его заново в момент первого пришедшего пакета
        {
            let mut s = self.state.lock().await;
            s.out_offset = None;
        }

        // 2. Запись из mpsc канала в local_track
        // здесь должен быть минимальный буфер,
        // т.к. лучше потом отправить из кеша, чем пытаться отправить застрявший пакет из очереди
        let (rtp_sender, mut rtp_rx) = mpsc::channel::<Packet>(64);
        let local_track = Arc::downgrade(&self);
        let rtp_writer = tokio::spawn(async move {
            while let Some(pkt) = rtp_rx.recv().await {
                if let Some(local_track) = local_track.upgrade() {
                    if let Err(err) = local_track.write_rtp(&pkt).await {
                        eprintln!("Ошибка записи данных в исходящий трек: {:?}", err);
                    }
                } else {
                    break;
                }
            }
            println!("Запись данных в трек остановлена!");
        });

        // 3. Чтение из remote_track в mpsc канал
        while let Ok(rtp) = remote_track.read_rtp().await {
            // 1. Сохраняем в кэш оригинальный rtp без смещений! Так быстрее происходит сохранение в кэш
            // При восстановлении кеша нужно вернуть порядковый номер к оригинальному, чтоб найти его
            self.rtp_cache.put(rtp.clone());

            // 2. Пытаемся отправить, если переполнен буфер, не ждём и позже в ответ на NACK берём из кэша
            // Без ожиданий, чтоб не замедлять процесс получения пакетов
            match rtp_sender.try_send(rtp) {
                Err(TrySendError::Closed(_)) => {
                    break;
                }
                Err(TrySendError::Full(_)) => {
                    eprintln!("Ошибка отправки RTP данных: Буфер переполнен");
                }
                _ => {}
            }
        }

        // 4. Если remote_track перестал слать пакеты, то перестаём и записывать их
        rtp_writer.abort();
    }

    /// Получаем ssrc всех RTCPeerConnection подключений к этому треку
    pub async fn bindings_ssrc(&self) -> Vec<u32> {
        let bindings = self.bindings.lock().await;
        bindings.iter().map(|b| b.ssrc).collect()
    }

    pub async fn bindings_ids(&self) -> Vec<String> {
        let bindings = self.bindings.lock().await;
        bindings.iter().map(|b| b.id.clone()).collect()
    }

    pub async fn write_rtp_with_extensions_to(
        &self,
        p: &rtp::packet::Packet,
        extensions: &[rtp::extension::HeaderExtension],
        binding_ssrc: u32,
    ) -> Result<usize> {
        let binding = {
            let bindings = self.bindings.lock().await;
            bindings
                .iter()
                .find(|b| b.ssrc == binding_ssrc)
                .map(|b| b.clone())
        };

        if let Some(b) = binding {
            // Prepare the extensions data
            let mut extension_error = None;
            let extension_data: HashMap<_, _> = extensions
                .iter()
                .flat_map(|extension| {
                    let buf = {
                        let mut buf = BytesMut::with_capacity(extension.marshal_size());
                        buf.resize(extension.marshal_size(), 0);
                        if let Err(err) = extension.marshal_to(&mut buf) {
                            extension_error = Some(Error::Util(err));
                            return None;
                        }

                        buf.freeze()
                    };

                    Some((extension.uri(), buf))
                })
                .collect();
            if let Some(err) = extension_error {
                return Err(err);
            }

            self.write_rtp_with_extensions_to_binding(p, &extension_data, b)
                .await
        } else {
            // Must return Ok(usize) to be consistent with write_rtp_with_extensions_attributes
            Err(Error::LocalTrackBindingNotFound)
        }
    }

    pub async fn write_rtp_with_extensions(
        &self,
        pkt: &rtp::packet::Packet,
        extensions: &[rtp::extension::HeaderExtension],
    ) -> Result<usize> {
        let mut pkt = pkt.clone();

        let (seq_number, timestamp) = {
            let mut st = self.state.lock().await;
            st.apply_offset(
                self.kind(),
                pkt.header.sequence_number,
                pkt.header.timestamp,
            )
        };

        pkt.header.sequence_number = seq_number;
        pkt.header.timestamp = timestamp;

        let mut n = 0;
        let mut write_errs = vec![];

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
            match self
                .write_rtp_with_extensions_to_binding(&pkt, &extension_data, b)
                .await
            {
                Ok(one_or_zero) => {
                    n += one_or_zero;
                }
                Err(err) => {
                    write_errs.push(err);
                }
            }
        }

        flatten_errs(write_errs)?;
        Ok(n)
    }

    pub async fn write_rtp_to(
        &self,
        pkt: &rtp::packet::Packet,
        binding_ssrc: u32,
    ) -> Result<usize> {
        let mut pkt = pkt.clone();

        let (seq_number, timestamp) = {
            let mut st = self.state.lock().await;
            st.apply_offset(
                self.kind(),
                pkt.header.sequence_number,
                pkt.header.timestamp,
            )
        };

        pkt.header.sequence_number = seq_number;
        pkt.header.timestamp = timestamp;
        self.write_rtp_with_extensions_to(&pkt, &[], binding_ssrc)
            .await
    }

    pub async fn set_muted(&self, muted: bool) {
        let bindings = {
            let bindings = self.bindings.lock().await;
            bindings.clone()
        };
        bindings.iter().for_each(|b| {
            b.set_sender_paused(muted);
        });
    }

    pub async fn set_muted_for(&self, bindings_ssrc: Vec<(u32, bool)>) {
        let bindings = {
            let bindings = self.bindings.lock().await;
            bindings.clone()
        };
        bindings.iter().for_each(|b| {
            if let Some((_, muted)) = bindings_ssrc.iter().find(|(ssrc, _)| *ssrc == b.ssrc) {
                b.set_sender_paused(*muted);
            }
        });
    }

    pub fn should_fire_pli(&self, now_ms: u64) -> bool {
        loop {
            let prev = self.pli_last_ms.load(Ordering::Relaxed);
            if now_ms.saturating_sub(prev) < self.pli_interval_ms {
                return false;
            }
            if self
                .pli_last_ms
                .compare_exchange(prev, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
            // кто-то другой успел обновить last_ms — пробуем снова
        }
    }

    async fn write_rtp_with_extensions_to_binding(
        &self,
        p: &rtp::packet::Packet,
        extension_data: &HashMap<Cow<'static, str>, Bytes>,
        binidng: Arc<TrackBinding>,
    ) -> Result<usize> {
        let mut pkt = p.clone();

        if binidng.is_sender_paused() {
            // See caveat in function doc.
            return Ok(0);
        }
        pkt.header.ssrc = binidng.ssrc;
        pkt.header.payload_type = binidng.payload_type;

        for ext in binidng.hdr_ext_ids.iter() {
            let payload = ext.payload.to_owned();
            if let Err(err) = pkt.header.set_extension(ext.id, payload) {
                return Err(Error::Rtp(err));
            }
        }

        for (uri, data) in extension_data.iter() {
            if let Some(id) = binidng
                .params
                .header_extensions
                .iter()
                .find(|ext| &ext.uri == uri)
                .map(|ext| ext.id)
            {
                if let Err(err) = pkt.header.set_extension(id as u8, data.clone()) {
                    return Err(Error::Rtp(err));
                }
            }
        }

        binidng.write_stream.write_rtp(&pkt).await
    }
}

#[async_trait]
impl TrackLocal for TrackLocalStaticRTP {
    /// bind is called by the PeerConnection after negotiation is complete
    /// This asserts that the code requested is supported by the remote peer.
    /// If so it setups all the state (SSRC and PayloadType) to have a call
    async fn bind(&self, t: &TrackLocalContext) -> Result<RTCRtpCodecParameters> {
        println!(
            "TrackLocalStaticRTP.bind: mid={:?}; ssrc={:?}",
            t.mid(),
            t.ssrc()
        );
        let parameters = RTCRtpCodecParameters {
            capability: self.codec.clone(),
            ..Default::default()
        };
        let mut hdr_ext_ids = vec![];
        if let Some(id) = t
            .header_extensions()
            .iter()
            .find(|e| e.uri == ::sdp::extmap::SDES_MID_URI)
            .map(|e| e.id as u8)
        {
            if let Some(payload) = t
                .mid
                .as_ref()
                .map(|mid| Bytes::copy_from_slice(mid.as_bytes()))
            {
                hdr_ext_ids.push(rtp::header::Extension { id, payload });
            }
        }

        if let Some(id) = t
            .header_extensions()
            .iter()
            .find(|e| e.uri == ::sdp::extmap::SDES_RTP_STREAM_ID_URI)
            .map(|e| e.id as u8)
        {
            if let Some(payload) = self.rid().map(|rid| rid.to_owned().into()) {
                hdr_ext_ids.push(rtp::header::Extension { id, payload });
            }
        }

        let (codec, match_type) = codec_parameters_fuzzy_search(&parameters, t.codec_parameters());
        if match_type != CodecMatch::None {
            {
                let mut bindings = self.bindings.lock().await;
                bindings.push(Arc::new(TrackBinding {
                    id: t.id(),
                    ssrc: t.ssrc(),
                    payload_type: codec.payload_type,
                    params: t.params.clone(),
                    write_stream: t.write_stream(),
                    sender_paused: t.paused.clone(),
                    hdr_ext_ids,
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
        println!(
            "TrackLocalStaticRTP.unbind: mid={:?}; ssrc={:?}",
            t.mid(),
            t.ssrc()
        );
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

    /// RID is the RTP Stream ID for this track.
    fn rid(&self) -> Option<&str> {
        self.rid.as_deref()
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
    /// `write_rtp` writes a RTP Packet to the TrackLocalStaticRTP
    /// If one PeerConnection fails the packets will still be sent to
    /// all PeerConnections. The error message will contain the ID of the failed
    /// PeerConnections so you can remove them
    ///
    /// If the RTCRtpSender direction is such that no packets should be sent, any call to this
    /// function are blocked internally. Care must be taken to not increase the sequence number
    /// while the sender is paused. While the actual _sending_ is blocked, the receiver will
    /// miss out when the sequence number "rolls over", which in turn will break SRTP.
    async fn write_rtp(&self, pkt: &rtp::packet::Packet) -> Result<usize> {
        self.write_rtp_with_extensions(pkt, &[]).await
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
